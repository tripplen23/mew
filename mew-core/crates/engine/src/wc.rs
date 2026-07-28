//! Word/line/character/byte counter — golden task 1 reference implementation.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_matches_python(input: &str, expected: (usize, usize, usize, usize)) {
        assert_eq!(wc(input), expected, "mismatch for input: {input:?}");
    }

    #[test]
    fn empty_string() {
        assert_matches_python("", (0, 0, 0, 0));
    }

    #[test]
    fn single_word() {
        assert_matches_python("hello", (1, 1, 5, 5));
    }

    #[test]
    fn simple_sentence() {
        assert_matches_python("hello world", (2, 1, 11, 11));
    }

    #[test]
    fn multiline() {
        assert_matches_python("line1\nline2\nline3", (3, 3, 17, 17));
    }

    #[test]
    fn trailing_newline() {
        // Only the terminator '\n' is consumed; no implicit empty line appended.
        assert_matches_python("text\n", (1, 1, 5, 5));
    }

    #[test]
    fn multiple_trailing_newlines() {
        assert_matches_python("text\n\n", (1, 2, 6, 6));
    }

    #[test]
    fn whitespace_only() {
        assert_matches_python("   \n  \n   ", (0, 3, 10, 10));
    }

    #[test]
    fn empty_lines() {
        assert_matches_python("\n\n\n", (0, 3, 3, 3));
    }

    #[test]
    fn unicode() {
        // 11 Unicode scalar values, 13 UTF-8 bytes.
        assert_matches_python("héllo wörld", (2, 1, 11, 13));
    }

    #[test]
    fn mixed_content() {
        let input = "Hello, world!\nThis is a test.\n\nThird line.";
        assert_matches_python(input, (8, 4, 42, 42));
    }
}
