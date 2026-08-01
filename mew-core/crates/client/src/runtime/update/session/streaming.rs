//! Applies streaming SSE events (`StreamMsg`) to the session state: folding
//! text and tool-call deltas into the in-flight turn, then committing the
//! finished assistant message.

use mewcode_protocol::event::ErrorCode;
use mewcode_protocol::{Message, MessagePart, ModelId, ToolCall, ToolResult};

use crate::runtime::model::{
    CompactionEntry, CompactionView, SessionState, StreamMsg, StreamingState, Toast, ToolCallView,
    TurnItem,
};

/// The client-owned recovery suggestion for a failed stream, derived from the
/// server's structured error code. The client decides the *presentation*
/// policy; the server's `retryable` flag only gates the generic retry hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NextAction {
    /// No recovery hint beyond the error message.
    None,
    /// The user must supply an API key.
    ConnectProvider,
    /// Retrying the same request may succeed (chat turn: resubmit the message).
    Retry,
    /// A compaction failed; the user must re-run `/compact` — resubmitting a
    /// message would not re-trigger compaction.
    RetryCompact,
    /// The model context is full; compact before retrying.
    SuggestCompact,
}

/// Map a structured stream error onto the user-facing recovery action.
pub(crate) fn next_action(code: ErrorCode, retryable: bool) -> NextAction {
    match code {
        ErrorCode::MissingApiKey => NextAction::ConnectProvider,
        ErrorCode::Upstream => {
            if retryable {
                NextAction::Retry
            } else {
                NextAction::None
            }
        }
        ErrorCode::ContextOverflow => NextAction::SuggestCompact,
        ErrorCode::CompactionFailed => NextAction::RetryCompact,
        ErrorCode::SessionNotFound
        | ErrorCode::BadRequest
        | ErrorCode::ToolFailed
        | ErrorCode::Internal => NextAction::None,
    }
}

/// Build the failure toast: the sanitised server message plus a hint derived
/// from the recovery action, if any.
pub(crate) fn failure_toast(message: &str, code: ErrorCode, retryable: bool) -> Toast {
    let hint = match next_action(code, retryable) {
        NextAction::None => String::new(),
        NextAction::ConnectProvider => " — run /connect to add a key".into(),
        NextAction::Retry => " — resubmit the message to retry".into(),
        NextAction::RetryCompact => " — run /compact to retry".into(),
        NextAction::SuggestCompact => " — run /compact to free context".into(),
    };
    Toast::error(format!("{message}{hint}"))
}

/// Fold one SSE sub-message into the in-flight turn.
///
/// Returns `Some(Toast)` to raise on terminal failure, otherwise `None`. Events
/// that arrive with no [`StreamingState`] are ignored. On `Finished` exactly
/// one assistant message is committed and `streaming` returns to `None`; on
/// `Failed` the partial buffer is discarded and history is kept. `Aborted` is
/// terminal but not an error: the buffer is discarded without a toast.
pub(crate) fn apply_stream_event(s: &mut SessionState, ev: StreamMsg) -> Option<Toast> {
    match ev {
        StreamMsg::Started { id, pwd } => {
            if let Some(st) = &mut s.streaming {
                st.assistant_id = id;
            }
            if let Some(p) = pwd {
                s.pwd = Some(p);
            }
            None
        }
        StreamMsg::Delta(delta) => {
            if let Some(st) = &mut s.streaming {
                st.push_text(&delta);
            }
            None
        }
        StreamMsg::ToolInput { id, name, input } => {
            if let Some(st) = &mut s.streaming {
                st.push_tool_call(ToolCallView {
                    id,
                    name,
                    input,
                    output: None,
                    display: None,
                });
            }
            None
        }
        StreamMsg::ToolOutput { id, output } => {
            if let Some(st) = &mut s.streaming {
                if let Some(call) = st.tool_mut(&id) {
                    call.output = Some(output);
                }
            }
            None
        }
        StreamMsg::ToolDisplay { id, display } => {
            if let Some(st) = &mut s.streaming {
                if let Some(call) = st.tool_mut(&id) {
                    call.display = Some(display);
                }
            }
            None
        }
        StreamMsg::ChoiceRequest(request) => {
            s.pending_choice = Some(crate::runtime::model::ChoicePromptState::new(request));
            s.overlay = crate::runtime::model::Overlay::Choice;
            None
        }
        StreamMsg::CompactionStarted => {
            // Compaction stream started — ensure we have a streaming state
            // so progress events can render inline.
            if s.streaming.is_none() {
                s.streaming = Some(StreamingState::new(uuid::Uuid::nil()));
            }
            None
        }
        StreamMsg::CompactionProgress { phase, message } => {
            match phase {
                mewcode_protocol::event::CompactionPhase::Pruning
                | mewcode_protocol::event::CompactionPhase::Summarizing => {
                    if let Some(st) = &mut s.streaming {
                        st.push_progress(&format!(" {message}\n"));
                    }
                }
                mewcode_protocol::event::CompactionPhase::Done => {}
            }
            None
        }
        StreamMsg::CompactionSummaryDelta(delta) => {
            if let Some(st) = &mut s.streaming {
                st.push_compaction_delta(&delta);
            }
            None
        }
        StreamMsg::Compacted {
            tokens_before,
            context_limit,
            summary,
            thought_duration_ms,
        } => {
            // Fill in metadata on the streamed summary; the Finished handler
            // commits it to `s.compaction.committed` to preserve ordering.
            if let Some(st) = &mut s.streaming {
                st.finish_compaction(tokens_before, context_limit, &summary, thought_duration_ms);
            }
            None
        }
        StreamMsg::Finished {
            duration_ms: _,
            session_tokens,
            context_limit,
        } => {
            let manual = s.compaction.active;
            if manual {
                s.compaction.active = false;
                s.compaction.started_at = None;
            }
            if let Some(tokens) = session_tokens {
                s.session_tokens = tokens;
            }
            if let Some(limit) = context_limit {
                s.context_limit = limit;
            }

            if let Some(st) = s.streaming.take() {
                if let Some(session) = s.session.as_mut() {
                    let (msg, views) = commit_turn(st, session.model);
                    // A compaction entry anchors to the message count before its
                    // reply is committed, so it renders above that reply. A turn
                    // that produced no content (e.g. a manual /compact, which
                    // streams no reply) commits no message, so its entry lands
                    // at the end of the transcript.
                    let anchor = session.messages.len();
                    if !msg.parts.is_empty() {
                        session.messages.push(msg);
                    }
                    for view in views {
                        s.compaction.committed.push(CompactionEntry {
                            after_message_count: anchor,
                            view,
                        });
                    }
                }
            }

            if manual {
                Some(Toast::info("context compacted"))
            } else {
                None
            }
        }
        StreamMsg::Failed {
            message,
            code,
            retryable,
        } => {
            // Clear compacting flag on failure to prevent user from being stuck.
            if s.compaction.active {
                s.compaction.active = false;
                s.compaction.started_at = None;
            }
            // Only react to a failure for a turn we are actually tracking.
            if s.streaming.take().is_some() {
                Some(failure_toast(&message, code, retryable))
            } else {
                None
            }
        }
        StreamMsg::Aborted => {
            if s.compaction.active {
                s.compaction.active = false;
                s.compaction.started_at = None;
            }
            // Abort is a terminal, expected outcome: drop the partial buffer
            // silently instead of surfacing an error toast.
            s.streaming.take();
            None
        }
    }
}

/// Commit a finished turn in a single pass: split its ordered items into the
/// committed assistant message (text and tool parts, in stream order) and any
/// inline compaction views.
fn commit_turn(st: StreamingState, model: ModelId) -> (Message, Vec<CompactionView>) {
    let mut parts: Vec<MessagePart> = Vec::new();
    let mut views: Vec<CompactionView> = Vec::new();
    for item in st.items {
        match item {
            TurnItem::Text(text) => {
                if !text.is_empty() {
                    parts.push(MessagePart::Text { text });
                }
            }
            TurnItem::Tool(ToolCallView {
                id,
                name,
                input,
                output,
                display,
            }) => {
                parts.push(MessagePart::ToolCall(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                }));
                if let Some(output) = output {
                    parts.push(MessagePart::ToolResult(ToolResult {
                        call_id: id,
                        name,
                        output,
                        is_error: false,
                        display,
                    }));
                }
            }
            TurnItem::Compaction(view) => views.push(view),
            TurnItem::Progress(_) => {}
        }
    }
    (Message::assistant(parts, model.as_str()), views)
}
