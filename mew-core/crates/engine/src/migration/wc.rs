//! Word/line/character/byte counter.
//!
//! Ported from `tests/fixtures/wc.py`. All behavior must match the Python
//! baseline byte-for-byte for every valid UTF-8 input, including edge cases
//! (empty string, whitespace-only, trailing newlines).

/// Count words, lines, Unicode scalar values, and UTF-8 bytes in `text`.
///
/// Returns `(words, lines, chars, bytes)`. Behavior matches the Python
/// `wc.py` baseline: `str::lines()` semantics for line counting,
/// `str::split_whitespace()` for word boundaries.
pub fn wc(text: &str) -> (usize, usize, usize, usize) {
    if text.is_empty() {
        return (0, 0, 0, 0);
    }

    let lines = text.lines().count();
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    let byte_count = text.len();

    (words, lines, chars, byte_count)
}
