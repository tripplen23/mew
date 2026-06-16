//! Home-screen snapshot test.
//!
//! Renders an empty (loaded, no-sessions) Home screen into a ratatui
//! `TestBackend` and pins the resulting buffer with `insta`. This locks the
//! empty-list affordance ("No sessions yet. / Press 'n' to start a new one.")
//! against accidental regressions.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use mewcode_client::runtime::model::{App, HomeState, Screen};
use mewcode_client::runtime::view::render;

#[test]
fn empty_home() {
    let mut app = App::new();
    // An empty Home: not loading, no sessions.
    app.screen = Screen::Home(HomeState {
        sessions: Vec::new(),
        selected: 0,
        loading: false,
    });

    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    terminal.draw(|frame| render(frame, &app)).unwrap();

    insta::assert_snapshot!(terminal.backend().to_string());
}
