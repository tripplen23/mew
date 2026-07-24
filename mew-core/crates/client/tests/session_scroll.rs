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
        compaction_summary: None,
        compacted_up_to: None,
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

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
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

/// The input box must grow with the text. A short message sits in a 3-line
/// box; a long wrapped message expands the box so every line of the input
/// is visible (instead of clipping at the right edge like the old
/// fixed-3-line layout did).
#[test]
fn input_box_grows_with_wrapped_text() {
    let mut app = app_with_long_transcript();

    // Now type a long line that wraps many times.
    type_text(&mut app, &"a".repeat(400));
    let buf = draw(&mut app);

    // And the long text must actually be visible in the buffer (not clipped).
    // We check for a run shorter than the wrap width (38 = 40 terminal - 2 borders)
    // because each wrapped row holds 38 a's, separated by line terminators in
    // the TestBackend's `to_string()` output.
    assert!(
        buf.lines()
            .filter(|line| line.contains(&"a".repeat(30)))
            .count()
            > 1,
        "the long input must wrap into multiple rendered rows, not clip off the right"
    );
}

#[test]
fn input_has_no_placeholder_after_typing() {
    let mut app = App::new();
    type_text(&mut app, "quit");
    let typed = draw(&mut app);

    assert!(typed.contains("quit"), "typed text is visible:\n{typed}");
    assert!(
        !typed.contains("mewcode to build"),
        "input must not render stale placeholder text:\n{typed}"
    );
}
