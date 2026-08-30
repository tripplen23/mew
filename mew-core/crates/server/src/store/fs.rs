//! Filesystem-backed [`SessionStore`] implementation.
//!
//! This is the default, persistent backend: one directory per session under a
//! per-user data dir. Each `sessions/<uuid>/` directory holds a `meta.json`
//! (session metadata, written atomically) and an append-only `messages.jsonl`
//! (one JSON-encoded [`Message`] per line, in append order).
//!
//! > Idiom: atomic write = temp file + `rename`. The full new contents are
//! > written to a sibling temp file, then `rename`d onto the target. POSIX
//! > `rename` is atomic within a filesystem, so a reader always observes either
//! > the previous complete file or the new complete file — never a torn write.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mewcode_protocol::env::MEWCODE_DATA_DIR;
use mewcode_protocol::{Message, Mode, ModelKind, ModelRef};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{
    Backend, CompactionPatch, NewSession, Session, SessionPatch, SessionStore, SessionSummary,
    StoreError, compaction_patch, validate_compaction_checkpoint, validate_title,
};

/// Name of the sessions subdirectory under the data dir.
const SESSIONS_SUBDIR: &str = "sessions";
/// Name of the per-session metadata file.
const META_FILE: &str = "meta.json";
/// Name of the per-session append-only message log.
const MESSAGES_FILE: &str = "messages.jsonl";

/// On-disk shape of `meta.json`.
///
/// `model` and `mode` serialize to exactly their wire forms (the `ModelId`
/// provider-id string and the `UPPERCASE` `Mode` label), satisfying the
/// documented layout while keeping a single source of truth for the mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaJson {
    /// Unique session identifier.
    id: Uuid,
    /// Human-readable title.
    title: String,
    /// Model selected for the session (serializes to its persistence identity).
    model: ModelRef,
    /// Runtime transport captured from provider discovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_kind: Option<ModelKind>,
    /// Runtime context limit captured from provider discovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_context_length: Option<u64>,
    /// Interaction mode (serializes to `BUILD` / `PLAN`).
    mode: Mode,
    /// When the session was created.
    created_at: DateTime<Utc>,
    /// When the session was last updated.
    updated_at: DateTime<Utc>,
    /// Optional compaction summary from the last manual or automatic compaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compaction_summary: Option<String>,
    /// Message index already covered by `compaction_summary`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compacted_up_to: Option<usize>,
    /// Stable id of the message at `compacted_up_to - 1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compacted_up_to_message_id: Option<Uuid>,
}

impl MetaJson {
    /// Project metadata into a wire [`SessionSummary`].
    fn to_summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id,
            title: self.title.clone(),
            model: self.model.clone(),
            model_kind: self.model_kind,
            model_context_length: self.model_context_length,
            mode: self.mode,
            created_at: self.created_at,
        }
    }

    /// Hydrate metadata into a full [`Session`] with the given messages.
    fn to_session(&self, messages: Vec<Message>) -> Session {
        Session {
            id: self.id,
            title: self.title.clone(),
            model: self.model.clone(),
            model_kind: self.model_kind,
            model_context_length: self.model_context_length,
            mode: self.mode,
            created_at: self.created_at,
            updated_at: self.updated_at,
            messages,
            todos: Vec::new(),
            compaction_summary: self.compaction_summary.clone(),
            compacted_up_to: self.compacted_up_to,
            compacted_up_to_message_id: self.compacted_up_to_message_id,
        }
    }
}

/// Filesystem-backed session store.
///
/// Writes are serialized by a single async [`Mutex`]; reads are lock-free
/// (they open and parse files independently). This is sufficient because
/// mewcode is a single-user tool.
#[derive(Debug)]
pub struct FsStore {
    /// Root data directory (contains the `sessions/` subdir).
    data_dir: PathBuf,
    /// Serializes all write operations; reads do not take this lock.
    write_lock: Mutex<()>,
}

/// Remove orphaned `.tmp` files older than 1 hour from a directory.
/// These are left behind if the process crashes between `File::create(tmp)`
/// and `rename(tmp, target)` in `write_meta_atomic`.
fn cleanup_stale_temps(dir: &Path) -> Result<(), StoreError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(()); // dir doesn't exist yet, nothing to clean
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            cleanup_stale_temps(&path)?;
            continue;
        }
        if path.extension().is_some_and(|e| e == "tmp") {
            // Only remove files older than 1 hour to avoid races with
            // running write operations.
            let age = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok());

            if age.is_some_and(|d| d > std::time::Duration::from_secs(3600)) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

impl FsStore {
    /// Build a store rooted at `data_dir`. The caller is responsible for
    /// ensuring the `sessions/` subdirectory exists (see `resolve_data_dir`).
    pub fn new(data_dir: PathBuf) -> Result<Self, StoreError> {
        // cleanup: remove orphaned .tmp files from crashed processes
        let _ = cleanup_stale_temps(&data_dir.join(SESSIONS_SUBDIR));
        Ok(Self {
            data_dir,
            write_lock: Mutex::new(()),
        })
    }

    /// The resolved data directory root.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Path to the `sessions/` directory.
    fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join(SESSIONS_SUBDIR)
    }

    /// Path to a single session's directory.
    fn session_dir(&self, id: Uuid) -> PathBuf {
        self.sessions_dir().join(id.to_string())
    }

    /// Atomically write `meta.json` into `dir` via temp-file-then-`rename`.
    fn write_meta_atomic(dir: &Path, meta: &MetaJson) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(meta)?;
        // A unique sibling temp file in the same directory, so the final
        // `rename` stays within one filesystem (and is therefore atomic).
        let tmp_path = dir.join(format!("{META_FILE}.{}.tmp", Uuid::new_v4()));
        let result = (|| -> Result<(), StoreError> {
            let mut tmp = File::create(&tmp_path)?;
            tmp.write_all(&bytes)?;
            tmp.sync_all()?;
            std::fs::rename(&tmp_path, dir.join(META_FILE))?;
            // Persist the directory entry update, not only the file contents.
            File::open(dir)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        result
    }
}

/// Read and parse a `meta.json` file.
///
/// A missing file surfaces as [`StoreError::NotFound`]; malformed contents
/// (corrupt JSON or values that cannot be parsed back into their domain types)
/// surface as [`StoreError::Invalid`] rather than leaking a raw serde error.
fn read_meta(meta_path: &Path) -> Result<MetaJson, StoreError> {
    let raw = match std::fs::read_to_string(meta_path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(StoreError::NotFound);
        }
        Err(e) => return Err(StoreError::Io(e)),
    };
    serde_json::from_str(&raw).map_err(|e| StoreError::Invalid(format!("corrupt {META_FILE}: {e}")))
}

/// Read and replay `messages.jsonl` in append order.
///
/// A missing file is treated as an empty history. An unparseable trailing
/// line is silently skipped (it may be a torn write from a concurrent
/// append). Lines before the last that cannot be parsed surface as
/// [`StoreError::Invalid`] — genuine corruption.
fn read_messages(messages_path: &Path) -> Result<Vec<Message>, StoreError> {
    let raw = match std::fs::read_to_string(messages_path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::Io(e)),
    };
    let lines: Vec<&str> = raw.lines().collect();
    let mut messages = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Message>(line) {
            Ok(msg) => messages.push(msg),
            Err(e) if i == lines.len() - 1 => {
                // ponytail: last line may be a torn concurrent write; skip it.
                tracing::warn!("ignoring unparseable trailing line in {MESSAGES_FILE}: {e}");
            }
            Err(e) => {
                return Err(StoreError::Invalid(format!(
                    "corrupt {MESSAGES_FILE} at line {}: {e}",
                    i + 1
                )));
            }
        }
    }
    Ok(messages)
}

#[async_trait]
impl SessionStore for FsStore {
    fn backend(&self) -> Backend {
        Backend::Filesystem
    }

    fn data_dir_path(&self) -> Option<PathBuf> {
        Some(self.data_dir.clone())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, StoreError> {
        let sessions_dir = self.sessions_dir();
        let entries = match std::fs::read_dir(&sessions_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(StoreError::Io(e)),
        };

        let mut summaries = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let meta_path = entry.path().join(META_FILE);
            // Scan metadata only — no message replay, so the list stays fast.
            match read_meta(&meta_path) {
                Ok(meta) => summaries.push(meta.to_summary()),
                // A directory without a readable meta.json is not a session.
                Err(StoreError::NotFound) => continue,
                // One corrupt session (stale model ref, etc.) must not break
                // the entire session listing. Skip it, log it.
                Err(e @ StoreError::Invalid(_)) => {
                    tracing::warn!(
                        session_dir = %entry.path().display(),
                        error = %e,
                        "skipping unreadable session in list"
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        // Newest-first.
        summaries.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        Ok(summaries)
    }

    async fn get_session(&self, id: Uuid) -> Result<Session, StoreError> {
        let dir = self.session_dir(id);
        // `read_meta` maps a missing file to `NotFound`.
        let meta = read_meta(&dir.join(META_FILE))?;
        // The JSONL sequence is canonical; timestamps are metadata only.
        let messages = read_messages(&dir.join(MESSAGES_FILE))?;
        Ok(meta.to_session(messages))
    }

    async fn create_session(&self, new: NewSession) -> Result<Session, StoreError> {
        let _guard = self.write_lock.lock().await;
        let now = Utc::now();
        let meta = MetaJson {
            id: Uuid::new_v4(),
            title: new.title,
            model: new.model,
            model_kind: new.model_kind,
            model_context_length: new.model_context_length,
            mode: new.mode,
            created_at: now,
            updated_at: now,
            compaction_summary: None,
            compacted_up_to: None,
            compacted_up_to_message_id: None,
        };

        let dir = self.session_dir(meta.id);
        std::fs::create_dir_all(&dir)?;
        let create_result = (|| -> Result<(), StoreError> {
            // `meta.json` is the discoverability/commit marker: create and
            // sync the empty log first, then publish metadata last.
            File::create(dir.join(MESSAGES_FILE))?.sync_all()?;
            Self::write_meta_atomic(&dir, &meta)?;
            File::open(self.sessions_dir())?.sync_all()?;
            Ok(())
        })();
        if let Err(error) = create_result {
            let _ = std::fs::remove_dir_all(&dir);
            let _ = File::open(self.sessions_dir()).and_then(|parent| parent.sync_all());
            return Err(error);
        }

        Ok(meta.to_session(Vec::new()))
    }

    async fn delete_session(&self, id: Uuid) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().await;
        let dir = self.session_dir(id);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    async fn patch_session(&self, id: Uuid, patch: SessionPatch) -> Result<Session, StoreError> {
        let compaction = compaction_patch(&patch)?;
        let _guard = self.write_lock.lock().await;
        let dir = self.session_dir(id);
        let mut meta = read_meta(&dir.join(META_FILE))?;
        let messages = read_messages(&dir.join(MESSAGES_FILE))?;
        if let CompactionPatch::Set {
            up_to, message_id, ..
        } = &compaction
        {
            validate_compaction_checkpoint(&messages, *up_to, *message_id)?;
        }
        if let Some(title) = patch.title {
            meta.title = validate_title(&title)?;
        }
        if let Some(model) = patch.model {
            meta.model = model;
            meta.model_kind = patch.model_kind;
            meta.model_context_length = patch.model_context_length;
        } else {
            if patch.model_kind.is_some() {
                meta.model_kind = patch.model_kind;
            }
            if patch.model_context_length.is_some() {
                meta.model_context_length = patch.model_context_length;
            }
        }
        if let Some(mode) = patch.mode {
            meta.mode = mode;
        }
        match compaction {
            CompactionPatch::Unchanged => {}
            CompactionPatch::Clear => {
                meta.compaction_summary = None;
                meta.compacted_up_to = None;
                meta.compacted_up_to_message_id = None;
            }
            CompactionPatch::Set {
                summary,
                up_to,
                message_id,
            } => {
                meta.compaction_summary = Some(summary);
                meta.compacted_up_to = Some(up_to);
                meta.compacted_up_to_message_id = Some(message_id);
            }
        }
        meta.updated_at = Utc::now();
        Self::write_meta_atomic(&dir, &meta)?;
        Ok(meta.to_session(messages))
    }

    async fn append_message(&self, id: Uuid, message: Message) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().await;
        let dir = self.session_dir(id);
        // Load current metadata (also maps a missing session to `NotFound`).
        let mut meta = read_meta(&dir.join(META_FILE))?;

        // Commit metadata first: after the message log is synced there must be
        // no fallible step that can report failure for an already-durable append.
        meta.updated_at = Utc::now();
        Self::write_meta_atomic(&dir, &meta)?;

        let line = serde_json::to_string(&message)?;
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(MESSAGES_FILE))?;
        writeln!(log, "{line}")?;
        log.sync_all()?;
        Ok(())
    }
}

/// Resolve the data directory from the documented precedence and ensure the
/// directory tree (including its `sessions/` subdir) exists.
///
/// Precedence:
/// 1. `MEWCODE_DATA_DIR`, if set and non-empty; else
/// 2. `$XDG_DATA_HOME/mew`; else
/// 3. `~/.local/share/mew`.
///
/// Steps 2 and 3 are resolved via [`dirs::data_dir`], which returns
/// `$XDG_DATA_HOME` when set and `~/.local/share` otherwise on Linux.
pub fn resolve_data_dir() -> Result<PathBuf, StoreError> {
    let dir = match std::env::var(MEWCODE_DATA_DIR) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => dirs::data_dir()
            .ok_or_else(|| {
                StoreError::Invalid("could not resolve a default data directory".to_string())
            })?
            .join("mew"),
    };
    // Create the data dir and its `sessions/` subdir on first use.
    std::fs::create_dir_all(dir.join(SESSIONS_SUBDIR))?;
    Ok(dir)
}
