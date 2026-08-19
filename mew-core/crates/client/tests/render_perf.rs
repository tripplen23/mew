//! Transcript render cost measurements.
//!
//! Diagnostic harness for the "scrolling a long session isn't smooth"
//! problem. Run with:
//!
//! ```text
//! cargo test -p mewcode-client --test render_perf --release -- --nocapture
//! ```
//!
//! The timing tests print numbers and assert nothing about wall-clock (that
//! would be flaky on shared CI); they exist to answer "where does a frame's
//! time actually go" before committing to a virtualization refactor.
//!
//! `wrapped_line_counts_are_additive` is a real assertion, not a
//! measurement: it pins the load-bearing property that per-item cached line
//! counts can be summed to derive `max_scroll`. Virtualization is only
//! correct if that holds.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{App, Msg, Screen, SessionState};
use mewcode_client::runtime::update;
use mewcode_client::runtime::view::render;
use mewcode_protocol::{Message, MessagePart, Mode, ModelId};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

const VIEWPORT_W: u16 = 100;
const VIEWPORT_H: u16 = 40;

/// A realistic-ish assistant reply: prose plus a fenced code block, so the
/// markdown + syntect path is exercised the way a real session does.
fn assistant_body(i: usize) -> String {
    format!(
        "Here is reply number {i} with some **bold** prose and a list:\n\
         \n\
         - first point about the change\n\
         - second point with `inline_code`\n\
         \n\
         ```rust\n\
         fn handler_{i}(input: &str) -> Result<usize, Error> {{\n\
             let parsed = input.trim().parse::<usize>()?;\n\
             Ok(parsed.saturating_add({i}))\n\
         }}\n\
         ```\n\
         \n\
         And a closing paragraph that is long enough to wrap across more than \
         one terminal row at typical widths.\n"
    )
}

fn session_with(message_count: usize) -> Session {
    let messages = (0..message_count)
        .map(|i| {
            if i % 2 == 0 {
                Message::user(vec![MessagePart::Text {
                    text: format!("user question {i} that is reasonably long and wraps a bit"),
                }])
            } else {
                Message::assistant(
                    vec![MessagePart::Text {
                        text: assistant_body(i),
                    }],
                    "test-model",
                )
            }
        })
        .collect();
    Session {
        id: Uuid::new_v4(),
        title: "perf".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages,
        compaction_summary: None,
        compacted_up_to: None,
        todos: vec![],
    }
}

fn app_with(message_count: usize) -> App {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session_with(message_count)));
    app
}

fn terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(VIEWPORT_W, VIEWPORT_H)).expect("test backend")
}

fn draw_once(terminal: &mut Terminal<TestBackend>, app: &mut App) {
    terminal.draw(|frame| render(frame, app)).expect("draw");
}

fn press(app: &mut App, code: KeyCode) {
    update(app, Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

/// Steady-state per-frame cost while scrolling: cache is warm (all committed
/// messages already rendered once), only the scroll offset changes. This is
/// the number that matters for "holding PageUp feels laggy".
#[test]
fn measure_warm_frame_cost_while_scrolling() {
    println!("\n=== warm frame cost while scrolling (cache hits, scroll changes) ===");
    for &count in &[40usize, 200, 800] {
        let mut app = app_with(count);
        let mut term = terminal();

        // Warm the transcript cache and let follow settle.
        draw_once(&mut term, &mut app);
        draw_once(&mut term, &mut app);

        let mut samples = Vec::new();
        for i in 0..40 {
            // Alternate direction so we stay in the middle of the transcript
            // and never sit clamped at an edge.
            press(
                &mut app,
                if i % 2 == 0 {
                    KeyCode::PageUp
                } else {
                    KeyCode::PageDown
                },
            );
            let start = Instant::now();
            draw_once(&mut term, &mut app);
            samples.push(start.elapsed());
        }
        let med = median(samples);
        println!(
            "  {count:>4} messages: median frame {:>9.3?}  -> max {:>5.0} frames/sec",
            med,
            1.0 / med.as_secs_f64()
        );
    }
}

/// Cold vs warm: how much of a frame the cache already removed. A large gap
/// means the cache is doing its job; the warm number is what's left to fix.
#[test]
fn measure_cold_versus_warm_frame() {
    println!("\n=== cold (first) frame vs warm frame ===");
    for &count in &[40usize, 200, 800] {
        let mut app = app_with(count);
        let mut term = terminal();

        let start = Instant::now();
        draw_once(&mut term, &mut app);
        let cold = start.elapsed();

        let mut samples = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            draw_once(&mut term, &mut app);
            samples.push(start.elapsed());
        }
        let warm = median(samples);
        println!(
            "  {count:>4} messages: cold {:>9.3?}   warm {:>9.3?}   cache saves {:>5.1}x",
            cold,
            warm,
            cold.as_secs_f64() / warm.as_secs_f64().max(f64::EPSILON)
        );
    }
}

/// Report the real wrapped height of each test transcript, so the synthetic
/// line-count benchmarks above can be related to actual sessions.
/// `max_scroll + viewport` is exactly the total wrapped line count.
#[test]
fn measure_real_transcript_wrapped_height() {
    println!("\n=== real wrapped height of the test transcripts ===");
    for &count in &[40usize, 200, 800] {
        let mut app = app_with(count);
        let mut term = terminal();
        draw_once(&mut term, &mut app);
        let Screen::Session(s) = &app.screen;
        println!(
            "  {count:>4} messages -> {:>6} wrapped lines (viewport shows {})",
            s.max_scroll as usize + s.viewport as usize,
            s.viewport,
        );
    }
}

fn rss_kb() -> u64 {
    // /proc/self/status is Linux-only and the VmRSS value is best-effort;
    // degrade to 0 instead of panicking the perf harness on other platforms.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|contents| {
            contents
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        })
        .unwrap_or(0)
}

/// Does multi-width churn actually cost memory? Each width change re-renders
/// every message (cache miss). The cache is keyed by message id, so a width
/// change replaces the old-width block in place rather than accumulating it;
/// the RSS delta shows whether the re-render churn itself is measurable.
const MESSAGES: usize = 800;

#[test]
fn measure_cache_growth_across_widths() {
    println!("\n=== cache entries + RSS as widths accumulate ({MESSAGES} messages) ===");
    let mut app = app_with(MESSAGES);
    let mut term = terminal();
    draw_once(&mut term, &mut app);
    let mut base_rss = rss_kb();
    for &w in &[100u16, 90, 80, 70, 60, 50] {
        term.backend_mut().resize(w, VIEWPORT_H);
        draw_once(&mut term, &mut app);
        let Screen::Session(s) = &app.screen;
        let entries = s.transcript_cache.message_cache_len();
        assert_eq!(
            entries, MESSAGES,
            "cache must hold exactly one entry per message across width changes"
        );
        let rss = rss_kb();
        println!(
            "  width {:>3}: {:>4} cache entries / {MESSAGES} messages, RSS {:>6} kB  (+{} kB)",
            w,
            entries,
            rss,
            rss.saturating_sub(base_rss)
        );
        base_rss = base_rss.max(rss);
    }
}

/// Load-bearing property for virtualization: ratatui wraps each `Line`
/// independently, so the wrapped height of a transcript equals the sum of
/// the wrapped heights of its parts. If this ever stops holding, summing
/// per-message cached line counts to derive `max_scroll` becomes wrong and
/// scrolling would drift.
#[test]
fn wrapped_line_counts_are_additive() {
    let width = 30;
    let chunk_a: Vec<Line> = vec![
        Line::from("short"),
        Line::from("a much longer line that definitely needs to wrap several times at width 30"),
    ];
    let chunk_b: Vec<Line> = vec![
        Line::from("another line here that also wraps because it is long"),
        Line::from(""),
        Line::from("tail"),
    ];

    let count = |lines: Vec<Line>| {
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .line_count(width)
    };

    let a = count(chunk_a.clone());
    let b = count(chunk_b.clone());
    let combined = count([chunk_a, chunk_b].concat());

    assert_eq!(
        combined,
        a + b,
        "per-Line wrapping must be additive for per-item cached line counts to sum correctly"
    );
}
