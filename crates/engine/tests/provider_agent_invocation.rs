// Feature: agent-pattern-harness
//
// The harness must invoke Rig's Agent abstraction, not wire chat by calling
// `CompletionModel::completion_request(...).send()` directly. This source-level
// regression is intentional: the requirement is architectural, and direct
// completion plumbing would still produce the same text output while blocking
// future tool/skill and streaming wiring at the right layer.

#[test]
fn provider_invocation_uses_rig_agent_pattern() {
    let provider_src = include_str!("../src/provider.rs");

    assert!(
        provider_src.contains(".agent("),
        "provider should build a Rig Agent via CompletionClient::agent"
    );
    assert!(
        !provider_src.contains(".completion_request("),
        "provider must not invoke direct completion_request from the harness path"
    );
    assert!(
        provider_src.contains("openai::CompletionsClient"),
        "OpenCode Go OpenAI-compatible models must use Rig's chat-completions client, not the Responses API client"
    );
    assert!(
        !provider_src.contains("openai::Client,"),
        "OpenAI-compatible provider must not use Rig's default Responses API client"
    );
}
