//! Context compaction domain.

mod history;
mod summary;

pub use history::{
    CHARS_PER_TOKEN, COMPACTION_PRESERVE_TURNS, COMPACTION_THRESHOLD, build_compacted_history,
    build_history_with_summary_tail, estimate_compacted_context, prune_messages,
    split_for_compaction,
};
pub use summary::{
    COMPACTION_STREAM_CHUNK_CHARS, CompactionResult, build_compaction_prompt,
    chunk_summary_for_streaming, compact_history, has_required_summary_sections,
    publish_validated_summary, select_authoritative_summary,
};
