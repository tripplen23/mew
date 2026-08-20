use mewcode_protocol::StreamEvent;
use rig_core::client::CompletionClient;
use tokio::sync::mpsc;

use super::Agent;
use super::provider::Provider;
use super::stream::{self, AgentActivity, TurnUsage};
use crate::error::EngineError;

pub(super) async fn run_turn(
    agent: Agent,
    user_text: String,
    history: Vec<rig_core::completion::Message>,
    tx: &mpsc::Sender<StreamEvent>,
    activity: AgentActivity,
) -> Result<(String, TurnUsage), EngineError> {
    let model_id = agent.model.as_str();
    match &agent.provider {
        Provider::Anthropic(p) => {
            let model = p
                .client()
                .completion_model(model_id)
                .with_automatic_caching_1h();
            let rig_agent = rig_core::agent::AgentBuilder::new(model)
                .name("mewcode")
                .preamble(&agent.system_prompt)
                .max_tokens(agent.max_tokens)
                .default_max_turns(agent.max_turns)
                .tools(agent.tools)
                .build();
            stream::run_agent_stream(
                rig_agent,
                agent.model,
                user_text,
                history,
                tx,
                agent.display_sink,
                activity,
                agent.session_tokens_base,
            )
            .await
        }
        Provider::OpenCodeGo(p) | Provider::OpenAi(p) => {
            let rig_agent = p
                .client()
                .agent(model_id)
                .name("mewcode")
                .preamble(&agent.system_prompt)
                .max_tokens(agent.max_tokens)
                .default_max_turns(agent.max_turns)
                .tools(agent.tools)
                .build();
            stream::run_agent_stream(
                rig_agent,
                agent.model,
                user_text,
                history,
                tx,
                agent.display_sink,
                activity,
                agent.session_tokens_base,
            )
            .await
        }
    }
}
