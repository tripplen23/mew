//! Long-running agent harness. Owns the conversation state, drives
//! the tool-calling loop, and streams [`mewcode_protocol::StreamEvent`]s
//! back through an mpsc channel until the model stops emitting tool
//! calls or the user cancels.

mod completion;
mod trace;

pub use self::completion::last_user_text;
#[doc(hidden)]
pub use self::completion::user_text_with_file_context;
pub use self::trace::{chat_turn_span, record_turn_input, record_turn_output};

use std::path::PathBuf;
use std::sync::Arc;

use mewcode_protocol::{Message, Mode, ModelId, Role, StreamEvent};
use tokio::sync::mpsc;
use tracing::Instrument;
use uuid::Uuid;

use crate::agent::{Agent, build_system_prompt};
use crate::approval::ApprovalBroker;
use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::history::{self, HistoryStrategy};
use crate::memory::MemoryStore;
use crate::provider::Provider;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

/// The agent harness.
#[derive(Clone)]
pub struct Harness {
    model: ModelId,
    mode: Mode,
    skills: Arc<SkillRegistry>,
    tools: Arc<ToolRegistry>,
    session_id: Option<Uuid>,
    history_strategy: HistoryStrategy,
    memory: Option<MemoryStore>,
    display_sink: Option<crate::tools::DisplaySink>,
    project_root: Option<PathBuf>,
    approval_broker: Option<ApprovalBroker>,
    session_tokens: u64,
    compaction_summary: Option<String>,
    compacted_up_to: usize,
    compaction_updated: bool,
}

impl std::fmt::Debug for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Harness")
            .field("model", &self.model)
            .field("mode", &self.mode)
            .field("tools", &self.tools.names())
            .field("skill_count", &self.skills.len())
            .finish()
    }
}

impl Harness {
    /// Build a new harness. `skills` is the catalog source for the
    /// system prompt; `tools` supplies the descriptors the model can call.
    pub fn new(
        model: ModelId,
        mode: Mode,
        skills: Arc<SkillRegistry>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            model,
            mode,
            skills,
            tools,
            session_id: None,
            history_strategy: HistoryStrategy::default_raw(),
            memory: None,
            display_sink: None,
            project_root: None,
            approval_broker: None,
            session_tokens: 0,
            compaction_summary: None,
            compacted_up_to: 0,
            compaction_updated: false,
        }
    }

    /// Set the project root used to resolve `@file` mentions.
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.project_root = Some(root.into());
        self
    }

    /// Attach the display sink so mutating tools' render-only data (diffs) is
    /// correlated to tool calls and streamed as `ToolDisplayAvailable`.
    pub fn with_display_sink(mut self, sink: crate::tools::DisplaySink) -> Self {
        self.display_sink = Some(sink);
        self
    }

    /// Attach the in-memory approval broker for interactive tool approvals.
    pub fn with_approval_broker(mut self, broker: ApprovalBroker) -> Self {
        self.approval_broker = Some(broker);
        self
    }

    /// Record the chat session id so reported turns are grouped by session in Langfuse.
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Seed the accumulated token total from a prior turn in the same session.
    pub fn with_session_tokens(mut self, tokens: u64) -> Self {
        self.session_tokens = tokens;
        self
    }

    /// Set a compaction summary from a previous manual or automatic compaction,
    /// paired with the boundary it covers. This replaces `messages[..compacted_up_to]`
    /// in the history sent to the model on the next turn — actually shrinking
    /// what's sent, not just displaying a summary.
    pub fn with_compaction_summary(
        mut self,
        summary: Option<String>,
        compacted_up_to: usize,
    ) -> Self {
        self.compaction_summary = summary;
        self.compacted_up_to = compacted_up_to;
        self
    }

    /// Current accumulated session token total.
    pub fn session_tokens(&self) -> u64 {
        self.session_tokens
    }

    /// The current compaction summary and the message-index boundary it
    /// covers, if this turn ran automatic compaction. `None` when no new
    /// compaction happened this turn — the caller should not overwrite the
    /// stored value in that case.
    pub fn updated_compaction(&self) -> Option<(&str, usize)> {
        if self.compaction_updated {
            self.compaction_summary
                .as_deref()
                .map(|s| (s, self.compacted_up_to))
        } else {
            None
        }
    }

    /// Attach a memory store for durable facts. When set, the memory content
    /// is injected into the system prompt as a `<memory>` section.
    pub fn with_memory(mut self, memory: MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// The exact system prompt sent this turn: static sections plus, when
    /// present, the durable-memory section. Single source of truth so
    /// `run_turn_inner` always sends what this returns.
    fn compose_system_prompt(&self) -> String {
        let mut prompt = build_system_prompt(self.mode, &self.skills, &self.tools);
        if let Some(section) = self.memory.as_ref().and_then(|m| m.format()) {
            prompt.push_str("\n\n");
            prompt.push_str(&section);
        }
        prompt
    }

    /// Check if compaction should trigger based on accumulated token usage.
    ///
    /// Returns `true` when `session_tokens` reaches 75% of the model's
    /// `context_limit`.
    fn should_compact(&self) -> bool {
        let limit = self.model.context_limit();
        if limit == 0 {
            return false;
        }
        let threshold = (limit as f64 * history::COMPACTION_THRESHOLD) as u64;
        self.session_tokens >= threshold
    }

    /// Run one agent invocation, streaming events through the channel.
    /// The agent may make up to `MAX_AGENT_TURNS` sub-turns (tool calls
    /// → results → reply) before finishing. Returns `Err` on any failure
    /// and emits nothing on that path — the caller owns the `Error` event.
    ///
    /// If the provider returns a context-overflow error, compacts history
    /// and retries once.
    pub async fn run_turn(
        &mut self,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        let span = trace::chat_turn_span(self.model, self.mode);
        if let Some(session_id) = self.session_id {
            span.record("langfuse.session.id", session_id.to_string());
        }

        match self
            .run_turn_inner(messages, &tx)
            .instrument(span.clone())
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if e.is_context_overflow() => {
                tracing::warn!("context overflow detected, forcing compaction and retrying");
                // Force compaction by setting session_tokens to threshold
                let limit = self.model.context_limit();
                if limit > 0 {
                    self.session_tokens = (limit as f64 * history::COMPACTION_THRESHOLD) as u64;
                }
                // Retry once with compacted history
                self.run_turn_inner(messages, &tx)
                    .instrument(span)
                    .await
                    .map(|_| ())
            }
            Err(e) => Err(e),
        }
    }

    /// Build the history to send this turn, running automatic compaction
    /// first if accumulated tokens are near the model's context limit.
    ///
    /// `prior_messages` is everything before the current user prompt.
    /// Messages already covered by a stored compaction summary
    /// (`self.compacted_up_to`) are never re-examined for further
    /// compaction — only the uncovered tail counts toward the trigger
    /// decision, and the summary stands in for everything before it in the
    /// history actually sent to the model.
    ///
    /// On success this may update `self.compacted_up_to`,
    /// `self.compaction_summary`, `self.compaction_updated`, and
    /// `self.session_tokens` as a side effect, and emits a `Compacted`
    /// event through `tx` when compaction ran. Errors from the LLM
    /// compaction call are swallowed here and fall back to a truncated
    /// concatenation — a fresh call already failed, so surfacing the error
    /// would just fail the turn a second time.
    async fn build_turn_history(
        &mut self,
        prior_messages: &[Message],
        cfg: &EngineConfig,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Vec<rig_core::completion::Message> {
        let compacted_up_to = self.compacted_up_to.min(prior_messages.len());
        let uncovered = &prior_messages[compacted_up_to..];
        self.compaction_updated = false;

        let needs_compaction =
            self.should_compact() && uncovered.len() > history::COMPACTION_PRESERVE_TURNS * 2;
        if !needs_compaction {
            return match self.compaction_summary.clone() {
                Some(summary) => history::build_history_with_summary_tail(
                    &summary,
                    uncovered,
                    &self.history_strategy,
                ),
                None => self.history_strategy.build(prior_messages),
            };
        }

        // Step 1: Prune (free, no LLM cost) — remove tool results, truncate file contents.
        let pruned = history::prune_messages(uncovered);

        // Heuristic: tool results are typically 50-70% of context tokens.
        // If we have many tool results, pruning alone might be enough.
        let has_tool_results = uncovered.iter().any(|m| {
            m.parts
                .iter()
                .any(|p| matches!(p, mewcode_protocol::MessagePart::ToolResult(_)))
        });

        // Estimate token savings from pruning (rough: 60% of tool result content).
        let tool_result_chars: usize = uncovered
            .iter()
            .flat_map(|m| &m.parts)
            .filter_map(|p| match p {
                mewcode_protocol::MessagePart::ToolResult(r) => {
                    serde_json::to_string(&r.output).ok().map(|s| s.len())
                }
                _ => None,
            })
            .sum();
        let estimated_token_savings = (tool_result_chars / history::CHARS_PER_TOKEN) as u64;
        let estimated_tokens_after_prune =
            self.session_tokens.saturating_sub(estimated_token_savings);

        let limit = self.model.context_limit();
        let threshold = (limit as f64 * history::COMPACTION_THRESHOLD) as u64;

        if has_tool_results && estimated_tokens_after_prune < threshold {
            // Pruning alone brings us back under threshold. Skip the LLM
            // call, but the pruned tail is still what's actually sent — no
            // summary is stored, so the compaction boundary does not move;
            // this is a per-turn optimization only.
            tracing::info!(
                estimated_tokens_after_prune,
                threshold,
                "pruned history, skipping LLM compaction"
            );
            let tokens_before = self.session_tokens;
            self.session_tokens = estimated_tokens_after_prune;
            let _ = tx
                .send(StreamEvent::Compacted {
                    tokens_before,
                    context_limit: limit,
                    summary: "[Pruned tool results — no LLM summary needed]".to_string(),
                    thought_duration_ms: 0,
                })
                .await;
            return match self.compaction_summary.clone() {
                Some(summary) => history::build_history_with_summary_tail(
                    &summary,
                    &pruned,
                    &self.history_strategy,
                ),
                None => self.history_strategy.build(&pruned),
            };
        }

        // Still over threshold after pruning (or nothing to prune). Split
        // the *uncovered, pruned* tail: fold the head into a new summary,
        // keep the recent turns verbatim. This actually shrinks what's sent
        // going forward, because the resulting boundary is persisted by the
        // caller via `updated_compaction()`.
        let (compact_head, tail) = history::split_for_compaction(&pruned);
        let tokens_before = self.session_tokens;
        let context_limit = limit;
        tracing::info!(
            head_count = compact_head.len(),
            tail_count = tail.len(),
            tokens_before,
            context_limit,
            "compacting history with LLM after prune"
        );

        let result = crate::compact::compact_history(
            compact_head,
            self.model,
            cfg,
            self.memory.clone(),
            tokens_before,
            self.compaction_summary.as_deref(),
            tx,
        )
        .await;
        let (summary, thought_duration_ms) = match result {
            Ok(r) => (r.summary, r.thought_duration_ms),
            Err(e) => {
                tracing::warn!(error = %e, "LLM compaction failed, using concatenation");
                // Fallback to simple concatenation, capped so a failure
                // doesn't just reproduce the same overflow.
                const FALLBACK_CHAR_CAP: usize = 8_000;
                let mut fallback = compact_head
                    .iter()
                    .map(|m| {
                        let role = match m.role {
                            Role::User => "User",
                            Role::Assistant => "Assistant",
                            Role::Tool => "Tool",
                        };
                        format!("{}: {}", role, history::text_of(m))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if fallback.len() > FALLBACK_CHAR_CAP {
                    fallback.truncate(FALLBACK_CHAR_CAP);
                    fallback.push_str("\n[...truncated]");
                }
                (fallback, 0)
            }
        };

        // The new boundary: everything already covered, plus the messages
        // just folded into this summary. `pruned` mirrors `uncovered`
        // message-for-message (pruning only strips parts, never whole
        // messages), so `compact_head.len()` maps directly onto
        // `uncovered`/`prior_messages` indices.
        self.compacted_up_to = compacted_up_to + compact_head.len();
        self.compaction_summary = Some(summary.clone());
        self.compaction_updated = true;
        // Reset token counter — the compacted history is much smaller than
        // the accumulated total.
        self.session_tokens = 0;

        let _ = tx
            .send(StreamEvent::Compacted {
                tokens_before,
                context_limit,
                summary: summary.clone(),
                thought_duration_ms,
            })
            .await;
        history::build_history_with_summary_tail(&summary, tail, &self.history_strategy)
    }

    /// The turn proper: resolve config, select the user message, build
    /// history from prior turns, optionally inject durable memory into
    /// the system prompt, then run one agent invocation streaming
    /// TextDelta events through the channel and emit the Finish event.
    /// Returns the assistant reply on success so the caller can both
    /// report it and discard it. The SSE emission is unchanged —
    /// nothing reaches the channel on failure, so the server route stays
    /// the single owner of the `Error` event.
    async fn run_turn_inner(
        &mut self,
        messages: &[Message],
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<String, EngineError> {
        // The turn always answers the most recent user message. With no
        // user message there is nothing to send, so fail without a provider.
        let user_text = if let Some(root) = self.project_root.as_deref() {
            completion::user_text_with_file_context(messages, root)
        } else {
            last_user_text(messages)
        }
        .ok_or_else(|| EngineError::Other("no user message in chat history".to_string()))?;

        // Resolve the credential before any provider is constructed.
        let cfg = EngineConfig::from_env()?;

        // Build history from messages before the current user prompt, so
        // the prompt text is not duplicated when invoke_agent sends it
        // via `.prompt(user_text).with_history(history)`.
        let current_user_pos = messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| m.role == Role::User)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let prior_messages = &messages[..current_user_pos];

        let history = self.build_turn_history(prior_messages, &cfg, tx).await;

        // Build the system prompt, optionally injecting durable memory.
        let system_prompt = self.compose_system_prompt();

        let provider = Provider::for_model(self.model, &cfg)?;
        trace::record_turn_input(&tracing::Span::current(), &system_prompt, &user_text);

        // Emit Start before the first token so the client can prepare.
        let message_id = Uuid::new_v4();
        let started = std::time::Instant::now();
        let pwd = self
            .project_root
            .as_deref()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string());
        tx.send(StreamEvent::Start {
            message_id,
            mode: self.mode,
            model: self.model,
            pwd,
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        // Stream the reply through the agent layer. Token/turn caps are
        // owned by Agent's defaults; the harness doesn't override them.
        let approved_tools;
        let tools_registry = if self.mode.allows_writes() {
            match (self.session_id, self.approval_broker.clone()) {
                (Some(session_id), Some(broker)) => {
                    approved_tools = self.tools.with_approval(session_id, broker, tx.clone());
                    &approved_tools
                }
                _ => &self.tools,
            }
        } else {
            &self.tools
        };
        let tools = crate::tools::adapter::rig_tools(tools_registry);
        let mut agent = Agent::new(provider, self.model, system_prompt).with_tools(tools);
        if let Some(sink) = self.display_sink.clone() {
            agent = agent.with_display_sink(sink);
        }
        let (reply, usage) = agent.run_turn(user_text, history, tx).await?;
        trace::record_turn_output(&tracing::Span::current(), &reply);

        // Accumulate session token total.
        if !usage.is_empty() {
            self.session_tokens += usage.total();
        }

        // Emit Finish with actual token counts.
        tx.send(StreamEvent::Finish {
            duration_ms: started.elapsed().as_millis() as u64,
            input_tokens: if usage.input_tokens > 0 {
                Some(usage.input_tokens)
            } else {
                None
            },
            output_tokens: if usage.output_tokens > 0 {
                Some(usage.output_tokens)
            } else {
                None
            },
            session_tokens: Some(self.session_tokens),
            context_limit: if self.model.context_limit() > 0 {
                Some(self.model.context_limit())
            } else {
                None
            },
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(reply)
    }

    /// Emit the success-path event sequence for one turn: exactly one `Start`
    /// carrying this turn's mode and model, then a single `TextDelta` (omitted
    /// when `reply` is empty), then exactly one `Finish`, with zero tool events.
    pub async fn emit_reply(
        &self,
        reply: &str,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        let started = std::time::Instant::now();
        let message_id = Uuid::new_v4();

        tx.send(StreamEvent::Start {
            message_id,
            mode: self.mode,
            model: self.model,
            pwd: self
                .project_root
                .as_deref()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string()),
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        if !reply.is_empty() {
            tx.send(StreamEvent::TextDelta {
                delta: reply.to_string(),
            })
            .await
            .map_err(|e| EngineError::Other(e.to_string()))?;
        }

        tx.send(StreamEvent::Finish {
            duration_ms: started.elapsed().as_millis() as u64,
            input_tokens: None,
            output_tokens: None,
            session_tokens: Some(self.session_tokens),
            context_limit: if self.model.context_limit() > 0 {
                Some(self.model.context_limit())
            } else {
                None
            },
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(())
    }
}
