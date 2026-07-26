use mewcode_engine::agent::AgentActivity;

#[test]
fn agent_activity_is_shared_across_attempt_observers() {
    let producer = AgentActivity::default();
    let observer = producer.clone();

    assert!(!observer.was_observed());
    producer.mark();
    assert!(observer.was_observed());
}
