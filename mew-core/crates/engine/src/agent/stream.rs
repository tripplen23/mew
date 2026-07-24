//! Streaming execution for a Rig agent turn.
//!
//! Bridges Rig's [`MultiTurnStreamItem`](rig_core::agent::MultiTurnStreamItem)
//! into mewcode's [`StreamEvent`](mewcode_protocol::StreamEvent) protocol.
//! Kept separate from [`super::Agent`] so the turn lifecycle and the wire
//! protocol don't tangle.

use std::collections::HashMap;

use futures::StreamExt;
use mewcode_protocol::StreamEvent;
use rig_core::agent::MultiTurnStreamItem;
use rig_core::streaming::{StreamedAssistantContent, StreamingPrompt};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::error::EngineError;
use crate::tools::DisplaySink;

/// Per-turn token usage accumulated from all completion calls in the turn.
#[derive(Debug, Clone, Copy, Default)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl TurnUsage {
    /// Total tokens consumed this turn (input + output + cache).
    pub fn total(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cached_input_tokens
            + self.cache_creation_input_tokens
    }

    /// Whether any usage was reported.
    pub fn is_empty(&self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.cached_input_tokens == 0
            && self.cache_creation_input_tokens == 0
    }
}

/// Pop the display record matching `args`. Tools don't see Rig's call id, so
/// we correlate by the only stable signal: the argument JSON the model sent.
// O(n) scan, first-match. Identical-arg calls are interchangeable for display.
fn take_display(sink: &DisplaySink, args: &Value) -> Option<mewcode_protocol::ToolDisplay> {
    let mut records = sink.lock().ok()?;
    let pos = records.iter().position(|r| &r.args == args)?;
    Some(records.remove(pos).display)
}

/// Stream one Rig agent prompt to completion, emitting `TextDelta`,
/// `ToolInputAvailable`, and `ToolOutputAvailable` events through `tx`.
///
/// The multi-turn loop is handled by Rig internally; this function only
/// translates Rig stream items into mewcode events.
///
/// Returns the full reply text and accumulated token usage.
pub async fn run_agent_stream<M: rig_core::completion::CompletionModel + 'static>(
    agent: rig_core::agent::Agent<M>,
    user_text: String,
    history: Vec<rig_core::completion::Message>,
    tx: &mpsc::Sender<StreamEvent>,
    display_sink: Option<DisplaySink>,
) -> Result<(String, TurnUsage), EngineError> {
    let mut stream = agent.stream_prompt(user_text).history(history).await;

    let mut full_reply = String::new();
    let mut usage = TurnUsage::default();
    // Remember each tool call's arguments by id so a tool result can be
    // correlated with the display record its execution left in the sink.
    let mut call_args: HashMap<String, Value> = HashMap::new();

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
                call_args.insert(tool_call.id.clone(), tool_call.function.arguments.clone());
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
                let call_id = tool_result.id;
                let _ = tx
                    .send(StreamEvent::ToolOutputAvailable {
                        tool_call_id: call_id.clone(),
                        output: parsed,
                    })
                    .await;

                // Emit any render-only display the tool recorded during
                // execution, keyed to this call by argument match. Kept
                // strictly after ToolOutputAvailable and off the model path.
                if let Some(sink) = &display_sink {
                    if let Some(args) = call_args.get(&call_id) {
                        if let Some(display) = take_display(sink, args) {
                            let _ = tx
                                .send(StreamEvent::ToolDisplayAvailable {
                                    tool_call_id: call_id,
                                    display,
                                })
                                .await;
                        }
                    }
                }
            }
            Ok(MultiTurnStreamItem::CompletionCall(call)) => {
                usage.input_tokens += call.usage.input_tokens;
                usage.output_tokens += call.usage.output_tokens;
                usage.cached_input_tokens += call.usage.cached_input_tokens;
                usage.cache_creation_input_tokens += call.usage.cache_creation_input_tokens;

                tracing::debug!(
                    input_tokens = call.usage.input_tokens,
                    output_tokens = call.usage.output_tokens,
                    cached_input_tokens = call.usage.cached_input_tokens,
                    cache_creation_input_tokens = call.usage.cache_creation_input_tokens,
                    "completion call usage"
                );
            }
            Ok(MultiTurnStreamItem::FinalResponse(response)) => {
                if full_reply.is_empty() {
                    let text = response.output().to_string();
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

    // Record accumulated cache totals on the parent span
    let span = tracing::Span::current();
    span.record(
        "gen_ai.usage.cache_read.input_tokens",
        usage.cached_input_tokens,
    );
    span.record(
        "gen_ai.usage.cache_creation.input_tokens",
        usage.cache_creation_input_tokens,
    );

    Ok((full_reply, usage))
}
