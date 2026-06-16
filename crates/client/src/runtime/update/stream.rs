use mewcode_protocol::{Message, MessagePart, ModelId, ToolCall, ToolResult};

use super::super::model::{SessionState, StreamMsg, StreamingState, Toast, ToolCallView};

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
                st.buffer.push_str(&delta);
            }
            None
        }
        StreamMsg::ToolInput { id, name, input } => {
            if let Some(st) = &mut s.streaming {
                st.tool_calls.push(ToolCallView {
                    id,
                    name,
                    input,
                    output: None,
                });
            }
            None
        }
        StreamMsg::ToolOutput { id, output } => {
            if let Some(st) = &mut s.streaming {
                if let Some(call) = st.tool_calls.iter_mut().find(|c| c.id == id) {
                    call.output = Some(output);
                }
            }
            None
        }
        StreamMsg::Finished { .. } => {
            if let Some(st) = s.streaming.take() {
                let model = s.session.model;
                s.session.messages.push(commit_assistant_message(st, model));
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

/// Assemble the committed assistant message from the streaming buffer and tool
/// calls. Text comes first, then each tool call followed by its output, so the
/// arrival order of tool parts is preserved.
fn commit_assistant_message(st: StreamingState, model: ModelId) -> Message {
    let mut parts: Vec<MessagePart> = Vec::new();
    if !st.buffer.is_empty() {
        parts.push(MessagePart::Text { text: st.buffer });
    }
    for call in st.tool_calls {
        let ToolCallView {
            id,
            name,
            input,
            output,
        } = call;
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
            }));
        }
    }
    Message::assistant(parts, model.provider_id())
}
