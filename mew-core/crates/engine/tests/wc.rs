use mewcode_engine::wc::wc;

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
    assert_matches_python("héllo wörld", (2, 1, 11, 13));
}

#[test]
fn mixed_content() {
    let input = "Hello, world!\nThis is a test.\n\nThird line.";
    assert_matches_python(input, (8, 4, 42, 42));
}
