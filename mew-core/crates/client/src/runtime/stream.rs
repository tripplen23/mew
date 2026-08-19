use futures::StreamExt;
use tokio::sync::mpsc;

use uuid::Uuid;

use mewcode_protocol::StreamEvent;
use mewcode_protocol::event::{ChatRequest, ErrorCode};

use crate::net::ApiClient;

use super::model::{Msg, StreamMsg};

/// HTTP statuses treated as transient upstream faults worth a retry hint.
/// Mirrors `retryable_status` in `mewcode-engine` (server-side policy);
/// kept local so the client stays independent of the engine crate.
const RETRYABLE_HTTP_STATUSES: &[u16] = &[408, 409, 425, 429, 500, 502, 503, 504, 529];

/// Map a network-boundary failure into the structured fields carried by
/// [`StreamMsg::Failed`]. Connection/transport/stream breaks are transient;
/// decode failures are client-side and not worth a retry hint.
fn net_error_parts(error: &crate::net::NetError) -> (String, ErrorCode, bool) {
    match error {
        crate::net::NetError::Transport(_) | crate::net::NetError::Stream(_) => {
            (error.to_string(), ErrorCode::Upstream, true)
        }
        crate::net::NetError::Status(status) => (
            error.to_string(),
            ErrorCode::Upstream,
            RETRYABLE_HTTP_STATUSES.contains(&status.as_u16()),
        ),
        crate::net::NetError::Decode(_) => (error.to_string(), ErrorCode::Internal, false),
    }
}

pub(crate) async fn run_chat_stream(api: ApiClient, req: ChatRequest, tx: mpsc::Sender<Msg>) {
    let stream = match api.chat_stream(&req).await {
        Ok(stream) => stream,
        Err(e) => {
            let (message, code, retryable) = net_error_parts(&e);
            let _ = tx
                .send(Msg::Stream(StreamMsg::Failed {
                    message,
                    code,
                    retryable,
                }))
                .await;
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
                let (message, code, retryable) = net_error_parts(&e);
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Failed {
                        message,
                        code,
                        retryable,
                    }))
                    .await;
                break;
            }
            Ok(StreamEvent::Start {
                message_id, pwd, ..
            }) => StreamMsg::Started {
                id: message_id,
                pwd,
            },
            Ok(StreamEvent::TextDelta { delta }) => StreamMsg::Delta(delta),
            Ok(StreamEvent::TokenUsage {
                input_tokens,
                output_tokens,
                session_tokens,
                context_limit,
            }) => StreamMsg::TokenUsage {
                input_tokens,
                output_tokens,
                session_tokens,
                context_limit,
            },
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
                cost_usd,
                ..
            }) => {
                terminated = true;
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Finished {
                        duration_ms,
                        session_tokens,
                        context_limit,
                        cost_usd,
                    }))
                    .await;
                break;
            }
            Ok(StreamEvent::Aborted) => {
                terminated = true;
                let _ = tx.send(Msg::Stream(StreamMsg::Aborted)).await;
                break;
            }
            Ok(StreamEvent::Error {
                code,
                message,
                retryable,
                ..
            }) => {
                terminated = true;
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Failed {
                        message,
                        code,
                        retryable,
                    }))
                    .await;
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
            .send(Msg::Stream(StreamMsg::Failed {
                message: "chat stream ended unexpectedly".into(),
                code: ErrorCode::Upstream,
                retryable: true,
            }))
            .await;
    }
}

pub(crate) async fn run_compact_stream(api: ApiClient, session_id: Uuid, tx: mpsc::Sender<Msg>) {
    let stream = match api.compact_session_stream(session_id).await {
        Ok(stream) => stream,
        Err(e) => {
            let (message, code, retryable) = net_error_parts(&e);
            let _ = tx
                .send(Msg::Stream(StreamMsg::Failed {
                    message,
                    code,
                    retryable,
                }))
                .await;
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
                let (message, code, retryable) = net_error_parts(&e);
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Failed {
                        message,
                        code,
                        retryable,
                    }))
                    .await;
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
                cost_usd,
                ..
            }) => {
                terminated = true;
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Finished {
                        duration_ms,
                        session_tokens,
                        context_limit,
                        cost_usd,
                    }))
                    .await;
                break;
            }
            Ok(StreamEvent::Aborted) => {
                terminated = true;
                let _ = tx.send(Msg::Stream(StreamMsg::Aborted)).await;
                break;
            }
            Ok(StreamEvent::Error {
                code,
                message,
                retryable,
                ..
            }) => {
                terminated = true;
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Failed {
                        message,
                        code,
                        retryable,
                    }))
                    .await;
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
            .send(Msg::Stream(StreamMsg::Failed {
                message: "compaction stream ended unexpectedly".into(),
                code: ErrorCode::Upstream,
                retryable: true,
            }))
            .await;
    }
}
