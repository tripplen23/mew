use futures::StreamExt;
use tokio::sync::mpsc;

use uuid::Uuid;

use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::StreamEvent;

use crate::net::ApiClient;

use super::model::{Msg, StreamMsg};

pub(crate) async fn run_chat_stream(api: ApiClient, req: ChatRequest, tx: mpsc::Sender<Msg>) {
    let stream = match api.chat_stream(&req).await {
        Ok(stream) => stream,
        Err(e) => {
            let _ = tx.send(Msg::Stream(StreamMsg::Failed(e.to_string()))).await;
            return;
        }
    };
    futures::pin_mut!(stream);

    // Prevents fallback from emitting duplicate terminal message.
    let mut terminated = false;

    while let Some(frame) = stream.next().await {
        let msg = match frame {
            Err(e) => {
                terminated = true;
                let _ = tx.send(Msg::Stream(StreamMsg::Failed(e.to_string()))).await;
                break;
            }
            Ok(StreamEvent::Start {
                message_id, pwd, ..
            }) => StreamMsg::Started {
                id: message_id,
                pwd,
            },
            Ok(StreamEvent::TextDelta { delta }) => StreamMsg::Delta(delta),
            Ok(StreamEvent::ToolInputAvailable {
                tool_call_id,
                tool_name,
                input,
            }) => StreamMsg::ToolInput {
                id: tool_call_id,
                name: tool_name,
                input,
            },
            Ok(StreamEvent::ToolOutputAvailable {
                tool_call_id,
                output,
            }) => StreamMsg::ToolOutput {
                id: tool_call_id,
                output,
            },
            Ok(StreamEvent::ToolDisplayAvailable {
                tool_call_id,
                display,
            }) => StreamMsg::ToolDisplay {
                id: tool_call_id,
                display,
            },
            Ok(StreamEvent::ChoiceRequest(request)) => StreamMsg::ChoiceRequest(request),
            Ok(StreamEvent::Compacted {
                tokens_before,
                context_limit,
                summary,
                thought_duration_ms,
            }) => StreamMsg::Compacted {
                tokens_before,
                context_limit,
                summary,
                thought_duration_ms,
            },
            Ok(StreamEvent::Finish {
                duration_ms,
                session_tokens,
                context_limit,
                ..
            }) => {
                terminated = true;
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Finished {
                        duration_ms,
                        session_tokens,
                        context_limit,
                    }))
                    .await;
                break;
            }
            Ok(StreamEvent::Aborted) => {
                terminated = true;
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Failed("aborted".into())))
                    .await;
                break;
            }
            Ok(StreamEvent::Error { message }) => {
                terminated = true;
                let _ = tx.send(Msg::Stream(StreamMsg::Failed(message))).await;
                break;
            }
            Ok(StreamEvent::CompactionStarted { .. })
            | Ok(StreamEvent::CompactionProgress { .. })
            | Ok(StreamEvent::CompactionSummaryDelta { .. }) => continue,
        };
        if tx.send(Msg::Stream(msg)).await.is_err() {
            return;
        }
    }

    // Stream ended without terminal event. Without fallback, `s.streaming` leaks,
    // silently blocking all future submits until client restart.
    if !terminated {
        let _ = tx
            .send(Msg::Stream(StreamMsg::Failed(
                "chat stream ended unexpectedly".to_string(),
            )))
            .await;
    }
}

pub(crate) async fn run_compact_stream(api: ApiClient, session_id: Uuid, tx: mpsc::Sender<Msg>) {
    let stream = match api.compact_session_stream(session_id).await {
        Ok(stream) => stream,
        Err(e) => {
            let _ = tx.send(Msg::Stream(StreamMsg::Failed(e.to_string()))).await;
            return;
        }
    };
    futures::pin_mut!(stream);

    // Prevents duplicate terminal message from fallback.
    let mut terminated = false;

    while let Some(frame) = stream.next().await {
        let msg = match frame {
            Err(e) => {
                terminated = true;
                let _ = tx.send(Msg::Stream(StreamMsg::Failed(e.to_string()))).await;
                break;
            }
            Ok(StreamEvent::CompactionStarted { .. }) => StreamMsg::CompactionStarted,
            Ok(StreamEvent::CompactionProgress { phase, message }) => {
                StreamMsg::CompactionProgress { phase, message }
            }
            Ok(StreamEvent::CompactionSummaryDelta { delta }) => {
                StreamMsg::CompactionSummaryDelta(delta)
            }
            Ok(StreamEvent::Compacted {
                tokens_before,
                context_limit,
                summary,
                thought_duration_ms,
            }) => StreamMsg::Compacted {
                tokens_before,
                context_limit,
                summary,
                thought_duration_ms,
            },
            Ok(StreamEvent::Finish {
                duration_ms,
                session_tokens,
                context_limit,
                ..
            }) => {
                terminated = true;
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Finished {
                        duration_ms,
                        session_tokens,
                        context_limit,
                    }))
                    .await;
                break;
            }
            Ok(StreamEvent::Error { message }) => {
                terminated = true;
                let _ = tx.send(Msg::Stream(StreamMsg::Failed(message))).await;
                break;
            }
            _ => continue,
        };
        if tx.send(Msg::Stream(msg)).await.is_err() {
            return;
        }
    }

    // Stream died without terminal event. Without fallback, session stays
    // in-flight, silently blocking all future submits until client restart.
    if !terminated {
        let _ = tx
            .send(Msg::Stream(StreamMsg::Failed(
                "compaction stream ended unexpectedly".to_string(),
            )))
            .await;
    }
}
