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
        todos: vec![],
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

/// Regression: `render_transcript` used to re-render every committed
/// message (markdown parse + syntect highlight) on every single frame,
/// including every PageUp/PageDown keystroke and every 50ms spinner tick,
/// even though history that has already been rendered never changes. This
/// asserts the memoization actually kicks in: after the first render, a
/// scroll key must not grow the transcript cache — it should already hold
/// one entry per committed message.
#[test]
fn scrolling_reuses_cached_transcript_lines() {
    let mut app = app_with_long_transcript();
    draw(&mut app); // first render populates the cache

    let cached_after_first_draw = session(&app).transcript_cache.message_cache_len();
    assert_eq!(
        cached_after_first_draw, 40,
        "first render should cache all 40 committed messages"
    );

    press(&mut app, KeyCode::PageUp);
    draw(&mut app);
    press(&mut app, KeyCode::PageDown);
    draw(&mut app);

    assert_eq!(
        session(&app).transcript_cache.message_cache_len(),
        40,
        "scrolling must not grow the cache — no new content was added"
    );
}

/// Switching to a different session must drop the previous session's
/// cached lines rather than accumulating them forever (and rather than
/// ever serving stale rendered content for a message id that happens to
/// collide, though ids are UUIDs so that's academic).
#[test]
fn opening_a_new_session_clears_the_old_transcript_cache() {
    let mut app = app_with_long_transcript();
    draw(&mut app);
    assert_eq!(session(&app).transcript_cache.message_cache_len(), 40);

    let fresh = Session {
        id: Uuid::new_v4(),
        title: "second".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![Message::user(vec![MessagePart::Text {
            text: "only-one".to_string(),
        }])],
        compaction_summary: None,
        compacted_up_to: None,
        todos: vec![],
    };
    app.screen = Screen::Session(SessionState::new(fresh));
    draw(&mut app);

    assert_eq!(
        session(&app).transcript_cache.message_cache_len(),
        1,
        "opening a different session must drop the old session's cached lines"
    );
}

/// Windowed (virtualized) rendering only materializes the blocks touching
/// the viewport and offsets into the first one with a *local* scroll value.
/// The bug that would introduce is an off-by-one in that local offset, which
/// shows up as content jumping or repeating instead of advancing smoothly.
///
/// Invariant asserted here: scrolling down exactly one row must shift the
/// visible frame by exactly one row — the new top row equals the previous
/// frame's second row. This holds across block boundaries, so it catches a
/// mis-computed local offset on the partially-visible first block.
#[test]
fn scrolling_one_row_shifts_the_frame_by_exactly_one_row() {
    let mut app = app_with_long_transcript();
    draw(&mut app);
    press_until(&mut app, KeyCode::PageUp, |s| s.scroll == 0);

    let rows = |app: &mut App| -> Vec<String> { draw(app).lines().map(str::to_string).collect() };

    let mut previous = rows(&mut app);
    // Walk far enough to cross several message boundaries.
    for step in 0..40 {
        let before_scroll = session(&app).scroll;
        press(&mut app, KeyCode::Down);
        let after_scroll = session(&app).scroll;
        if after_scroll == before_scroll {
            break; // clamped at the bottom
        }
        assert_eq!(
            after_scroll,
            before_scroll + 1,
            "Down must advance the offset by one row"
        );

        let current = rows(&mut app);
        assert_eq!(
            current[0], previous[1],
            "step {step}: after scrolling one row the top row must be the previous \
             frame's second row (local offset off by one?)"
        );
        previous = current;
    }
}

/// The transcript must stay correct at the extremes after virtualization:
/// the very first line is reachable at the top, and follow mode still pins
/// the very last line at the bottom.
#[test]
fn windowing_still_reaches_both_ends_of_the_transcript() {
    let mut app = app_with_long_transcript();
    draw(&mut app);

    press_until(&mut app, KeyCode::PageUp, |s| s.scroll == 0);
    let top = draw(&mut app);
    assert!(
        top.contains("line-00"),
        "top must show the first line:\n{top}"
    );

    press_until(&mut app, KeyCode::PageDown, |s| s.follow);
    let bottom = draw(&mut app);
    assert!(
        bottom.contains("line-39"),
        "bottom must show the last line:\n{bottom}"
    );
}
