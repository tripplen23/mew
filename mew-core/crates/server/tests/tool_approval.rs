use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mewcode_engine::memory::MemoryStore as FactStore;
use mewcode_protocol::StreamEvent;
use mewcode_protocol::event::{CHOICE_ALLOW_ONCE, ChoiceResponse, ChoiceResponseRequest};
use mewcode_protocol::routes::CHOICES;
use mewcode_protocol::tool::names;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: "test-key".into(),
        openai_api_key: None,
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
    }
}

fn state() -> AppState {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    AppState::new(test_config(), Arc::new(MemoryStore::default()), fact_store)
}

fn post_choice(req: &ChoiceResponseRequest) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(CHOICES)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(req).unwrap()))
        .unwrap()
}

async fn status(app: axum::Router, req: Request<Body>) -> StatusCode {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let _ = resp.into_body().collect().await.unwrap();
    status
}

#[tokio::test]
async fn choice_route_resolves_matching_pending_approval() {
    let state = state();
    let broker = state.approvals.clone();
    let app = build_app(state);
    let session_id = Uuid::new_v4();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let waiter = {
        let broker = broker.clone();
        tokio::spawn(async move {
            broker
                .approve_tool(
                    session_id,
                    names::WRITE_FILE,
                    &json!({"path": "x.txt"}),
                    &tx,
                )
                .await
        })
    };
    let request = match rx.recv().await.unwrap() {
        StreamEvent::ChoiceRequest(request) => request,
        other => panic!("unexpected event: {other:?}"),
    };

    let status = status(
        app,
        post_choice(&ChoiceResponseRequest {
            session_id,
            response: ChoiceResponse::Selected {
                request_id: request.request_id,
                option_id: CHOICE_ALLOW_ONCE.into(),
            },
        }),
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(waiter.await.unwrap().is_ok());
}

#[tokio::test]
async fn choice_route_rejects_missing_pending_approval() {
    let app = build_app(state());
    let status = status(
        app,
        post_choice(&ChoiceResponseRequest {
            session_id: Uuid::new_v4(),
            response: ChoiceResponse::Selected {
                request_id: "missing".into(),
                option_id: CHOICE_ALLOW_ONCE.into(),
            },
        }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_session_choice_response_does_not_consume_pending_approval() {
    let state = state();
    let broker = state.approvals.clone();
    let app = build_app(state);
    let session_id = Uuid::new_v4();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let waiter = {
        let broker = broker.clone();
        tokio::spawn(async move {
            broker
                .approve_tool(
                    session_id,
                    names::WRITE_FILE,
                    &json!({"path": "x.txt"}),
                    &tx,
                )
                .await
        })
    };
    let request = match rx.recv().await.unwrap() {
        StreamEvent::ChoiceRequest(request) => request,
        other => panic!("unexpected event: {other:?}"),
    };

    let wrong_session_status = status(
        app.clone(),
        post_choice(&ChoiceResponseRequest {
            session_id: Uuid::new_v4(),
            response: ChoiceResponse::Selected {
                request_id: request.request_id.clone(),
                option_id: CHOICE_ALLOW_ONCE.into(),
            },
        }),
    )
    .await;
    let right_session_status = status(
        app,
        post_choice(&ChoiceResponseRequest {
            session_id,
            response: ChoiceResponse::Selected {
                request_id: request.request_id,
                option_id: CHOICE_ALLOW_ONCE.into(),
            },
        }),
    )
    .await;

    assert_eq!(wrong_session_status, StatusCode::NOT_FOUND);
    assert_eq!(right_session_status, StatusCode::NO_CONTENT);
    assert!(waiter.await.unwrap().is_ok());
}
