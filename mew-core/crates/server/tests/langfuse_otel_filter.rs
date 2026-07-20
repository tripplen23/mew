// Feature: Rig-agent Langfuse tracing
//
// The server exports a Mew-level `chat-turn` span to Langfuse with
// `langfuse.*` IO fields. Rig's `invoke_agent` and `execute_tool` spans
// pass through the filter and carry the standard `gen_ai.*` fields.
// Rig's per-turn `chat` spans and provider `completions` spans are
// suppressed to avoid noisy duplicate observations.

#[test]
fn langfuse_otel_exports_mew_turn_not_rig_internal_spans() {
    let main_src = include_str!("../src/main.rs");

    assert!(
        main_src.contains("rig::agent_chat=off"),
        "Langfuse OTel export should suppress Rig's internal agent chat span"
    );
    assert!(
        main_src.contains("rig::completions=off"),
        "Langfuse OTel export should suppress Rig's internal provider span"
    );
    assert!(main_src.contains("LANGFUSE_BASE_URL"));
    assert!(
        main_src.contains("with_scheduled_delay(Duration::from_millis(500))"),
        "Langfuse batch export should not wait for the SDK's 5s default delay"
    );
    assert!(
        !main_src.contains("LANGFUSE_HOST"),
        "LANGFUSE_BASE_URL is the only Langfuse URL env var"
    );
}
