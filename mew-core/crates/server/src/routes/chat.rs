//! `POST /chat` — accept a `ChatRequest`, stream `StreamEvent`s back as SSE.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::sse::Sse;
use futures::stream::Stream;
use mewcode_engine::{
    Harness,
    skills::SkillRegistry,
    tools::{ProjectContext, default_registry},
};
use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart, Role, StreamEvent};
use std::convert::Infallible;

use crate::AppState;
use crate::sse::from_channel;

/// `POST /chat` — stream a chat turn. The response is `text/event-stream`;
/// each `data:` line is a JSON [`StreamEvent`].
///
/// The turn is also **persisted** to the session store: the user's new message
/// is appended up front (so it survives even if the turn fails), and the
/// assistant's reply is appended once the turn finishes. A forwarder task sits
/// between the harness and the SSE channel so persistence is a pure side effect
/// of the events the client already receives — the harness stays unaware of the
/// store, and the wire protocol is unchanged.
#[utoipa::path(
    post,
    path = "/chat",
    tag = "chat",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "SSE stream of StreamEvent", body = StreamEvent, content_type = "text/event-stream"),
    ),
)]
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    // Two channels: the harness produces on `htx`; a forwarder relays to `stx`
    // (the SSE output) while persisting the turn as it streams.
    let (htx, mut hrx) = tokio::sync::mpsc::channel::<StreamEvent>(64);
    let (stx, srx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

    // Load skills from default + configured external dirs. Missing dirs
    // silently skipped.
    let skill_cfg = mewcode_engine::skills::SkillLoadConfig {
        bundled_dir: None,
        external_dirs: state.config.skills.resolved_dirs(),
        project_search_start: std::env::current_dir().ok(),
        include_dev_dir: true,
    };
    let skills = Arc::new(SkillRegistry::load(&skill_cfg));
    // Tool registry: read-only + memory + skill_view always; write tools
    // in Build mode. Root defaults to server CWD.
    let root = std::env::current_dir()
        .or_else(|_| std::fs::canonicalize("."))
        .unwrap_or_else(|_| ".".into());
    // Shared display sink: mutating tools drop render-only diffs here during
    // execution; the stream layer correlates them to tool calls and emits
    // `ToolDisplayAvailable`. Never enters the model's context.
    let display_sink: mewcode_engine::tools::DisplaySink =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let ctx = ProjectContext::new(root.clone()).with_display(display_sink.clone());
    // Per-project memory scope — prevents facts leaking across projects.
    // Falls back to global profile if data dir can't be resolved.
    let memory = match state.memory.data_dir() {
        Some(data_dir) => {
            mewcode_engine::memory::MemoryStore::for_project(data_dir.to_path_buf(), &root)
        }
        None => state.memory.clone(),
    };
    let tools = Arc::new(default_registry(
        ctx,
        skills.clone(),
        Some(memory.clone()),
        req.mode,
    ));

    // Load accumulated session tokens from the shared map.
    let prior_tokens = {
        let map = state.session_tokens.read().await;
        map.get(&req.session_id).copied().unwrap_or(0)
    };

    // Load the compaction summary and boundary from the session, if a prior
    // manual or automatic compaction ran. Both are needed together: the
    // boundary tells the harness which leading messages the summary already
    // covers, so those are not re-sent to the model this turn.
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
        // Persist the user's new message first so it survives even if the turn
        // fails partway through.
        if let Some(message) = new_user_message {
            if let Err(e) = store.append_message(session_id, message).await {
                tracing::warn!(error = %e, "failed to persist user message");
            }
        }

        // Relay every event to the SSE channel, accumulating the assistant
        // reply. Draining continues even if the client disconnects, so the full
        // turn is still persisted.
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

        // A finished turn commits the assistant message (mirroring the client's
        // own commit-on-finish). A failed turn emits no `Finish`, so nothing is
        // persisted for the assistant side. A turn whose model produced no text
        // is also not persisted: the user would see an empty assistant bubble,
        // which is worse than a missing one.
        if finished && !reply.is_empty() {
            let message =
                Message::assistant(vec![MessagePart::Text { text: reply }], model.as_str());
            if let Err(e) = store.append_message(session_id, message).await {
                tracing::warn!(error = %e, "failed to persist assistant message");
            }
        }
    });

    from_channel(srx)
}
