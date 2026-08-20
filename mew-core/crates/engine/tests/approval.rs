//! Tests for the approval broker's persistent always-allow path, including
//! per-command/per-path scoping.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mewcode_engine::tools::ApprovalBroker;
use mewcode_protocol::StreamEvent;
use mewcode_protocol::event::{CHOICE_ALWAYS_ALLOW, ChoiceResponse};
use mewcode_protocol::tool::names;
use serde_json::json;
use tokio::sync::mpsc;
use uuid::Uuid;

#[tokio::test]
async fn preloaded_scoped_allow_short_circuits_without_prompting() {
    // Whole-tool seed: every bash command passes.
    let broker = ApprovalBroker::default().with_always_allowed(vec![(names::BASH, None)]);
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(8);
    let result = broker
        .approve_tool(Uuid::new_v4(), names::BASH, &json!({"command": "ls"}), &tx)
        .await;
    assert!(result.is_ok());
    assert!(rx.try_recv().is_err(), "preloaded allow must not prompt");
}

#[tokio::test]
async fn always_allow_choice_persists_scope_and_skips_future_prompts() {
    type Rule = (&'static str, Option<String>);
    let persisted: Arc<Mutex<Vec<Rule>>> = Arc::new(Mutex::new(Vec::new()));
    let persist_hook = {
        let persisted = persisted.clone();
        Arc::new(move |tool: &'static str, scope: Option<&str>| {
            persisted
                .lock()
                .unwrap()
                .push((tool, scope.map(str::to_string)));
        }) as Arc<dyn Fn(&'static str, Option<&str>) + Send + Sync>
    };
    let broker = ApprovalBroker::default().with_persist_always_allow(persist_hook);
    let session = Uuid::new_v4();
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(8);

    // Round-trip: approve `bash ls` with "always allow".
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
    assert!(request.options.iter().any(|o| o.id == CHOICE_ALWAYS_ALLOW));
    assert!(request.prompt.contains("ls"), "prompt names the scope");

    let delivered = broker.answer(
        session,
        ChoiceResponse::Selected {
            request_id: request.request_id.clone(),
            option_id: CHOICE_ALWAYS_ALLOW.into(),
        },
    );
    assert!(delivered);
    assert!(approval.await.unwrap().is_ok());

    // Hook received the scoped rule: tool + the exact `ls` command.
    {
        let snapshot = persisted.lock().unwrap().clone();
        assert_eq!(snapshot.as_slice(), [("bash", Some("ls".to_string()))]);
    }

    // Same command again: no prompt, no hook re-fire.
    let result = broker
        .approve_tool(Uuid::new_v4(), names::BASH, &json!({"command": "ls"}), &tx)
        .await;
    assert!(result.is_ok());
    assert!(
        rx.try_recv().is_err(),
        "allowed command must not prompt again"
    );

    // A different command is NOT covered by the scoped rule: it must ask.
    let other_session = Uuid::new_v4();
    let other = tokio::spawn({
        let broker = broker.clone();
        let tx = tx.clone();
        async move {
            broker
                .approve_tool(
                    other_session,
                    names::BASH,
                    &json!({"command": "rm -rf /tmp/x"}),
                    &tx,
                )
                .await
        }
    });
    let stream_event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("choice request within timeout")
        .expect("other command must prompt");
    let StreamEvent::ChoiceRequest(other_request) = stream_event else {
        panic!("expected a ChoiceRequest for the unscoped command");
    };
    assert!(
        other_request.prompt.contains("rm -rf"),
        "prompt names the new scope"
    );
    // Deny resolves the pending approval.
    let delivered = broker.answer(
        other_session,
        ChoiceResponse::Selected {
            request_id: other_request.request_id.clone(),
            option_id: "deny".into(),
        },
    );
    assert!(delivered);
    assert!(other.await.unwrap().is_err(), "deny rejects the tool call");
}
