use mewcode_protocol::{Message, MessagePart, ModelId, ToolCall, ToolResult};

use super::super::model::{SessionState, StreamMsg, StreamingState, Toast, ToolCallView, TurnItem};

/// Fold one SSE sub-message into the in-flight turn.
///
/// Returns `Some(Toast)` to raise on terminal failure, otherwise `None`. Events
/// that arrive with no [`StreamingState`] are ignored. On `Finished` exactly
/// one assistant message is committed and `streaming` returns to `None`; on
/// `Failed` the partial buffer is discarded and history is kept.
pub(super) fn apply_stream_event(s: &mut SessionState, ev: StreamMsg) -> Option<Toast> {
    match ev {
        StreamMsg::Started(id) => {
            if let Some(st) = &mut s.streaming {
                st.assistant_id = id;
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
        StreamMsg::Finished { .. } => {
            if let Some(st) = s.streaming.take() {
                if let Some(session) = s.session.as_mut() {
                    let model = session.model;
                    session.messages.push(commit_assistant_message(st, model));
                }
            }
            None
        }
        StreamMsg::Failed(e) => {
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
        }
    }
    Message::assistant(parts, model.as_str())
}
