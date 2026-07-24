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
//! - Errors from talking to the server come back as the typed `NetError`
//!   defined in `crate::net`, never as a string.
//!
//! `unwrap` lives on neither boundary — both types force a deliberate choice.

pub mod model;
pub mod update;
pub mod view;

mod dispatch;
mod stream;

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

use crate::config::ClientConfig;
use crate::net::ApiClient;

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
        dispatch::dispatch(cmd, &api, &tx);
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
