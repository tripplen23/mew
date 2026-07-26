use mewcode_client::runtime::view::window_bounds;

#[test]
fn window_starts_at_the_block_containing_the_offset() {
    let (first, local, end) = window_bounds(&[5, 5, 5], 7, 4).expect("has content");
    assert_eq!((first, local), (1, 2));
    assert_eq!(end, 3);
}

#[test]
fn exact_block_boundary_starts_at_the_next_block() {
    let (first, local, end) = window_bounds(&[5, 5, 5], 5, 5).expect("has content");
    assert_eq!((first, local, end), (1, 0, 2));
}

#[test]
fn tall_first_block_is_entered_partway_without_pulling_in_neighbours() {
    let (first, local, end) = window_bounds(&[100, 5], 40, 10).expect("has content");
    assert_eq!((first, local), (0, 40));
    assert_eq!(end, 1, "the tall block alone already covers the viewport");
}

#[test]
fn viewport_taller_than_content_takes_everything() {
    let (first, local, end) = window_bounds(&[2, 3], 0, 50).expect("has content");
    assert_eq!((first, local, end), (0, 0, 2));
}

#[test]
fn nothing_to_render_is_reported_as_none() {
    assert!(window_bounds(&[], 0, 10).is_none(), "no blocks");
    assert!(window_bounds(&[5], 0, 0).is_none(), "zero viewport");
    assert!(window_bounds(&[5, 5], 10, 4).is_none(), "scrolled past end");
}

#[test]
fn zero_height_blocks_do_not_stall_the_walk() {
    let (first, local, end) = window_bounds(&[0, 0, 6], 2, 3).expect("has content");
    assert_eq!((first, local, end), (2, 2, 3));
}
