//! Streaming execution for a Rig agent turn.
//!
//! Bridges Rig's [`MultiTurnStreamItem`](rig_core::agent::MultiTurnStreamItem)
//! into mewcode's [`StreamEvent`](mewcode_protocol::StreamEvent) protocol.
//! Kept separate from [`super::Agent`] so the turn lifecycle and the wire
//! protocol don't tangle.

use futures::StreamExt;
use mewcode_protocol::StreamEvent;
use rig_core::agent::MultiTurnStreamItem;
use rig_core::streaming::{StreamedAssistantContent, StreamingPrompt};
use tokio::sync::mpsc;

use crate::error::EngineError;

/// Stream one Rig agent prompt to completion, emitting `TextDelta`,
/// `ToolInputAvailable`, and `ToolOutputAvailable` events through `tx`.
///
/// The multi-turn loop is handled by Rig internally; this function only
/// translates Rig stream items into mewcode events.
pub async fn run_agent_stream<M: rig_core::completion::CompletionModel + 'static>(
    agent: rig_core::agent::Agent<M, ()>,
    user_text: String,
    history: Vec<rig_core::completion::Message>,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<String, EngineError> {
    let mut stream = agent.stream_prompt(user_text).with_history(history).await;

    let mut full_reply = String::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(t))) => {
                let delta = t.text;
                let _ = tx
                    .send(StreamEvent::TextDelta {
                        delta: delta.clone(),
                    })
                    .await;
                full_reply.push_str(&delta);
            }
            Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall {
                tool_call,
                ..
            })) => {
                let _ = tx
                    .send(StreamEvent::ToolInputAvailable {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.function.name.clone(),
                        input: tool_call.function.arguments.clone(),
                    })
                    .await;
            }
            Ok(MultiTurnStreamItem::StreamUserItem(user_content)) => {
                // StreamedUserContent has a single variant (ToolResult), so we destructure directly.
                let rig_core::streaming::StreamedUserContent::ToolResult { tool_result, .. } =
                    user_content;
                let output = tool_result
                    .content
                    .iter()
                    .find_map(|c| match c {
                        rig_core::completion::message::ToolResultContent::Text(t) => {
                            Some(t.text.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let parsed = serde_json::from_str::<serde_json::Value>(&output)
                    .unwrap_or(serde_json::Value::String(output));
                let _ = tx
                    .send(StreamEvent::ToolOutputAvailable {
                        tool_call_id: tool_result.id,
                        output: parsed,
                    })
                    .await;
            }
            Ok(MultiTurnStreamItem::CompletionCall(call)) => {
                if let Some(usage) = &call.usage {
                    tracing::debug!(
                        input_tokens = usage.input_tokens,
                        output_tokens = usage.output_tokens,
                        "completion call usage"
                    );
                }
            }
            Ok(MultiTurnStreamItem::FinalResponse(response)) => {
                if full_reply.is_empty() {
                    let text = response.response().to_string();
                    if !text.is_empty() {
                        let _ = tx
                            .send(StreamEvent::TextDelta {
                                delta: text.clone(),
                            })
                            .await;
                        full_reply = text;
                    }
                }
            }
            Err(e) => return Err(EngineError::Other(e.to_string())),
            Ok(_) => {
                tracing::trace!("unhandled MultiTurnStreamItem variant");
            }
        }
    }
    Ok(full_reply)
}
