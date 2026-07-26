use ratatui::text::Line;
use uuid::Uuid;

use mewcode_client::runtime::model::TranscriptCache;

#[test]
fn reuses_cached_lines_for_same_id_and_width_only() {
    let mut cache = TranscriptCache::default();
    let id = Uuid::new_v4();
    let mut calls = 0;
    let render = |calls: &mut i32| {
        *calls += 1;
        (vec![Line::from("rendered")], 1)
    };

    let block = cache.message_lines(id, 80, || render(&mut calls));
    assert_eq!(block.height, 1, "cached block carries its wrapped height");
    cache.message_lines(id, 80, || render(&mut calls));
    assert_eq!(calls, 1, "same id + width must hit the cache");

    cache.message_lines(id, 40, || render(&mut calls));
    assert_eq!(calls, 2, "a different width must invalidate the entry");
}

#[test]
fn sync_session_drops_stale_entries_on_session_change() {
    let mut cache = TranscriptCache::default();
    let first = Uuid::new_v4();
    let id = Uuid::new_v4();
    cache.sync_session(first);
    cache.message_lines(id, 80, || (vec![Line::from("x")], 1));
    assert_eq!(cache.message_cache_len(), 1);

    cache.sync_session(Uuid::new_v4());
    assert_eq!(
        cache.message_cache_len(),
        0,
        "switching sessions must clear the previous session's cache"
    );
}

#[test]
fn compaction_cache_reuses_lines_for_same_index_and_width_only() {
    let mut cache = TranscriptCache::default();
    let mut calls = 0;
    let render = |calls: &mut i32| {
        *calls += 1;
        (vec![Line::from("compaction")], 1)
    };

    cache.compaction_lines(0, 1, 80, || render(&mut calls));
    cache.compaction_lines(0, 1, 80, || render(&mut calls));
    assert_eq!(calls, 1, "same index + width must hit the cache");

    cache.compaction_lines(0, 1, 40, || render(&mut calls));
    assert_eq!(calls, 2, "a different width must invalidate the entry");
}

#[test]
#[should_panic(expected = "append-only")]
fn compaction_cache_asserts_committed_len_never_shrinks() {
    let mut cache = TranscriptCache::default();
    cache.compaction_lines(0, 3, 80, || (vec![Line::from("x")], 1));
    cache.compaction_lines(0, 1, 80, || (vec![Line::from("y")], 1));
}
