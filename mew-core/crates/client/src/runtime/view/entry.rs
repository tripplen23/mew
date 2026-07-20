//! Empty/start screen: the layout shown when no session is active yet.
//!
//! Picks the right size variant for the available `Rect` (mascot + `MEW`,
//! just `MEW`, or the word `mewcode`) and then always appends the
//! model/status line so the user knows which model the next message will
//! land on.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::super::model::SessionState;
use super::spinner::spinner_frame;
use super::theme::Theme;

const MEW_LOGO: [&str; 6] = [
    "███╗   ███╗███████╗██╗    ██╗",
    "████╗ ████║██╔════╝██║    ██║",
    "██╔████╔██║█████╗  ██║ █╗ ██║",
    "██║╚██╔╝██║██╔══╝  ██║███╗██║",
    "██║ ╚═╝ ██║███████╗╚███╔███╔╝",
    "╚═╝     ╚═╝╚══════╝ ╚══╝╚══╝ ",
];

pub(super) fn render_entry_lines(
    s: &SessionState,
    theme: Theme,
    inner: Rect,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    if inner.height > 24 {
        out.extend((0..inner.height.saturating_sub(22) / 3).map(|_| Line::from("")));
    }
    if inner.width >= 60 && inner.height >= 23 {
        for line in [
            "⣿⣿⣿⣿⣿⣿⠟⢁⣴⣿⣿⠁⢸⣿⣿⣿⣯⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿",
            "⣿⣿⣿⣿⡿⠋⣠⣾⣿⣿⣿⡆⠘⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿",
            "⣿⣿⣿⡟⢁⣼⣿⣿⣿⣿⣿⡿⠀⠉⣼⣿⠿⠿⡟⣿⣿⣿⣿⣿⠛⡻⣋⠉⠛",
            "⣿⣿⡟⢀⣾⣿⣿⣿⣿⡿⠟⢠⡆⢰⣿⠇⡾⠀⠸⣼⣿⣿⣿⣿⣖⠐⡇⣥⣼",
            "⣿⡟⢀⣾⣿⠿⠿⠋⢉⣠⣾⣿⡇⠸⣿⡌⢧⢸⡇⣿⣿⣿⣿⣿⣧⠸⠿⣘⣩",
            "⡿⠁⣾⣿⠇⣠⣶⣿⣿⣿⣿⣿⣷⣄⠙⠻⣷⣶⣴⣾⣿⣿⣿⣿⣿⣿⢿⡿⠟",
            "⡇⢰⣿⡏⢠⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣦⣄⡀⠐⢿⣿⣿⣿⣿⡿⠃⣠⣶⣇",
            "⠁⣼⣿⠀⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠟⠋⣁⣤⣶⣬⣭⣭⣥⣴⣿⣿⣿⣿",
            "⠀⢿⣿⠀⣿⣿⣿⣿⣿⣿⣿⣿⣿⠋⠠⣶⣿⣿⡿⠿⠟⠓⠈⣿⣿⣿⣿⣿⣍",
            "⡆⢸⣿⡄⠹⣿⣿⣿⣿⣿⣿⣿⣿⣧⡄⢉⣁⣤⠤⠒⣀⠄⢀⣿⣿⣿⣿⣿⣿",
            "⣧⠀⢿⣿⡄⠙⢿⣿⣿⣿⣿⣿⣿⣿⡿⠛⢋⣡⠔⠊⣡⠀⣼⣿⣿⣿⣿⣿⣿",
            "⣿⣧⡀⠻⢿⣶⣤⣈⣉⣉⣉⣉⣩⣤⠴⠚⢉⣠⣴⡿⠃⢠⣿⣿⣿⣿⣿⣿⣿",
        ] {
            out.push(Line::from(Span::styled(
                line,
                Style::default()
                    .fg(Color::Rgb(255, 190, 220))
                    .add_modifier(Modifier::BOLD),
            )));
        }
        out.push(Line::from(""));
        for line in MEW_LOGO {
            out.push(Line::from(Span::styled(
                line,
                Style::default()
                    .fg(theme.mew_gold)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        out.push(Line::from(""));
    } else if inner.width >= 48 && inner.height >= 10 {
        for line in MEW_LOGO {
            out.push(Line::from(Span::styled(
                line,
                Style::default()
                    .fg(theme.hot_pink)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        out.push(Line::from(""));
    } else {
        out.push(Line::from(Span::styled(
            "mewcode",
            Style::default()
                .fg(theme.hot_pink)
                .add_modifier(Modifier::BOLD),
        )));
    }
    let status = if let Some(started) = s.creation_started_at {
        format!("{} starting session...", spinner_frame(started.elapsed()))
    } else {
        "Type a message to start a new session.".to_string()
    };
    let model = s.pending_model.unwrap_or_default().display_name();
    out.push(Line::from(vec![
        Span::styled("Build", Style::default().fg(theme.hot_pink)),
        Span::styled(" · ", Style::default().fg(theme.muted)),
        Span::styled(model, Style::default().fg(theme.text)),
        Span::styled(" · ", Style::default().fg(theme.muted)),
        Span::styled(status, Style::default().fg(theme.muted)),
    ]));
    out
}
