use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::super::model::{Toast, ToastKind};

/// How long a toast stays fully visible before it begins to fade.
const TOAST_HOLD: Duration = Duration::from_millis(3000);
/// Fade-out duration after the hold window.
const TOAST_FADE: Duration = Duration::from_millis(1000);

/// Opacity of a toast that has been showing for `elapsed`: fully opaque during
/// the hold window, then eased down to 0 over the fade window. The value
/// changes on each tick once fading begins, driving the animation.
///
/// ```
/// use std::time::Duration;
/// use mewcode_client::runtime::view::toast_alpha;
///
/// assert_eq!(toast_alpha(Duration::from_millis(0)), 1.0);
/// assert!(toast_alpha(Duration::from_millis(10_000)) <= 0.0);
/// ```
pub fn toast_alpha(elapsed: Duration) -> f32 {
    if elapsed <= TOAST_HOLD {
        return 1.0;
    }
    let into_fade = (elapsed - TOAST_HOLD).as_secs_f32();
    let fade = TOAST_FADE.as_secs_f32();
    if into_fade >= fade {
        return 0.0;
    }
    // Ease the fade with a sine-out curve.
    let progress = into_fade / fade;
    1.0 - (progress * std::f32::consts::FRAC_PI_2).sin()
}

/// Draw the active toast as a banner along the top of the screen, dimmed as it
/// fades.
pub(super) fn render_toast(frame: &mut Frame, area: Rect, toast: &Toast) {
    let alpha = toast_alpha(toast.started_at.elapsed());
    if alpha <= 0.0 {
        return;
    }

    let base = match toast.kind {
        ToastKind::Error => Color::Red,
        ToastKind::Info => Color::Blue,
    };
    let style = if alpha < 0.5 {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(base).add_modifier(Modifier::BOLD)
    };

    let width = (UnicodeWidthStr::width(toast.text.as_str()) as u16 + 4).min(area.width);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width),
        y: area.y,
        width,
        height: 1,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(Span::styled(format!(" {} ", toast.text), style)),
        rect,
    );
}
