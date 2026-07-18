//! The TUI's event loop and the bits that talk to the outside world.
//!
//! The TUI is split into three pieces that work together:
//!
//! - [`model`] — the data: which screen is showing, what the user has typed,
//!   the in-flight chat turn, and so on.
//! - [`mod@update`] — a pure function that takes the model and a `Msg` (e.g. a key
//!   press, a tick, a server response) and returns the next model plus a
//!   [`Cmd`] describing any side effect to run.
//! - [`view`] — turns the model into pixels on the screen. The session
//!   transcript is the one exception: it writes its scroll bookkeeping back
//!   during rendering, because the wrapped line count is only known at draw time.
//!
//! This file holds the rest: the event loop that wires those three together,
//! the terminal guard that hands the user a working terminal on every exit
//! path, the background tasks that read keys and tick the clock, the
//! dispatcher that runs [`Cmd`]s, and the SSE consumer that turns the
//! server's stream into `Msg`s.
//!
//! Two boundaries, one rule:
//! - Errors from the user-facing world (terminal setup, runtime crashes) come
//!   back as `anyhow::Error`.
//! - Errors from talking to the server come back as the typed [`NetError`]
//!   defined in `crate::net`, never as a string.
//!
//! `unwrap` lives on neither boundary — both types force a deliberate choice.

pub mod model;
pub mod update;
pub mod view;

pub use model::{App, Cmd, Msg, Overlay, Screen, SessionState, StreamMsg};
pub use update::update;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use mewcode_protocol::StreamEvent;
use mewcode_protocol::event::ChatRequest;

use crate::config::ClientConfig;
use crate::net::{ApiClient, NetError};

use model::CreateError;

const CHANNEL_CAPACITY: usize = 256;
const TICK_INTERVAL: Duration = Duration::from_millis(50);
const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// RAII guard over the terminal: raw mode, the alternate screen, and (on
/// supporting terminals) the Kitty keyboard flags that make auto-repeat
/// visible to the input reader. [`Drop`] reverses each step, so a panic
/// or an early `?`-return always leaves a usable prompt behind.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    /// Enter raw mode + the alternate screen and build the ratatui terminal.
    fn new() -> Result<Self> {
        enable_raw_mode().context("enabling raw mode")?;
        let mut stdout = io::stdout();
        if let Err(e) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e).context("entering alternate screen");
        }
        // Two flags, no-ops on terminals that don't speak the protocol.
        // `REPORT_ALL_KEYS_AS_ESCAPE_CODES` upgrades plain text keys to
        // CSI-u so unmodified chars get distinct Press/Repeat/Release;
        // `REPORT_EVENT_TYPES` does the same for modified keys.
        let _ = execute!(
            stdout,
            EnableBracketedPaste,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        );
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(t) => t,
            Err(e) => {
                // Clean up before propagating: raw mode and alternate screen
                // are both active at this point.
                let _ = execute!(
                    io::stdout(),
                    DisableBracketedPaste,
                    PopKeyboardEnhancementFlags
                );
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                return Err(e).context("initialising terminal");
            }
        };
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    /// Best-effort restore: nothing here may panic or early-return, since we
    /// might already be unwinding. Errors are intentionally swallowed.
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Run the client TUI: bootstrap the terminal, then drive the Elm-style loop
/// until the user quits.
///
/// The loop renders the current model, awaits the next [`Msg`], applies the
/// pure [`update()`], and dispatches the resulting [`Cmd`] as async side
/// effects whose results return as more `Msg`s. The terminal is restored by
/// the guard's `Drop` on every exit path.
pub async fn run(config: ClientConfig) -> Result<()> {
    let mut guard = TerminalGuard::new()?;
    let api = ApiClient::new(config.api_url.clone());
    let mut app = App::new();

    let (tx, mut rx) = mpsc::channel::<Msg>(CHANNEL_CAPACITY);

    spawn_input_reader(tx.clone());
    spawn_ticker(tx.clone());

    loop {
        // Render the current model before blocking on the next event so the
        // user always sees a fresh frame, even after a long await.
        guard
            .terminal
            .draw(|frame| view::render(frame, &mut app))
            .context("drawing frame")?;

        // All senders dropping closes the channel; treat that as a clean exit.
        let Some(msg) = rx.recv().await else { break };

        let cmd = update(&mut app, msg);
        if matches!(cmd, Cmd::Quit) {
            break;
        }
        dispatch(cmd, &api, &tx);
    }

    Ok(())
}

/// Spawn the blocking input reader: it polls crossterm for events and forwards
/// keys and bracketed paste payloads.
fn spawn_input_reader(tx: mpsc::Sender<Msg>) {
    tokio::task::spawn_blocking(move || {
        loop {
            match event::poll(INPUT_POLL_INTERVAL) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                        if tx.blocking_send(Msg::Key(key)).is_err() {
                            break; // loop gone
                        }
                    }
                    Ok(Event::Paste(text)) => {
                        if tx.blocking_send(Msg::Paste(text)).is_err() {
                            break; // loop gone
                        }
                    }
                    Ok(_) => {} // resize, mouse, focus, repeat, release: ignored
                    Err(_) => break,
                },
                // Timed out with no event: stop if the loop has shut down.
                Ok(false) => {
                    if tx.is_closed() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

/// Spawn the ticker: a [`Msg::Tick`] every [`TICK_INTERVAL`] to drive
/// animations (spinner, toast fade) at a steady 50 ms cadence.
fn spawn_ticker(tx: mpsc::Sender<Msg>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        loop {
            interval.tick().await;
            if tx.send(Msg::Tick).await.is_err() {
                break; // loop gone
            }
        }
    });
}

/// Execute a [`Cmd`]'s side effect on a background task, sending its result
/// back into the loop as a follow-up [`Msg`]. `update` stays pure; all I/O lives here.
fn dispatch(cmd: Cmd, api: &ApiClient, tx: &mpsc::Sender<Msg>) {
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
        Cmd::FetchModels => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api.models().await.map_err(|e| e.to_string());
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
    }
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

/// Consume one SSE chat stream, forwarding each decoded [`StreamEvent`] as
/// exactly one [`Msg::Stream`] and exactly one terminal message (finished or
/// failed) per stream. The consumer never touches `App` directly —
/// that is the loop's job — keeping the ownership model clean.
async fn run_chat_stream(api: ApiClient, req: ChatRequest, tx: mpsc::Sender<Msg>) {
    use futures::StreamExt;

    let stream = match api.chat_stream(&req).await {
        Ok(stream) => stream,
        // A failed open forwards exactly one terminal failure.
        Err(e) => {
            let _ = tx.send(Msg::Stream(StreamMsg::Failed(e.to_string()))).await;
            return;
        }
    };
    futures::pin_mut!(stream);

    while let Some(frame) = stream.next().await {
        let msg = match frame {
            Err(e) => {
                let _ = tx.send(Msg::Stream(StreamMsg::Failed(e.to_string()))).await;
                break;
            }
            Ok(StreamEvent::Start { message_id, .. }) => StreamMsg::Started(message_id),
            Ok(StreamEvent::TextDelta { delta }) => StreamMsg::Delta(delta),
            Ok(StreamEvent::ToolInputAvailable {
                tool_call_id,
                tool_name,
                input,
            }) => StreamMsg::ToolInput {
                id: tool_call_id,
                name: tool_name,
                input,
            },
            Ok(StreamEvent::ToolOutputAvailable {
                tool_call_id,
                output,
            }) => StreamMsg::ToolOutput {
                id: tool_call_id,
                output,
            },
            Ok(StreamEvent::Finish { duration_ms, .. }) => {
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Finished { duration_ms }))
                    .await;
                break;
            }
            Ok(StreamEvent::Aborted) => {
                let _ = tx
                    .send(Msg::Stream(StreamMsg::Failed("aborted".into())))
                    .await;
                break;
            }
            Ok(StreamEvent::Error { message }) => {
                let _ = tx.send(Msg::Stream(StreamMsg::Failed(message))).await;
                break;
            }
        };
        if tx.send(Msg::Stream(msg)).await.is_err() {
            break; // loop gone
        }
    }
}
