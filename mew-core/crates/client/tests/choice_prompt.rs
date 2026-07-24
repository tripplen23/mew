use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{App, Cmd, Msg, Overlay, Screen, SessionState};
use mewcode_client::runtime::update;
use mewcode_protocol::event::{ChoiceCancelReason, ChoiceOption, ChoiceRequest, ChoiceResponse};
use mewcode_protocol::{Mode, ModelId};
use uuid::Uuid;

fn choice() -> ChoiceRequest {
    ChoiceRequest {
        request_id: "req-1".into(),
        title: "Pick one".into(),
        prompt: "Choose a path".into(),
        timeout_ms: 30_000,
        options: vec![
            ChoiceOption {
                id: "a".into(),
                label: "Alpha".into(),
                description: None,
            },
            ChoiceOption {
                id: "b".into(),
                label: "Beta".into(),
                description: Some("Second option".into()),
            },
        ],
    }
}

fn key(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn session(app: &mut App) -> &mut mewcode_client::runtime::model::SessionState {
    let Screen::Session(s) = &mut app.screen;
    s
}

fn app_with_session(session_id: Uuid) -> App {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(Session {
        id: session_id,
        title: "test".into(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
        compaction_summary: None,
        compacted_up_to: None,
    }));
    app
}

#[test]
fn choice_request_opens_modal_and_enter_returns_option_id() {
    let mut app = App::new();
    update(&mut app, Msg::ChoiceRequested(choice()));
    assert_eq!(session(&mut app).overlay, Overlay::Choice);

    update(&mut app, key(KeyCode::Down));
    update(&mut app, key(KeyCode::Enter));

    let state = session(&mut app).pending_choice.as_ref().unwrap();
    assert_eq!(
        state.response,
        Some(ChoiceResponse::Selected {
            request_id: "req-1".into(),
            option_id: "b".into(),
        })
    );
    assert_eq!(session(&mut app).overlay, Overlay::None);
}

#[test]
fn choice_request_esc_cancels() {
    let mut app = App::new();
    update(&mut app, Msg::ChoiceRequested(choice()));
    update(&mut app, key(KeyCode::Esc));

    let state = session(&mut app).pending_choice.as_ref().unwrap();
    assert_eq!(
        state.response,
        Some(ChoiceResponse::Cancelled {
            request_id: "req-1".into(),
            reason: ChoiceCancelReason::User,
        })
    );
    assert_eq!(session(&mut app).overlay, Overlay::None);
}

#[test]
fn choice_request_tick_timeout_cancels() {
    let mut request = choice();
    request.timeout_ms = 0;
    let mut app = App::new();
    update(&mut app, Msg::ChoiceRequested(request));
    update(&mut app, Msg::Tick);

    let state = session(&mut app).pending_choice.as_ref().unwrap();
    assert_eq!(
        state.response,
        Some(ChoiceResponse::Cancelled {
            request_id: "req-1".into(),
            reason: ChoiceCancelReason::Timeout,
        })
    );
    assert_eq!(session(&mut app).overlay, Overlay::None);
}

#[test]
fn choice_enter_submits_response_for_session() {
    let session_id = Uuid::new_v4();
    let mut app = app_with_session(session_id);
    update(&mut app, Msg::ChoiceRequested(choice()));
    let cmd = update(&mut app, key(KeyCode::Enter));

    match cmd {
        Cmd::SubmitChoice(req) => {
            assert_eq!(req.session_id, session_id);
            assert_eq!(
                req.response,
                ChoiceResponse::Selected {
                    request_id: "req-1".into(),
                    option_id: "a".into(),
                }
            );
        }
        other => panic!("unexpected cmd: {other:?}"),
    }
}
