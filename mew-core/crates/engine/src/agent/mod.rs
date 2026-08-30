//! The mewcode agent: a configured model with a system prompt and tools.
//!
//! This module owns everything that talks to the LLM through Rig's
//! [`Agent`](rig_core::agent::struct.Agent) abstraction:
//! - system-prompt construction ([`build_system_prompt`])
//! - Rig agent execution (`Agent::run_turn`)
//! - streaming translation from Rig items to [`StreamEvent`]s (the stream module)
//!
//! The [`Harness`](crate::harness::Harness) consumes an [`Agent`] each turn:
//! it builds the system prompt, creates an [`Agent`], and delegates execution.

mod prompt;
mod provider;
mod rig;
mod stream;

use mewcode_protocol::{ModelRef, StreamEvent};
use tokio::sync::mpsc;

pub use self::prompt::build_system_prompt;
pub use self::provider::Provider;
pub use self::stream::AgentActivity;
pub use self::stream::TurnUsage;
pub use self::stream::map_stream_error;
use crate::error::EngineError;

pub(crate) const DEFAULT_MAX_TOKENS: u64 = 16384;

const DEFAULT_MAX_TURNS: usize = 100;

/// A configured agent ready to run one turn.
///
/// The agent is intentionally built per-turn: the system prompt may change
/// between turns, and tool wrappers are cheap to reconstruct from the registry.
pub struct Agent {
    provider: Provider,
    model: ModelRef,
    system_prompt: String,
    tools: Vec<Box<dyn rig_core::tool::ToolDyn>>,
    max_tokens: u64,
    max_turns: usize,
    display_sink: Option<crate::tools::DisplaySink>,
    session_tokens_base: u64,
    context_limit: u64,
}

impl Agent {
    /// Build an agent for the given provider, model, and system prompt.
    pub fn new(provider: Provider, model: ModelRef, system_prompt: String) -> Self {
        Self {
            provider,
            model,
            system_prompt,
            tools: Vec::new(),
            max_tokens: DEFAULT_MAX_TOKENS,
            max_turns: DEFAULT_MAX_TURNS,
            display_sink: None,
            session_tokens_base: 0,
            context_limit: 0,
        }
    }

    /// Attach tools the model may call during the turn.
    pub fn with_tools(mut self, tools: Vec<Box<dyn rig_core::tool::ToolDyn>>) -> Self {
        self.tools = tools;
        self
    }

    /// Attach the display sink so tool render-data is streamed to the client.
    pub fn with_display_sink(mut self, sink: crate::tools::DisplaySink) -> Self {
        self.display_sink = Some(sink);
        self
    }

    /// Cap completion tokens for this turn.
    pub fn with_max_tokens(mut self, max_tokens: u64) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Cap internal agent turns for this turn.
    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    /// Seed the session context size from before this turn, so mid-turn
    /// `TokenUsage` events can report a running total.
    pub fn with_session_tokens(mut self, tokens: u64) -> Self {
        self.session_tokens_base = tokens;
        self
    }

    /// Set the provider-reported context limit used by live usage events.
    pub fn with_context_limit(mut self, context_limit: u64) -> Self {
        self.context_limit = context_limit;
        self
    }

    /// Run one user prompt through the configured Rig agent, streaming events
    /// through `tx` and returning the full assistant reply plus token usage.
    pub(crate) async fn run_turn(
        self,
        user_text: String,
        history: Vec<rig_core::completion::Message>,
        tx: &mpsc::Sender<StreamEvent>,
        activity: AgentActivity,
    ) -> Result<(String, TurnUsage), EngineError> {
        rig::run_turn(self, user_text, history, tx, activity).await
    }
}
