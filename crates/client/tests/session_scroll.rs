//! Session transcript scroll behaviour.
//!
//! The transcript auto-follows its latest line so a reply that overflows the
//! viewport is always visible (the bug this fixes: new answers scrolled off
//! the bottom with no way to reach them). Scrolling up with PageUp releases the
//! follow and reveals earlier history; scrolling back to the bottom re-engages
//! it. `scroll`/`max_scroll`/`viewport` are derived during rendering, so each
//! assertion renders first, then drives keys, then renders again.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{App, Msg, Screen, SessionState};
use mewcode_client::runtime::update;
use mewcode_client::runtime::view::render;
use mewcode_protocol::{Message, MessagePart, Mode, ModelId};

use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// An app sitting on a Session screen whose transcript far exceeds any small
/// viewport. The first user line says `line-00`, the last `line-39`.
fn app_with_long_transcript() -> App {
    let messages = (0..40)
        .map(|i| {
            Message::user(vec![MessagePart::Text {
                text: format!("line-{i:02}"),
            }])
        })
        .collect();
    let session = Session {
        id: Uuid::new_v4(),
        title: "scrolltest".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages,
    };
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session));
    app
}

fn draw(app: &mut App) -> String {
    // A short, narrow viewport so the 40-message transcript overflows it.
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    terminal.backend().to_string()
}

fn press(app: &mut App, code: KeyCode) {
    update(app, Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn press_until(app: &mut App, code: KeyCode, done: impl Fn(&SessionState) -> bool) {
    for _ in 0..200 {
        if done(session(app)) {
            return;
        }
        press(app, code);
    }
    panic!("scroll did not reach expected boundary");
}

fn session(app: &App) -> &SessionState {
    match &app.screen {
        Screen::Session(s) => s,
    }
}

#[test]
fn auto_follows_the_latest_line() {
    let mut app = app_with_long_transcript();
    let buf = draw(&mut app);

    assert!(
        buf.contains("line-39"),
        "latest line must be visible:\n{buf}"
    );
    assert!(
        !buf.contains("line-00"),
        "earliest line must be scrolled off:\n{buf}"
    );
    assert!(session(&app).follow, "starts in follow mode");
    assert!(
        session(&app).max_scroll > 0,
        "content overflows the viewport"
    );
}

#[test]
fn page_up_reveals_history_and_releases_follow() {
    let mut app = app_with_long_transcript();
    draw(&mut app); // populate max_scroll / viewport

    // Page up until the state reaches the very top.
    press_until(&mut app, KeyCode::PageUp, |s| s.scroll == 0);
    let buf = draw(&mut app);

    assert!(
        buf.contains("line-00"),
        "top of history must be visible:\n{buf}"
    );
    assert!(!session(&app).follow, "scrolling up releases follow");
    assert_eq!(session(&app).scroll, 0, "clamped at the top");
}

#[test]
fn page_down_to_bottom_re_engages_follow() {
    let mut app = app_with_long_transcript();
    draw(&mut app);
    press_until(&mut app, KeyCode::PageUp, |s| s.scroll == 0);
    draw(&mut app);
    assert!(!session(&app).follow);

    // Page back down until reaching the bottom re-engages follow.
    press_until(&mut app, KeyCode::PageDown, |s| s.follow);
    let buf = draw(&mut app);

    assert!(buf.contains("line-39"), "back at the latest line:\n{buf}");
    assert!(
        session(&app).follow,
        "reaching the bottom re-engages follow"
    );
}
