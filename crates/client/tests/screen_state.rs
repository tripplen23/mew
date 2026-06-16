//! Property 7: No illegal screen state.
//!
//! Each `Screen` variant can only be built with its required data. This is a
//! compile-time guarantee — the fact this file compiles already proves it —
//! exercised here by constructing every variant and asserting it holds the
//! data it was given.

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{HomeState, NewSessionState, Screen, SessionState};
use mewcode_protocol::{Mode, ModelId};

#[test]
fn home_variant_carries_its_state() {
    let screen = Screen::Home(HomeState::loading());
    match screen {
        Screen::Home(state) => assert!(state.loading && state.sessions.is_empty()),
        _ => panic!("expected Home"),
    }
}

#[test]
fn new_session_variant_carries_its_state() {
    let screen = Screen::NewSession(NewSessionState::default());
    assert!(matches!(screen, Screen::NewSession(_)));
}

#[test]
fn session_variant_cannot_exist_without_a_session() {
    // `SessionState::new` *requires* a `Session` argument — there is no way to
    // build a `Screen::Session` without hydrated data. That is the property.
    let session = Session {
        id: uuid::Uuid::new_v4(),
        title: "demo".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
    };
    let session_id = session.id;

    let screen = Screen::Session(SessionState::new(session));
    match screen {
        Screen::Session(state) => assert_eq!(state.session.id, session_id),
        _ => panic!("expected Session"),
    }
}
