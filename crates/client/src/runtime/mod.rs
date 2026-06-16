//! TUI runtime: the Elm-style model, messages, and the event loop.
//!
//! The model lives in [`model`], the pure state transition in [`mod@update`],
//! and the rendering in [`view`]. This module owns the imperative shell: a
//! RAII terminal guard, the crossterm-backed event loop, the input/ticker
//! tasks, the [`Cmd`] dispatcher, and the SSE consumer.
//!
//! > Idiom: RAII terminal guard. Raw mode and the alternate screen are entered
//! > in `TerminalGuard::new`; its `Drop` restores cooked mode and leaves the
//! > alternate screen. So a normal exit, an error `?`-return, or an unwinding
//! > panic all hand the user back a working terminal — there is no cleanup path
//! > to forget. `unwrap` stays off the production path: `anyhow` sits at this
//! > boundary, `thiserror`/`NetError` at the `net` boundary.

pub mod model;
pub mod update;
pub mod view;

pub use model::{
    App, Cmd, HomeState, Msg, NewSessionField, NewSessionState, Overlay, Screen, SessionState,
    StreamMsg,
};
pub use update::update;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
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
use crate::net::ApiClient;

const CHANNEL_CAPACITY: usize = 256;
const TICK_INTERVAL: Duration = Duration::from_millis(50);
const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// RAII guard owning the terminal in raw mode + alternate screen.
///
/// Construction enters raw mode and the alternate screen; [`Drop`] restores
/// both. Because restoration happens in `Drop`, an early `?`-return or an
/// unwinding panic still leaves the user's terminal usable.
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
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(t) => t,
            Err(e) => {
                // Clean up before propagating: raw mode and alternate screen
                // are both active at this point.
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                return Err(e).context("initialising terminal");
            }
        };
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort restore: nothing here may panic or early-return, since we
        // might already be unwinding. Errors are intentionally swallowed.
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
    dispatch(Cmd::LoadSessions, &api, &tx); // initial Home fetch

    loop {
        // Render the current model before blocking on the next event so the
        // user always sees a fresh frame, even after a long await.
        guard
            .terminal
            .draw(|frame| view::render(frame, &app))
            .context("drawing frame")?;

        // All senders dropping closes the channel; treat that as a clean exit.
        let Some(msg) = rx.recv().await else { break };

        let cmd = update(&mut app, msg);
        if app.should_quit {
            break;
        }
        dispatch(cmd, &api, &tx);
    }

    Ok(())
    // `guard` drops here → terminal restored.
}

/// Spawn the blocking input reader: it polls crossterm for events and forwards
/// each key press as a [`Msg::Key`]. Key *release* events are dropped so an
/// action never fires twice on platforms that report both.
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
                    Ok(_) => {} // resize, mouse, focus, paste, key-release: ignored
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
        Cmd::None => {}
        Cmd::LoadSessions => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api.list_sessions().await.map_err(|e| e.to_string());
                let _ = tx.send(Msg::SessionsLoaded(result)).await;
            });
        }
        Cmd::CreateSession(req) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = api.create_session(&req).await.map_err(|e| e.to_string());
                let _ = tx.send(Msg::SessionCreated(result)).await;
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
        Cmd::StartChat(req) => {
            let api = api.clone();
            let tx = tx.clone();
            tokio::spawn(run_chat_stream(api, req, tx));
        }
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
