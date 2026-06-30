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

/// The input box must grow with the text. A short message sits in a 3-line
/// box; a long wrapped message expands the box so every line of the input
/// is visible (instead of clipping at the right edge like the old
/// fixed-3-line layout did).
#[test]
fn input_box_grows_with_wrapped_text() {
    let mut app = app_with_long_transcript();

    // Render once with empty input so the cursor settles at the baseline.
    draw(&mut app);
    let height_short = input_box_height(&draw(&mut app));

    // Now type a long line that wraps many times.
    for c in "a".repeat(400).chars() {
        press(&mut app, KeyCode::Char(c));
    }
    let height_long = input_box_height(&draw(&mut app));

    assert!(
        height_long > height_short,
        "input box must grow when text wraps: short={height_short}, long={height_long}"
    );
    // And the long text must actually be visible in the buffer (not clipped).
    // We check for a run shorter than the wrap width (38 = 40 terminal - 2 borders)
    // because each wrapped row holds 38 a's, separated by line terminators in
    // the TestBackend's `to_string()` output.
    let buf = draw(&mut app);
    assert!(
        buf.contains(&"a".repeat(30)),
        "the long input must be in the rendered buffer, not clipped off the right"
    );
}

/// The terminal cursor must follow the **visual** wrap, not the text position.
/// A long single line that wraps to 2 visual rows has the textarea's
/// `cursor()` at `(0, 80)` — the row is still 0 in the text, even though
/// visually the character is on row 1. `park_cursor_in_field` has to map
/// that to the second visual row, otherwise the blink sits at the end of
/// the first wrapped line and the user has to guess where the next char
/// will land.
#[test]
fn cursor_follows_visual_wrap_not_text_position() {
    let mut app = app_with_long_transcript();

    // Type a single long line that wraps in a 40-wide terminal: 38 chars
    // per wrapped row.
    for c in "a".repeat(80).chars() {
        press(&mut app, KeyCode::Char(c));
    }

    let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();

    // Find the input box top row.
    let buf = terminal.backend().to_string();
    let top = buf
        .lines()
        .position(|l| l.contains(" message "))
        .expect("input box top border missing");
    // The top border row is at `top`; inner row 0 is at `top + 1`, inner
    // row 1 is at `top + 2`, etc. 80 chars in a 38-wide inner box wraps to
    // ceil(80/38) = 3 visual rows (rows 0, 1, 2); the cursor must land on
    // the last one.
    let inner_y = top as u16 + 1;
    let expected_visual_row = 80_usize.div_ceil(38) - 1; // 0-indexed last row
    let expected_y = inner_y + expected_visual_row as u16;

    let pos = terminal.backend().cursor_position();
    assert_eq!(
        pos.y, expected_y,
        "cursor must follow the visual wrap; expected y={expected_y} (text row 0, visual row {expected_visual_row}), got y={}",
        pos.y
    );
}

/// Cursor on a multi-line text must follow the **text** row, not get stuck
/// on the last visual row of the previous text line. After typing two
/// text lines (`hello\nworld`) with the cursor at the end of the second,
/// the cursor lands on the second visual row inside the input box, not
/// on the first row's tail.
#[test]
fn cursor_on_second_text_line_is_on_second_visual_row() {
    let mut app = app_with_long_transcript();

    for c in "hello\nworld".chars() {
        press(&mut app, KeyCode::Char(c));
    }

    let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    let buf = terminal.backend().to_string();

    let top = buf
        .lines()
        .position(|l| l.contains(" message "))
        .expect("input box top border missing");
    // text row 1 → visual row 1. Inner first row is `top + 1`.
    let expected_y = top as u16 + 1 + 1; // top border + 1 + visual row 1

    let pos = terminal.backend().cursor_position();
    assert_eq!(
        pos.y, expected_y,
        "cursor on second text line should be on second visual row"
    );
}

/// After line 1 is full and the input wraps, the user types
/// "and" + space + "here" + space + "too" + space on the wrapped line 2.
/// All three spaces must be in the textarea — both the inter-word
/// spaces and the trailing one. The single space between two words is
/// the same path (input reader forwards `Release(space)`), so this
/// test covers the core fix too.
#[test]
fn spaces_on_wrapped_line_2_are_kept() {
    let mut app = app_with_long_transcript();

    let prefix = "this is a long line 1 message with many words to test space.";
    for c in prefix.chars() {
        press(&mut app, KeyCode::Char(c));
    }
    for c in "and here too ".chars() {
        press(&mut app, KeyCode::Char(c));
    }

    let s = session(&app);
    let text = s.input.lines().join("\n");
    let line2_start = text.find("and").expect("line 2 payload present");
    let line2 = &text[line2_start..];
    let space_count = line2.chars().filter(|c| *c == ' ').count();
    assert_eq!(
        space_count, 3,
        "expected 3 spaces in the line-2 payload, got {space_count} in {line2:?}"
    );
}

/// Count the number of rows of the rendered ` message ` input box. Walks
/// the buffer looking for the top/bottom border rows of the box and
/// returns the height in rows (including both borders).
fn input_box_height(buf: &str) -> u16 {
    let top = buf
        .lines()
        .position(|l| l.contains(" message "))
        .expect("input box top border missing");
    // Bottom border of the box is the next line that contains `└`. We
    // can't use `starts_with` because the TestBackend wraps every line
    // in quotes for `to_string()`.
    let bottom = buf
        .lines()
        .skip(top + 1)
        .position(|l| l.contains('└'))
        .map(|i| i + top + 1)
        .expect("input box bottom border missing");
    (bottom - top + 1) as u16
}
