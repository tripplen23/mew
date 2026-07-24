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

pub(crate) async fn start_chat_stream(
    state: AppState,
    req: ChatRequest,
) -> mpsc::Receiver<StreamEvent> {
    let (htx, mut hrx) = mpsc::channel::<StreamEvent>(64);
    let (stx, srx) = mpsc::channel::<StreamEvent>(64);

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

    let prior_tokens = {
        let map = state.session_tokens.read().await;
        map.get(&req.session_id).copied().unwrap_or(0)
    };

    let (compaction_summary, compacted_up_to) = state
        .store
        .get_session(req.session_id)
        .await
        .ok()
        .map(|s| (s.compaction_summary, s.compacted_up_to.unwrap_or(0)))
        .unwrap_or((None, 0));

    let mut harness = Harness::new(req.model, req.mode, skills, tools)
        .with_session(req.session_id)
        .with_project_root(root)
        .with_memory(memory)
        .with_display_sink(display_sink)
        .with_approval_broker(state.approvals.clone())
        .with_session_tokens(prior_tokens)
        .with_compaction_summary(compaction_summary, compacted_up_to);

    // Last User-role message from history. Role filter prevents malformed
    // trailing messages from being persisted as a user turn.
    let new_user_message = req
        .messages
        .last()
        .filter(|m| m.role == Role::User)
        .cloned();
    let session_id = req.session_id;
    let model = req.model;
    let store = state.store.clone();
    let messages = req.messages;

    let session_tokens = state.session_tokens.clone();
    let compact_store = state.store.clone();
    tokio::spawn(async move {
        // Handler owns error emission — a failed turn produces exactly one
        // Error and nothing after.
        if let Err(e) = harness.run_turn(&messages, htx.clone()).await {
            tracing::error!(error = ?e, "harness error");
            let _ = htx
                .send(StreamEvent::Error {
                    message: e.to_string(),
                })
                .await;
        }
        // Persist updated token total for compaction decisions on the next turn.
        let updated = harness.session_tokens();
        {
            let mut map = session_tokens.write().await;
            map.insert(session_id, updated);
        }
        // Persist compaction result so the next turn builds on the summary
        // rather than re-summarizing from scratch.
        if let Some((summary, compacted_up_to)) = harness.updated_compaction() {
            let patch = crate::store::SessionPatch {
                compaction_summary: Some(summary.to_string()),
                compacted_up_to: Some(compacted_up_to),
                ..Default::default()
            };
            if let Err(e) = compact_store.patch_session(session_id, patch).await {
                tracing::warn!(error = %e, "failed to persist automatic compaction state");
            }
        }
    });

    tokio::spawn(async move {
        if let Some(message) = new_user_message {
            if let Err(e) = store.append_message(session_id, message).await {
                tracing::warn!(error = %e, "failed to persist user message");
            }
        }

        let mut reply = String::new();
        let mut finished = false;
        while let Some(event) = hrx.recv().await {
            if let StreamEvent::TextDelta { delta } = &event {
                reply.push_str(delta);
            }
            if matches!(event, StreamEvent::Finish { .. }) {
                finished = true;
            }
            let _ = stx.send(event).await;
        }

        if finished && !reply.is_empty() {
            let message =
                Message::assistant(vec![MessagePart::Text { text: reply }], model.as_str());
            if let Err(e) = store.append_message(session_id, message).await {
                tracing::warn!(error = %e, "failed to persist assistant message");
            }
        }
    });

    srx
}
