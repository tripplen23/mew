//! Word/line/character/byte counter — golden task 1 reference implementation.
//!
//! Ported from `tests/fixtures/wc.py`. All behavior must match the Python
//! baseline byte-for-byte for every valid UTF-8 input, including edge cases
//! (empty string, whitespace-only, trailing newlines).

/// Count words, lines, characters, and bytes in a string.
///
/// # Returns
///
/// A tuple of `(words, lines, chars, bytes)` where:
/// - `words`: number of whitespace-separated words
/// - `lines`: number of lines (trailing content without newline counts as one line)
/// - `chars`: number of Unicode scalar values (Rust `char` count)
/// - `bytes`: UTF-8 byte length of the input
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

    /// Compare with Python `wc()` baseline for a given input.
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
        // Python: "text\n" → text.count("\n") = 1, "text" doesn't end with \n → lines += 1 → 2
        assert_matches_python("text\n", (1, 2, 5, 5));
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
        // "héllo wörld" has 11 Unicode scalar values, 13 UTF-8 bytes
        assert_matches_python("héllo wörld", (2, 1, 11, 13));
    }

    #[test]
    fn mixed_content() {
        let input = "Hello, world!\nThis is a test.\n\nThird line.";
        assert_matches_python(input, (8, 4, 42, 42));
    }

    #[test]
    fn trailing_newline() {
        // "text\n" → 1 newline, ends with \n → lines = 1
        assert_matches_python("text\n", (1, 1, 5, 5));
    }

    #[test]
    fn multiple_trailing_newlines() {
        assert_matches_python("text\n\n", (1, 2, 6, 6));
    }

    #[test]
    fn embedded_whitespace() {
        assert_matches_python("  \n  \n  ", (0, 3, 10, 10));
    }
}
