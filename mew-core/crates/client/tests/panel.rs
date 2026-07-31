//! Tests for the overlay-panel scroll helper.

use mewcode_client::runtime::view::scroll_start_for_cursor;

#[test]
fn scroll_start_keeps_cursor_visible() {
    assert_eq!(scroll_start_for_cursor(0, 3, 10), 0);
    assert_eq!(scroll_start_for_cursor(2, 3, 10), 0);
    assert_eq!(scroll_start_for_cursor(3, 3, 10), 1);
    assert_eq!(scroll_start_for_cursor(9, 3, 10), 7);
    assert_eq!(scroll_start_for_cursor(9, 20, 10), 0);
    assert_eq!(scroll_start_for_cursor(9, 0, 10), 0);
}
