use mewcode_client::runtime::view::visual_cursor_pos;

#[test]
fn cursor_uses_paragraph_word_wrap() {
    let lines = vec!["hello world".to_string()];

    assert_eq!(visual_cursor_pos(&lines, 0, 6, 6), (1, 0));
    assert_eq!(visual_cursor_pos(&lines, 0, 11, 6), (1, 5));
}
