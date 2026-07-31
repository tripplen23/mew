use std::collections::HashMap;
use std::rc::Rc;

use ratatui::text::Line;
use uuid::Uuid;

/// Caches rendered lines for committed transcript history.
///
/// Markdown parse + syntect highlight is expensive and used to repeat on every
/// frame for the entire transcript. Only committed messages are cached; the
/// in-flight turn is re-rendered each frame as before.
///
/// No eviction, per-session growth matching `session.messages`. Each entry
/// keeps the width it was last rendered at; a resize misses and overwrites
/// in place, so multi-width churn never accumulates (measured, see
/// `render_perf.rs::measure_cache_growth_across_widths`).
#[derive(Debug, Default)]
pub struct TranscriptCache {
    /// Rendered message lines keyed by `(id, width)`; hits clone via `Rc`.
    messages: HashMap<Uuid, (u16, CachedBlock)>,
    /// Rendered compaction-entry lines keyed by `(index, width)`. Positional
    /// keys assume `committed` never shrinks; `sync_session` clears on switch.
    compaction: HashMap<usize, (u16, CachedBlock)>,
    /// Session that owns this cache; a mismatch marks stale entries.
    session_id: Option<Uuid>,
    /// Peak `committed.len()` — asserts the append-only invariant (debug only).
    #[cfg(debug_assertions)]
    max_compaction_len_seen: usize,
}

/// Rendered lines for one transcript block, plus its wrapped row count.
///
/// `height` enables virtualized rendering: sum heights to derive scroll
/// extent instead of wrapping the whole transcript every frame. Clone is
/// cheap — an `Rc` bump plus a `u16`.
#[derive(Debug, Clone)]
pub struct CachedBlock {
    /// Rendered lines for this block.
    pub lines: Rc<Vec<Line<'static>>>,
    /// Wrapped height in terminal rows at the cached width.
    pub height: u16,
}

/// Look up `key` in `map`, falling back to `render()` on a miss or a
/// width mismatch. Shared by [`TranscriptCache::message_lines`] and
/// [`TranscriptCache::compaction_lines`] so the hit/miss logic can't drift
/// between the two as it evolves.
///
/// `render` returns the lines *and* their wrapped height; wrapping is the
/// view's concern, so the model never needs to know about ratatui layout.
fn cached_lines<K: std::hash::Hash + Eq>(
    map: &mut HashMap<K, (u16, CachedBlock)>,
    key: K,
    width: u16,
    render: impl FnOnce() -> (Vec<Line<'static>>, u16),
) -> CachedBlock {
    if let Some((cached_width, block)) = map.get(&key) {
        if *cached_width == width {
            return block.clone();
        }
    }
    let (lines, height) = render();
    let block = CachedBlock {
        lines: Rc::new(lines),
        height,
    };
    map.insert(key, (width, block.clone()));
    block
}

impl TranscriptCache {
    /// Drop every cached entry if `session_id` no longer matches — called
    /// once per render before any lookup/insert.
    pub fn sync_session(&mut self, session_id: Uuid) {
        if self.session_id != Some(session_id) {
            self.messages.clear();
            self.compaction.clear();
            self.session_id = Some(session_id);
            #[cfg(debug_assertions)]
            {
                self.max_compaction_len_seen = 0;
            }
        }
    }

    /// Look up or compute the rendered lines for one committed message.
    pub fn message_lines(
        &mut self,
        id: Uuid,
        width: u16,
        render: impl FnOnce() -> (Vec<Line<'static>>, u16),
    ) -> CachedBlock {
        cached_lines(&mut self.messages, id, width, render)
    }

    /// Look up or compute the rendered lines for one committed compaction
    /// entry, indexed by its position in `committed`. `committed_len` is
    /// the current length of `CompactionUiState::committed`, used only to
    /// assert (debug builds) that it never shrinks — the invariant this
    /// cache's positional keys depend on.
    pub fn compaction_lines(
        &mut self,
        index: usize,
        #[cfg_attr(not(debug_assertions), allow(unused_variables))] committed_len: usize,
        width: u16,
        render: impl FnOnce() -> (Vec<Line<'static>>, u16),
    ) -> CachedBlock {
        #[cfg(debug_assertions)]
        {
            debug_assert!(
                committed_len >= self.max_compaction_len_seen,
                "compaction.committed must be append-only for positional cache keys to stay valid"
            );
            self.max_compaction_len_seen = committed_len.max(self.max_compaction_len_seen);
        }
        cached_lines(&mut self.compaction, index, width, render)
    }

    /// Number of cached message entries.`#[doc(hidden)]` keeps it out of
    /// public docs while still allowing that cross-crate test visibility.
    #[doc(hidden)]
    pub fn message_cache_len(&self) -> usize {
        self.messages.len()
    }
}
