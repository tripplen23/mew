//! The mewcode agent: a configured model with a system prompt and tools.
//!
//! This module owns everything that talks to the LLM through Rig's
//! [`Agent`](rig_core::agent::struct.Agent.html) abstraction:
//! - system-prompt construction ([`build_system_prompt`])
//! - Rig agent execution ([`Agent::run_turn`])
//! - streaming translation from Rig items to [`StreamEvent`]s (the stream module)
//!
//! The [`Harness`](crate::harness::Harness) consumes an [`Agent`] each turn:
//! it builds the system prompt, creates an [`Agent`], and delegates execution.

mod prompt;
mod stream;

use mewcode_protocol::{ModelId, StreamEvent};
use rig_core::client::CompletionClient;
use tokio::sync::mpsc;

pub use self::prompt::build_system_prompt;
use crate::error::EngineError;
use crate::provider::Provider;

pub(crate) const DEFAULT_MAX_TOKENS: u64 = 16384;

const DEFAULT_MAX_TURNS: usize = 100;

/// A configured agent ready to run one turn.
///
/// The agent is intentionally built per-turn: the system prompt may change
/// between turns, and tool wrappers are cheap to reconstruct from the registry.
pub struct Agent {
    provider: Provider,
    model: ModelId,
    system_prompt: String,
    tools: Vec<Box<dyn rig_core::tool::ToolDyn>>,
    max_tokens: u64,
    max_turns: usize,
    display_sink: Option<crate::tools::DisplaySink>,
}

impl Agent {
    /// Build an agent for the given provider, model, and system prompt.
    pub fn new(provider: Provider, model: ModelId, system_prompt: String) -> Self {
        Self {
            provider,
            model,
            system_prompt,
            tools: Vec::new(),
            max_tokens: DEFAULT_MAX_TOKENS,
            max_turns: DEFAULT_MAX_TURNS,
            display_sink: None,
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

    /// Run one user prompt through the configured Rig agent, streaming events
    /// through `tx` and returning the full assistant reply.
    pub async fn run_turn(
        self,
        user_text: String,
        history: Vec<rig_core::completion::Message>,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<String, EngineError> {
        let model_id = self.model.as_str();
        match &self.provider {
            Provider::Anthropic(p) => {
                let model = p
                    .client()
                    .completion_model(model_id)
                    .with_automatic_caching_1h();
                let agent = rig_core::agent::AgentBuilder::new(model)
                    .name("mewcode")
                    .preamble(&self.system_prompt)
                    .max_tokens(self.max_tokens)
                    .default_max_turns(self.max_turns)
                    .tools(self.tools)
                    .build();
                stream::run_agent_stream(agent, user_text, history, tx, self.display_sink).await
            }
            Provider::OpenCodeGo(p) | Provider::OpenAi(p) => {
                let agent = p
                    .client()
                    .agent(model_id)
                    .name("mewcode")
                    .preamble(&self.system_prompt)
                    .max_tokens(self.max_tokens)
                    .default_max_turns(self.max_turns)
                    .tools(self.tools)
                    .build();
                stream::run_agent_stream(agent, user_text, history, tx, self.display_sink).await
            }
        }
    }
}
