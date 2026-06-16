use std::time::Duration;

/// Braille spinner frames, advanced one step roughly every 80 ms.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The spinner glyph for a turn that has been running for `elapsed`.
///
/// The frame index is derived from elapsed time, so redrawing on each 50 ms
/// tick advances the spinner.
///
/// ```
/// use std::time::Duration;
/// use mewcode_client::runtime::view::spinner_frame;
///
/// let a = spinner_frame(Duration::from_millis(0));
/// let b = spinner_frame(Duration::from_millis(80));
/// assert_ne!(a, b); // a tick later, a different frame
/// ```
pub fn spinner_frame(elapsed: Duration) -> &'static str {
    let idx = (elapsed.as_millis() / 80) as usize % SPINNER_FRAMES.len();
    SPINNER_FRAMES[idx]
}
