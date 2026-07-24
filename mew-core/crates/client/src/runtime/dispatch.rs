use std::io;

use tokio::sync::mpsc;

use mewcode_protocol::tool::SKIPPED_DIRS;

use crate::net::{ApiClient, NetError};

use super::model::{Cmd, CreateError, FileEntry, Msg};
use super::stream::{run_chat_stream, run_compact_stream};

const NOTIFICATION_SOUND: &[u8] = include_bytes!("../../../../../assets/mew-voice.mp3");
const NOTIFICATION_SOUND_FILE: &str = "mew-voice.mp3";
const NOTIFICATION_PLAYER: &str = "ffplay";

pub(crate) fn dispatch(cmd: Cmd, api: &ApiClient, tx: &mpsc::Sender<Msg>) {
    match cmd {
        Cmd::None | Cmd::Quit => {}
        Cmd::CreateSession(req) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api
                    .create_session(&req)
                    .await
                    .map_err(create_error_from_net);
                let _ = tx.send(Msg::SessionCreated(result)).await;
            });
        }
        Cmd::StartChat(req) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(run_chat_stream(api, req, tx));
        }
        Cmd::SubmitChoice(req) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api.respond_choice(&req).await.map_err(|e| e.to_string());
                let _ = tx.send(Msg::ChoiceSubmitted(result)).await;
            });
        }
        Cmd::FetchModels => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api
                    .providers()
                    .await
                    .map(|providers| {
                        providers
                            .into_iter()
                            .filter(|provider| provider.available)
                            .flat_map(|provider| provider.models)
                            .collect()
                    })
                    .map_err(|e| e.to_string());
                let _ = tx.send(Msg::ModelsFetched(result)).await;
            });
        }
        Cmd::FetchSkills => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api.skills().await.map_err(|e| e.to_string());
                let _ = tx.send(Msg::SkillsFetched(result)).await;
            });
        }
        Cmd::FetchSessions => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api.list_sessions().await.map_err(|e| e.to_string());
                let _ = tx.send(Msg::SessionsFetched(result)).await;
            });
        }
        Cmd::FetchFiles => {
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                let result = list_files().map_err(|e| e.to_string());
                let _ = tx.blocking_send(Msg::FilesFetched(result));
            });
        }
        Cmd::PatchSession {
            id,
            patch,
            from_rename,
        } => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api
                    .patch_session(id, &patch)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(Msg::SessionPatched(result, from_rename)).await;
            });
        }
        Cmd::OpenSession(id) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api.get_session(id).await.map_err(|e| e.to_string());
                let _ = tx.send(Msg::SessionOpened(result)).await;
            });
        }
        Cmd::DeleteSession(id) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api
                    .delete_session(id)
                    .await
                    .map(|()| id)
                    .map_err(|e| e.to_string());
                let _ = tx.send(Msg::SessionDeleted(result)).await;
            });
        }
        Cmd::Compact(id) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(run_compact_stream(api, id, tx));
        }
        Cmd::PlayNotificationSound => {
            tokio::task::spawn_blocking(|| {
                play_notification_sound();
            });
        }
        Cmd::Batch(cmds) => {
            for c in cmds {
                dispatch(c, api, tx);
            }
        }
    }
}

fn list_files() -> io::Result<Vec<FileEntry>> {
    const MAX_FILES: usize = 2000;
    let root = std::env::current_dir()?;
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIPPED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            if kind.is_dir() {
                if let Ok(rel) = path.strip_prefix(&root) {
                    out.push(FileEntry {
                        path: rel.to_string_lossy().replace('\\', "/"),
                        is_dir: true,
                    });
                    if out.len() >= MAX_FILES {
                        out.sort_by(|a, b| a.path.cmp(&b.path));
                        return Ok(out);
                    }
                }
                stack.push(path);
            } else if kind.is_file() {
                if let Ok(rel) = path.strip_prefix(&root) {
                    out.push(FileEntry {
                        path: rel.to_string_lossy().replace('\\', "/"),
                        is_dir: false,
                    });
                    if out.len() >= MAX_FILES {
                        out.sort_by(|a, b| a.path.cmp(&b.path));
                        return Ok(out);
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn play_notification_sound() {
    let path = std::env::temp_dir().join(NOTIFICATION_SOUND_FILE);
    if std::fs::write(&path, NOTIFICATION_SOUND).is_err() {
        return;
    }
    let _ = std::process::Command::new(NOTIFICATION_PLAYER)
        .args(["-nodisp", "-autoexit", "-loglevel", "quiet"])
        .arg(path)
        .status();
}

/// Map a [`NetError`] from `create_session` into a [`CreateError`] at the
/// dispatch boundary so the pure `update` need not re-derive HTTP semantics.
///
/// Only `400 Bad Request` is treated as the empty-title rejection (the
/// server emits it when the trimmed title is empty); every other status —
/// including other 4xx — becomes [`CreateError::Other`] so the dialog shows
/// the real error instead of a misleading title hint.
fn create_error_from_net(e: NetError) -> CreateError {
    match &e {
        NetError::Status(status) if status.as_u16() == 400 => {
            CreateError::EmptyTitle(e.to_string())
        }
        _ => CreateError::Other(e.to_string()),
    }
}
