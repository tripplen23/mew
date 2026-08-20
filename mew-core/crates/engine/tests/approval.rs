//! Tests for the approval broker's persistent always-allow path.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mewcode_engine::tools::ApprovalBroker;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    StreamEvent,
    event::{CHOICE_ALWAYS_ALLOW, ChoiceResponse},
};
use serde_json::json;
use tokio::sync::mpsc;
use uuid::Uuid;

#[tokio::test]
async fn preloaded_always_allow_short_circuits_without_prompting() {
    let broker = ApprovalBroker::default().with_always_allowed(vec![names::BASH]);
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(8);

    let result = broker
        .approve_tool(Uuid::new_v4(), names::BASH, &json!({"command": "ls"}), &tx)
        .await;
    assert!(result.is_ok());
    assert!(
        rx.try_recv().is_err(),
        "preloaded always-allow must not emit a choice request"
    );
}

#[tokio::test]
async fn always_allow_choice_persists_and_skips_future_prompts() {
    let persisted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let persist_hook = {
        let persisted = persisted.clone();
        Arc::new(move |tool: &'static str| {
            persisted.lock().unwrap().push(tool.to_string());
        }) as Arc<dyn Fn(&'static str) + Send + Sync>
    };
    let broker = ApprovalBroker::default().with_persist_always_allow(persist_hook);
    let session = Uuid::new_v4();
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(8);

    // Drive the approval round-trip: approve_tool emits the choice request,
    // the client answers "always allow".
    let approval = tokio::spawn({
        let broker = broker.clone();
        let tx = tx.clone();
        async move {
            broker
                .approve_tool(session, names::BASH, &json!({"command": "ls"}), &tx)
                .await
        }
    });

    let StreamEvent::ChoiceRequest(request) =
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("choice request within timeout")
            .expect("choice request present")
    else {
        panic!("expected a ChoiceRequest");
    };
    assert!(
        request.options.iter().any(|o| o.id == CHOICE_ALWAYS_ALLOW),
        "the dialog must offer an always-allow option"
    );

    let delivered = broker.answer(
        session,
        ChoiceResponse::Selected {
            request_id: request.request_id.clone(),
            option_id: CHOICE_ALWAYS_ALLOW.into(),
        },
    );
    assert!(delivered, "pending approval must resolve");
    assert!(
        approval.await.unwrap().is_ok(),
        "always allow approves the turn"
    );

    assert!(persisted.lock().unwrap().as_slice() == ["bash"]);
    assert!(
        request.request_id.starts_with("tool-approval-"),
        "request id is stable-shaped"
    );

    // Second call: no new prompt, hook not re-invoked.
    let result = broker
        .approve_tool(Uuid::new_v4(), names::BASH, &json!({"command": "ls"}), &tx)
        .await;
    assert!(result.is_ok());
    assert!(
        rx.try_recv().is_err(),
        "always-allowed tool must not prompt again"
    );
    assert_eq!(
        persisted.lock().unwrap().len(),
        1,
        "hook fires once per rule"
    );
}
