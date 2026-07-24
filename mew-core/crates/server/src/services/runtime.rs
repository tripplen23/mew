use std::path::PathBuf;

use mewcode_engine::memory::MemoryStore;

pub(crate) fn project_root() -> PathBuf {
    std::env::current_dir()
        .or_else(|_| std::fs::canonicalize("."))
        .unwrap_or_else(|_| ".".into())
}

pub(crate) fn project_memory(base: &MemoryStore, root: &std::path::Path) -> MemoryStore {
    match base.data_dir() {
        Some(data_dir) => MemoryStore::for_project(data_dir.to_path_buf(), root),
        None => base.clone(),
    }
}
