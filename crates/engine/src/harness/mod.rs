//! Long-running agent harness. Owns the conversation state, drives
//! the tool-calling loop, and streams [`mewcode_protocol::StreamEvent`]s
//! back through an mpsc channel until the model stops emitting tool
//! calls or the user cancels.

use std::sync::Arc;

use mewcode_protocol::{Mode, ModelId, StreamEvent};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent::build_system_prompt;
use crate::error::EngineError;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

/// The agent harness.
#[derive(Clone)]
pub struct Harness {
    model: ModelId,
    mode: Mode,
    cancel: CancellationToken,
    skills: Arc<SkillRegistry>,
    tools: Arc<ToolRegistry>,
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
            cancel: CancellationToken::new(),
            skills,
            tools,
        }
    }

    /// Cancel the in-flight stream, if any.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// The system prompt for the model's first turn.
    pub fn system_prompt(&self) -> String {
        build_system_prompt(self.mode, &self.skills, &self.tools)
    }

    /// Number of skills currently available.
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }

    /// Tool names currently registered.
    pub fn tool_names(&self) -> Vec<&'static str> {
        self.tools.names()
    }

    /// Run a single turn: send a synthetic "hello" reply, then finish.
    /// Placeholder until the real rig streaming lands.
    pub async fn run_placeholder(
        &self,
        user_text: &str,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        let started = std::time::Instant::now();
        let message_id = uuid::Uuid::new_v4();

        tx.send(StreamEvent::Start {
            message_id,
            mode: self.mode,
            model: self.model,
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        // The placeholder advertises the prompt-side state so a developer
        // can sanity-check wiring without waiting for a real LLM call.
        let reply = format!(
            "mewcode placeholder reply (model={}, mode={}, skills={}, tools=[{}], you said: {:?})",
            self.model.provider_id(),
            self.mode.as_str(),
            self.skills.len(),
            self.tools.names().join(", "),
            user_text
        );

        for chunk in reply.chars().collect::<Vec<_>>().chunks(8) {
            if self.cancel.is_cancelled() {
                tx.send(StreamEvent::Aborted).await.ok();
                return Err(EngineError::Aborted);
            }
            let delta: String = chunk.iter().collect();
            tx.send(StreamEvent::TextDelta { delta }).await.ok();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        tx.send(StreamEvent::Finish {
            duration_ms: started.elapsed().as_millis() as u64,
            input_tokens: None,
            output_tokens: None,
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(())
    }
}
