//! Chat turn workflow: harness setup, streaming, persistence.

use std::sync::Arc;

use mewcode_engine::{
    Harness,
    skills::SkillRegistry,
    tools::{ProjectContext, default_registry},
};
use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart, Role, StreamEvent};
use tokio::sync::mpsc;

use crate::AppState;

use super::runtime::{project_memory, project_root};

#[doc(hidden)]
pub fn canonical_turn_messages(
    mut stored_messages: Vec<Message>,
    request_messages: &[Message],
) -> Result<(Vec<Message>, Message), &'static str> {
    let mut new_user = request_messages
        .last()
        .filter(|message| message.role == Role::User)
        .cloned()
        .ok_or("chat request must end with a user message")?;
    if stored_messages
        .iter()
        .any(|message| message.id == new_user.id)
    {
        return Err("chat request replays an existing message id");
    }

    // The server owns transcript ordering; do not let a stale client timestamp
    // move this new turn into the middle of persisted history on the next load.
    new_user.created_at = chrono::Utc::now();
    stored_messages.push(new_user.clone());
    Ok((stored_messages, new_user))
}

async fn persist_checkpoint(
    store: &dyn crate::store::SessionStore,
    session_id: uuid::Uuid,
    checkpoint: Option<(&str, usize, uuid::Uuid)>,
) -> Result<(), crate::store::StoreError> {
    if let Some((summary, compacted_up_to, compacted_up_to_message_id)) = checkpoint {
        store
            .patch_session(
                session_id,
                crate::store::SessionPatch {
                    compaction_summary: Some(summary.to_owned()),
                    compacted_up_to: Some(compacted_up_to),
                    compacted_up_to_message_id: Some(compacted_up_to_message_id),
                    ..Default::default()
                },
            )
            .await?;
    }
    Ok(())
}

#[doc(hidden)]
pub const GENERIC_CHAT_ERROR: &str = "chat turn failed";

#[doc(hidden)]
pub fn try_forward_event(client: &mut Option<mpsc::Sender<StreamEvent>>, event: StreamEvent) {
    let Some(sender) = client else {
        return;
    };
    if sender.try_send(event).is_err() {
        *client = None;
    }
}

#[doc(hidden)]
pub fn stage_harness_event(
    event: StreamEvent,
    reply: &mut String,
    assistant_message_id: &mut Option<uuid::Uuid>,
    finish: &mut Option<StreamEvent>,
    engine_failed: &mut bool,
    client: &mut Option<mpsc::Sender<StreamEvent>>,
) {
    match &event {
        StreamEvent::Start { message_id, .. } => *assistant_message_id = Some(*message_id),
        StreamEvent::TextDelta { delta } => reply.push_str(delta),
        StreamEvent::Finish { .. } => {
            *finish = Some(event);
            return;
        }
        StreamEvent::Error { message } => {
            tracing::error!(%message, "harness stream error");
            *engine_failed = true;
            return;
        }
        _ => {}
    }
    try_forward_event(client, event);
}

#[derive(Debug)]
pub enum CommitTurnError {
    MissingAssistantMessageId,
    Store(crate::store::StoreError),
}

impl From<crate::store::StoreError> for CommitTurnError {
    fn from(error: crate::store::StoreError) -> Self {
        Self::Store(error)
    }
}

#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub async fn commit_successful_turn(
    store: &dyn crate::store::SessionStore,
    session_tokens: &tokio::sync::RwLock<std::collections::HashMap<uuid::Uuid, u64>>,
    session_id: uuid::Uuid,
    model: mewcode_protocol::ModelId,
    reply: String,
    assistant_message_id: Option<uuid::Uuid>,
    finish: StreamEvent,
    context_tokens: u64,
    checkpoint: Option<(&str, usize, uuid::Uuid)>,
) -> Result<StreamEvent, CommitTurnError> {
    let message_id = assistant_message_id.ok_or(CommitTurnError::MissingAssistantMessageId)?;
    persist_checkpoint(store, session_id, checkpoint).await?;
    store
        .append_message(
            session_id,
            Message {
                id: message_id,
                role: Role::Assistant,
                parts: vec![MessagePart::Text { text: reply }],
                model: Some(model.as_str().to_owned()),
                created_at: chrono::Utc::now(),
            },
        )
        .await?;
    session_tokens
        .write()
        .await
        .insert(session_id, context_tokens);
    Ok(finish)
}

pub(crate) async fn start_chat_stream(
    state: AppState,
    req: ChatRequest,
) -> mpsc::Receiver<StreamEvent> {
    let (stx, srx) = mpsc::channel::<StreamEvent>(64);

    tokio::spawn(async move {
        let mut client = Some(stx);
        let session_id = req.session_id;
        let operation_lock = match state.existing_session_operation_lock(session_id).await {
            Ok(lock) => lock,
            Err(crate::store::StoreError::NotFound) => {
                try_forward_event(
                    &mut client,
                    StreamEvent::Error {
                        message: "session not found".into(),
                    },
                );
                return;
            }
            Err(error) => {
                tracing::error!(%error, %session_id, "failed to load session operation lock");
                try_forward_event(
                    &mut client,
                    StreamEvent::Error {
                        message: GENERIC_CHAT_ERROR.into(),
                    },
                );
                return;
            }
        };
        let _operation_guard = operation_lock.lock().await;

        let session = match state.store.get_session(session_id).await {
            Ok(session) => session,
            Err(crate::store::StoreError::NotFound) => {
                try_forward_event(
                    &mut client,
                    StreamEvent::Error {
                        message: "session not found".into(),
                    },
                );
                return;
            }
            Err(error) => {
                tracing::error!(%error, %session_id, "failed to reload session");
                try_forward_event(
                    &mut client,
                    StreamEvent::Error {
                        message: GENERIC_CHAT_ERROR.into(),
                    },
                );
                return;
            }
        };
        let (messages, new_user_message) =
            match canonical_turn_messages(session.messages.clone(), &req.messages) {
                Ok(turn) => turn,
                Err(message) => {
                    try_forward_event(
                        &mut client,
                        StreamEvent::Error {
                            message: message.into(),
                        },
                    );
                    return;
                }
            };
        if let Err(error) = state
            .store
            .append_message(session_id, new_user_message)
            .await
        {
            tracing::error!(%error, %session_id, "failed to persist user message");
            try_forward_event(
                &mut client,
                StreamEvent::Error {
                    message: GENERIC_CHAT_ERROR.into(),
                },
            );
            return;
        }

        let skills = {
            let cfg = mewcode_engine::skills::SkillLoadConfig {
                bundled_dir: None,
                external_dirs: state.config.skills.resolved_dirs(),
                project_search_start: std::env::current_dir().ok(),
                include_dev_dir: true,
            };
            Arc::new(SkillRegistry::load(&cfg))
        };
        let root = project_root();
        let display_sink: mewcode_engine::tools::DisplaySink =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let ctx = ProjectContext::new(root.clone()).with_display(display_sink.clone());
        let memory = project_memory(&state.memory, &root);
        let tools = Arc::new(default_registry(
            ctx,
            skills.clone(),
            Some(memory.clone()),
            req.mode,
        ));
        let prior_tokens = state
            .session_tokens
            .read()
            .await
            .get(&session_id)
            .copied()
            .unwrap_or(0);
        let mut harness = Harness::new(req.model, req.mode, skills, tools)
            .with_session(session_id)
            .with_project_root(root)
            .with_memory(memory)
            .with_display_sink(display_sink)
            .with_approval_broker(state.approvals.clone())
            .with_session_tokens(prior_tokens)
            .with_engine_config(build_engine_config(&state).await)
            .with_compaction_summary(
                session.compaction_summary,
                session.compacted_up_to.unwrap_or(0),
                session.compacted_up_to_message_id,
            );

        let (htx, mut hrx) = mpsc::channel::<StreamEvent>(64);
        let worker = tokio::spawn(async move {
            let result = harness.run_turn(&messages, htx).await;
            (result, harness)
        });

        let mut reply = String::new();
        let mut finish = None;
        let mut assistant_message_id = None;
        let mut engine_failed = false;
        while let Some(event) = hrx.recv().await {
            // ponytail: The public channel has a 64-event burst ceiling; a
            // stalled client loses the rest. Upgrade to a detached bounded
            // spool with cancellation if lossless slow-client delivery matters.
            stage_harness_event(
                event,
                &mut reply,
                &mut assistant_message_id,
                &mut finish,
                &mut engine_failed,
                &mut client,
            );
        }

        let (result, harness) = match worker.await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::error!(%error, %session_id, "chat worker failed");
                try_forward_event(
                    &mut client,
                    StreamEvent::Error {
                        message: GENERIC_CHAT_ERROR.into(),
                    },
                );
                return;
            }
        };

        if let Err(error) = result {
            tracing::error!(?error, %session_id, "harness error");
            try_forward_event(
                &mut client,
                StreamEvent::Error {
                    message: GENERIC_CHAT_ERROR.into(),
                },
            );
            return;
        }
        if engine_failed {
            try_forward_event(
                &mut client,
                StreamEvent::Error {
                    message: GENERIC_CHAT_ERROR.into(),
                },
            );
            return;
        }

        let Some(finish) = finish else {
            tracing::error!(%session_id, "successful chat turn ended without Finish");
            try_forward_event(
                &mut client,
                StreamEvent::Error {
                    message: GENERIC_CHAT_ERROR.into(),
                },
            );
            return;
        };
        match commit_successful_turn(
            state.store.as_ref(),
            state.session_tokens.as_ref(),
            session_id,
            req.model,
            reply,
            assistant_message_id,
            finish,
            harness.session_tokens(),
            harness.updated_compaction(),
        )
        .await
        {
            Ok(finish) => try_forward_event(&mut client, finish),
            Err(error) => {
                match &error {
                    CommitTurnError::Store(store_error) => {
                        tracing::error!(%store_error, %session_id, "failed to commit chat turn");
                    }
                    CommitTurnError::MissingAssistantMessageId => {
                        tracing::error!(%session_id, "chat reply had no assistant message id");
                    }
                }
                try_forward_event(
                    &mut client,
                    StreamEvent::Error {
                        message: GENERIC_CHAT_ERROR.into(),
                    },
                );
            }
        }
    });

    srx
}

/// Build an EngineConfig from the credential store (YAML), ServerConfig,
/// and environment variables, in priority order:
///   1. Credential store (YAML, from /connect TUI)
///   2. ServerConfig fields (from mewcode.toml or env)
///   3. Raw environment variables
async fn build_engine_config(state: &AppState) -> mewcode_engine::EngineConfig {
    let store = state.credentials.lock().await;
    let api_key = store
        .api_key(mewcode_protocol::ProviderId::OpenCodeGo)
        .or_else(|| state.config.opencode_go_api_key.clone())
        .or_else(|| std::env::var("OPENCODE_GO_API_KEY").ok())
        .unwrap_or_default();
    let openai_api_key = store
        .api_key(mewcode_protocol::ProviderId::OpenAi)
        .or_else(|| state.config.openai_api_key.clone())
        .or_else(|| std::env::var("OPENAI_API_KEY").ok());

    mewcode_engine::EngineConfig {
        api_key,
        openai_api_key,
        openai_base_url: None,
        default_model: state
            .config
            .default_model
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(mewcode_protocol::ModelId::DEFAULT),
        base_url: "https://opencode.ai/zen/go".to_string(),
    }
}
