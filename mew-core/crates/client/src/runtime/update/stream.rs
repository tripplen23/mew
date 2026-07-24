use mewcode_protocol::{Message, MessagePart, ModelId, ToolCall, ToolResult};

use super::super::model::{
    CompactionEntry, CompactionView, SessionState, StreamMsg, StreamingState, Toast, ToolCallView,
    TurnItem,
};

/// Fold one SSE sub-message into the in-flight turn.
///
/// Returns `Some(Toast)` to raise on terminal failure, otherwise `None`. Events
/// that arrive with no [`StreamingState`] are ignored. On `Finished` exactly
/// one assistant message is committed and `streaming` returns to `None`; on
/// `Failed` the partial buffer is discarded and history is kept.
pub(super) fn apply_stream_event(s: &mut SessionState, ev: StreamMsg) -> Option<Toast> {
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
            s.pending_choice = Some(super::super::model::ChoicePromptState::new(request));
            s.overlay = super::super::model::Overlay::Choice;
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
            // Only render Pruning and Summarizing progress as text.
            // Skip "Done" message — the Compaction section indicates completion.
            match phase {
                mewcode_protocol::event::CompactionPhase::Pruning
                | mewcode_protocol::event::CompactionPhase::Summarizing => {
                    if let Some(st) = &mut s.streaming {
                        st.push_text(&format!(" {message}\n"));
                    }
                }
                mewcode_protocol::event::CompactionPhase::Done => {
                    // Don't clear compacting flag here — Finish handler needs it
                    // to know this was a compaction stream. Safety timeout in Tick.
                }
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
            // Fill in metadata on the summary already streamed in via
            // `CompactionSummaryDelta` — will be committed to
            // s.compaction.committed in the Finished handler to preserve
            // correct ordering.
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
            // If we were compacting, commit the streaming state and extract
            // any compaction entry to preserve correct ordering.
            if s.compaction.active {
                s.compaction.active = false;
                s.compaction.started_at = None;
                if let Some(tokens) = session_tokens {
                    s.session_tokens = tokens;
                }
                if let Some(limit) = context_limit {
                    s.context_limit = limit;
                }
                // Commit streaming state: extract compaction view first,
                // then commit assistant message (text only).
                if let Some(st) = s.streaming.take() {
                    // Extract compaction view from items (if any).
                    let compaction_view = st.items.iter().find_map(|item| {
                        if let TurnItem::Compaction(view) = item {
                            Some(view.clone())
                        } else {
                            None
                        }
                    });
                    // Commit assistant message (text/tool only).
                    if let Some(session) = s.session.as_mut() {
                        let model = session.model;
                        let msg = commit_assistant_message(st, model);
                        if !msg.parts.is_empty() {
                            session.messages.push(msg);
                        }
                        // Push compaction entry AFTER assistant message
                        // so it renders in correct order in transcript.
                        if let Some(view) = compaction_view {
                            let msg_count = session.messages.len();
                            s.compaction.committed.push(CompactionEntry {
                                after_message_count: msg_count,
                                view,
                            });
                        }
                    }
                }
                return Some(Toast::info("context compacted"));
            }
            // Auto-compaction can fire mid-chat. Extract Compaction entries
            // before commit — otherwise they render live then silently vanish.
            if let Some(st) = s.streaming.take() {
                let compaction_views: Vec<CompactionView> = st
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        TurnItem::Compaction(view) => Some(view.clone()),
                        _ => None,
                    })
                    .collect();
                if let Some(session) = s.session.as_mut() {
                    let model = session.model;
                    // Compaction runs before reply — record above it in
                    // causal order. Assistant message always committed here
                    // (unlike manual-compaction branch above).
                    let before_count = session.messages.len();
                    session.messages.push(commit_assistant_message(st, model));
                    for view in compaction_views {
                        s.compaction.committed.push(CompactionEntry {
                            after_message_count: before_count,
                            view,
                        });
                    }
                }
            }
            if let Some(tokens) = session_tokens {
                s.session_tokens = tokens;
            }
            if let Some(limit) = context_limit {
                s.context_limit = limit;
            }
            None
        }
        StreamMsg::Failed(e) => {
            // Clear compacting flag on failure to prevent user from being stuck.
            if s.compaction.active {
                s.compaction.active = false;
                s.compaction.started_at = None;
            }
            // Only react to a failure for a turn we are actually tracking.
            if s.streaming.take().is_some() {
                Some(Toast::error(format!("stream failed: {e}")))
            } else {
                None
            }
        }
    }
}

/// Assemble the committed assistant message from the turn's ordered items, so
/// text and tool parts land in the transcript in the exact order the runtime
/// streamed them (`text -> tool -> text -> ...`), each tool call immediately
/// followed by its result.
fn commit_assistant_message(st: StreamingState, model: ModelId) -> Message {
    let mut parts: Vec<MessagePart> = Vec::new();
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
            TurnItem::Compaction(_) => {}
        }
    }
    Message::assistant(parts, model.as_str())
}
