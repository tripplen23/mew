# mewcode — implementation phases

The project is pivoting: phases 1-13 are the "v0 coding agent" foundation,
and the next phase is the **mewdraw** architecture canvas
(`docs/architecture-canvas/`). The new work is sequenced as M1-M5
milestones; this file tracks both tracks until the mewdraw milestones
absorb or supersede the remaining TUI/config work.

| #  | Phase | Status |
|----|-------|--------|
| 1  | Workspace skeleton (4 crates, 2 binaries, wire protocol) | ✅ |
| 2  | Anthropic-aligned tools + Skills skeleton | ✅ |
| 3  | `server` skeleton (axum + figment) | ✅ |
| 4  | Persistence (filesystem): `SessionStore` trait, `FsStore` + `MemoryStore`, XDG layout | ✅ |
| 5  | `client` shell (ratatui event loop, home screen) | ✅ |
| 6  | New session flow (title / mode / model pickers) | ✅ |
| 7  | Engine v0 (rig Anthropic-compat client, e2e smoke) | ✅ |
| 8  | Conversation history + session resume (`HistoryStrategy::Raw`) | ✅ |
| 9  | Durable memory scaffold (fact store, `# Memory` preamble, `mewcode_memory` tool) | ✅ |
| 10 | Streaming (rig → SSE → TUI live tokens) | ✅ |
| 11 | Tool-calling loop (`RigToolAdapter`, `MAX_AGENT_TURNS=10`, `agent_tool_e2e.rs`) | ✅ |
| 12 | Remaining tools + PLAN mode gate + Anthropic prompt caching | ✅ |
| 13 | Skills runtime (2-tool progressive disclosure + external dirs) | ✅ |
| 14 | TUI polish (markdown, code blocks, tool cards, theme, slash menu, @-mention) | 📦 absorbed into M1 |
| 15 | Config & persistence (`~/.config/mewcode/config.toml`, recent sessions) | 📦 partially absorbed (M1 needs theme loading) |
| 16 | Hardening (error toasts, Ctrl-C graceful shutdown, retries, command palette) | 📦 partially absorbed (M1 needs toast + Ctrl-C *and* panic-recovery) |
| 17 | Trace ingestion latency | ⬜ (active) |

## Mewdraw milestones

The full design is in `docs/architecture-canvas/`:

- `README.md` — thesis, settled decisions, data model, crate map, risks
- `milestone-1-promptable-canvas.md` — M1 tasks T1-T7 with acceptance criteria
- `ui-aesthetic.md` — visual target (draw.io × Warp in a terminal)
- `hermes-loop-prompt.md` — copy-pasteable build prompt for the closed loop

| M  | Name              | Outcome                                                                                          | Status |
|----|-------------------|--------------------------------------------------------------------------------------------------|--------|
| M1 | Promptable canvas | Render `graph.json` as boxes + edges; mouse select/pan; prompt bar where the agent mutates the graph and the canvas redraws live. **No codegen, no drift detection in M1.** | ⬜ (active on `wtf-bby-im-lit`; T1 = PR #13) |
| M2 | Manual editing    | Mouse create/connect/rename nodes; `layout.json` drag-to-move; undo/redo.                        | ⬜      |
| M3 | Forward codegen   | `canvas_gen_code`: graph → crate/module/trait/struct skeleton (contracts, not bodies).            | ⬜      |
| M4 | Drift detection   | `canvas_check_drift`: parse bound symbols with `syn`, diff signatures vs `contract`, report divergence read-only. | ⬜      |
| M5 | Code → graph sync | On commit, re-extract structure and update/annotate affected nodes; propose graph deltas for human approval. | ⬜      |

## Pre-M1 infrastructure (still needed)

Some phase 14-16 work is load-bearing for M1 even though the rest is
absorbed. Resolve these **before M1 lands**, not as part of M1's
acceptance:

- **Theme loading from `config.toml`** (phase 15 subset). M1's T4a
  reads theme slots from config; the config file does not yet exist.
  Either ship a minimal `~/.config/mewcode/config.toml` reader in M1's
  T4a, or hardcode a single default theme in M1 and defer config to
  M2. **Recommend:** hardcode in M1, defer config to a small follow-up
  PR after M1 lands.
- **Toast / status surface** (phase 16 subset). M1's T7 needs to show
  "mew added Authenticator" in the existing toast area. The toast
  already exists in the runtime model (`crates/client/src/runtime/model/states/mod.rs`),
  but T7 needs to wire `StreamMsg::ToolOutput` to toast emission.
  Either fix in M1's T7 or land a small "toast helper" PR first.
- **Graceful shutdown on Ctrl-C** (phase 16 subset). M1's T3 enables
  mouse capture; if the TUI panics during a mouse event, the terminal
  must restore cleanly. The existing `TerminalGuard::Drop` covers the
  happy path; verify it covers the panic path in T3's acceptance.

## Phase 14 — TUI polish (absorbed into M1+M2)

Subsumed by mewdraw milestones. Component breakdown:

- Markdown rendering (`tui-markdown`) → re-scoped to M2 (block strip in M1 doesn't need full markdown).
- Code blocks with `syntect` → re-scoped to M2.
- Tool cards → re-scoped to M2.
- Theme switcher → re-scoped to M2 (M1 ships one default theme per `ui-aesthetic.md` §6).
- Slash command menu → out of scope (M2+).
- @-mention popover → out of scope (M2+).
- Toast, trace pane, animations → partially in M1 (toast), partially in M2 (trace pane, animations).

## Phase 15 — Config & persistence (partial)

- `~/.config/mewcode/config.toml` → split: minimal config (theme + last-used model) in M1's T4a; full config (recent sessions, default model per project) in M2.
- Last-used model, theme, recent sessions → last-used model in M1; theme in M1; recent sessions in M2.

## Phase 16 — Hardening (partial)

- Error toasts → partially in M1 (toast from `canvas_mutate` failures), fully in M2.
- Ctrl-C graceful shutdown → in M1's T3 acceptance (signal handler drains in-flight requests, then exits).
- Panic recovery (terminal restore) → in M1's T3 acceptance (verify `TerminalGuard::Drop` covers panic from mouse-event handlers).
- Retries → out of scope (M2+).
- Command palette → out of scope (M2+, per `ui-aesthetic.md` §6).

## Phase 17 — Trace ingestion latency

Traces take ~13 min to appear in Langfuse. Three confirmed root causes
(verified against `opentelemetry_sdk-0.31.0` / `opentelemetry-langfuse-0.6.1`
source, plus [Langfuse v4 FAQ][langfuse-v4]):

1. **Missing `x-langfuse-ingestion-version: 4` header.** Langfuse's
   Fast Preview path needs this; without it traces land in the S3
   batched path which the FAQ itself documents as "multi-minute
   delays". The langfuse crate's `exporter.rs:185-199` only injects
   `Authorization`, not this header.
2. **Unconfigured `BatchConfig` defaults.** `main.rs:116` uses
   defaults (5s tick, 30s export timeout, batch 512, queue 2048).
3. **No graceful shutdown + no per-turn `force_flush`.** Ctrl-C drops
   in-flight spans; the 5s ticker is the only flush driver.

Fix shape:
- Set the v4 header via `OTEL_EXPORTER_OTLP_HEADERS` (langfuse builder
  doesn't expose header injection).
- Tune `BatchConfigBuilder`: `scheduled_delay=2s`, `export_timeout=10s`,
  `batch=256`, `queue=4096`.
- Wrap `axum::serve` in `with_graceful_shutdown(tokio::signal::ctrl_c())`
  so `provider.shutdown()` is actually reached.
- `force_flush()` at end of `Harness::run_turn` and the chat forwarder.

E2E: extend `crates/server/tests/agent_tool_e2e.rs` to assert trace
returns from a Langfuse API query in <5s.

[langfuse-v4]: https://langfuse.com/faq/all/explore-observations-in-v4

## Open strategic questions (tracked in #14)

Four strategic questions came up during the PR #12 review. Their
milestone impact:

- **Q1 (first-run UX):** when a user has code but no `graph.json`,
  where does the graph come from? Affects M1 (T1 renders an empty
  graph; first-run needs either an init command or a "code → graph"
  bootstrap before the M1 UX feels useful) and M5.
- **Q2 (drift policy):** when M4 finds divergence between contract
  and code, what happens? Affects M3+M4.
- **Q3 (layout-crate spike):** `ascii-dag` vs `layout-rs` needs a
  real evaluation PR before T2. Blocks M1 (T2 depends on the
  choice).
- **Q4 (Plan mode asymmetry):** `canvas_mutate` is allowed in Plan
  mode; `edit_file` isn't. Document the policy. Blocks M1 (T6 must
  enforce this).

## Key entry points

| Concern | Location |
|---------|----------|
| Wire protocol | `crates/protocol/src/` |
| Canvas data types (M1/T1) | `crates/protocol/src/canvas.rs` |
| Harness | `crates/engine/src/harness/` |
| System prompt | `crates/engine/src/agent/prompt.rs` |
| Trace helpers | `crates/engine/src/harness/trace.rs` |
| Tool adapter (rig ↔ mewcode) | `crates/engine/src/tools/adapter.rs` |
| Tool registry / mode gate | `crates/engine/src/tools/mod.rs` |
| Memory store | `crates/engine/src/memory.rs` |
| OTel/Langfuse init | `crates/server/src/main.rs:73-120` |
| `/chat` SSE | `crates/server/src/routes/chat.rs` |
| E2E (real LLM + Langfuse) | `crates/server/tests/agent_tool_e2e.rs` |
| Canvas design | `docs/architecture-canvas/` |
