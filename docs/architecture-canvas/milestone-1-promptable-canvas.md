# Milestone 1 — Promptable Architecture Canvas

> Turn the current ratatui TUI into a promptable terminal architecture-design
> surface. Render a graph as boxes + edges, let the user pan/select with mouse
> and keyboard, and add a prompt bar where the agent mutates the graph and the
> canvas redraws live.
>
> **Scope guard:** NO code generation, NO drift detection, NO manual node
> creation-by-mouse in this milestone. Just: render a graph, navigate it, and
> let the agent edit it by prompt. That isolates the canvas + agent-mutation
> loop before any MDA machinery exists.

Read `README.md` in this folder first for the data model (§5), the settled
decisions (§3), and the crate map (§6). Read `ui-aesthetic.md` for the visual
target (draw.io × Warp) that T4/T7 render into.

---

## 1. What "done" looks like (demo script)

1. Run the existing `cargo run -- tui` and open the canvas from a key
   (e.g. a binding on the Home/Session screen). No new subcommand.
2. The canvas shows boxes for each node in `graph.json`, connected by routed
   edges, auto-laid-out. An empty graph shows an empty canvas with a hint.
3. Mouse: click a node to select it (highlight); scroll/drag to pan; arrow keys
   move selection as a keyboard fallback.
4. Type in the prompt bar: "add an auth component that depends on the session
   store." The agent calls `canvas_mutate`, `graph.json` changes, and the
   canvas **redraws with the new node and edge** without a restart.
5. Quit restores the terminal cleanly (existing `TerminalGuard` already does
   this on every exit path).

If that loop feels alive — you prompt, the architecture redraws — milestone 1
succeeded and the whole product thesis is de-risked.

## 2. Architecture within the existing TEA loop

The client is an Elm-style loop: `model` → `update(&mut App, Msg) -> Cmd` →
`view`, with `Cmd`s executed off the loop and results fed back as `Msg`s
(`crates/client/src/runtime/mod.rs`). We extend it, we don't replace it.

**Key facts about the current loop (verified in source):**

- `update` is **pure, no I/O, never `.await`s**. All side effects go through
  `Cmd` and `dispatch`. The canvas must obey this — file reads/writes and agent
  calls are `Cmd`s.
- The input reader currently **drops mouse events**: in
  `runtime/mod.rs::spawn_input_reader`, the match arm
  `Ok(_) => {} // resize, mouse, focus, paste, key-release: ignored`. Enabling
  the canvas starts with capturing them.
- A `Msg::Tick` fires every 50 ms (`TICK_INTERVAL`) and already drives
  animations — reuse it for any canvas animation/auto-layout settling.
- `Screen` is an enum (`Home`, `NewSession`, `Session`). The canvas is a new
  variant.

## 3. Tasks

Each task is independently reviewable. Acceptance criteria are written so
another agent can verify with `cargo test` / `cargo build` plus the noted
manual check. Order matters; later tasks depend on earlier ones.

### T1 — Graph data types in `protocol`

Add serde types to `crates/protocol/src/` (new module `canvas.rs`, re-exported
from the crate root): `Graph`, `Node`, `Edge`, `NodeKind` (`System` /
`Container` / `Component`), `EdgeKind` (`Depends` / `Calls` / `Implements` /
`Owns`), and `Layout` (`positions: HashMap<NodeId, Point>`, `theme: String`).
`NodeId` is a newtype over `String`.

- Match the JSON shapes in `README.md` §5 exactly.
- Pure data only — no I/O (respects the crate's existing rule).
- **Acceptance:** unit test round-trips the §5 example `graph.json` and
  `layout.json` through serde (`serde_json::to_string` → `from_str` →
  `assert_eq!`). `cargo test -p mewcode-protocol` green.

### T2 — Graph load/save + auto-layout in a new `engine` module

New module `crates/engine/src/canvas/` with:
- `load(project_root) -> Result<(Graph, Layout)>` reading
  `.mewcode/canvas/{graph,layout}.json`; missing files → empty graph + empty
  layout (do not error).
- `save_graph` / `save_layout` writing them back (pretty JSON).
- `auto_layout(&Graph, &Layout) -> ResolvedLayout` that fills positions for
  nodes missing from `layout.json`, using a layout crate.

**Layout crate spike (do this first, timeboxed):** try
[`ascii-dag`](https://lib.rs/crates/ascii-dag) for grid placement + edge
routing. If edge routing in a char grid is unusable, fall back to
[`layout-rs`](https://lib.rs/crates/layout-rs) or
[`rust-sugiyama`](https://lib.rs/crates/rust-sugiyama). Record the choice and
why in a `// ponytail:` comment naming the ceiling.

- **Acceptance:** unit tests: (a) loading a non-existent project yields an empty
  graph; (b) a 3-node/2-edge graph round-trips through save→load; (c)
  `auto_layout` assigns a distinct position to every node and is deterministic
  for a fixed input (deterministic order = sort by `NodeId` before placement;
  ties broken by edge `(src, tgt)` lex order). `cargo test -p mewcode-engine` green.

### T3 — Mouse capture in the client event loop

In `crates/client/src/runtime/mod.rs`:
- Enable mouse capture: `execute!(stdout, EnableMouseCapture)` in
  `TerminalGuard::new` and `DisableMouseCapture` in `Drop` (alongside the
  existing alternate-screen setup/teardown).
- In `spawn_input_reader`, forward `Event::Mouse(m)` as a new `Msg::Mouse(m)`
  instead of dropping it. Keep dropping resize/focus/paste for now.
- Add `Msg::Mouse(crossterm::event::MouseEvent)` to the `Msg` enum
  (`runtime/model/msg.rs`).

- **Acceptance:** `cargo build -p mewcode-client` green; manual check: a
  temporary `eprintln!`/log shows mouse coordinates on click (remove before
  merge). Existing screens must ignore `Msg::Mouse` (no behaviour change) —
  verified by existing client tests still passing.

### T4 — `Screen::Canvas` state + read-only render

> Visual target for this task lives in `ui-aesthetic.md` (draw.io × Warp).
> Render the polished version from the start: rounded node cards, C4 color-coded
> title bars, a dotted-grid background, arrowhead connectors, and the
> three-pane + bottom-block shell. The heavy flourish (palette overlay, minimap,
> gradients, animations) is explicitly deferred to M2 per that doc §6.

#### T4a — Theme foundation (do first, within T4)

- Add a `Theme` struct with semantic color slots (see `ui-aesthetic.md` §4) and
  detect terminal color depth once at startup (truecolor → 256 → 16), resolving
  each slot to the best available representation.
- Every view reads colors from `Theme`; no hardcoded colors. Ship one good
  default theme (theme switching is M2).
- **Acceptance:** unit test that slot resolution degrades correctly for each
  color depth; existing screens still render (can adopt `Theme` lazily).

- Add `Screen::Canvas(CanvasState)` to the `Screen` enum and a `CanvasState`
  model: `graph: Graph`, `layout: Layout`, `selected: Option<NodeId>`,
  `viewport: ViewportOffset`, `prompt: <textarea>`, `status: Option<Toast>`.
- New `view/canvas.rs`: render each node as a rounded `ratatui` card
  (`BorderType::Rounded`) with a C4-colored title bar at its resolved position
  offset by the viewport; draw edges as routed lines with arrowheads from the
  layout engine; paint the dotted grid background. Selected node gets the
  `accent`/`edge_selected` border. Lay out the left palette rail, right
  inspector, and bottom block strip per `ui-aesthetic.md` §3, collapsing side
  panes below a width breakpoint.
- New `Cmd::LoadCanvas(project_root)` + `Msg::CanvasLoaded(Result<(Graph,
  Layout), String>)`, dispatched through the existing `dispatch` fn calling the
  T2 `engine::canvas::load`. (Mirror the `LoadSessions` pattern exactly.)
- Entry point: a key binding on an existing screen (Home or Session) that
  pushes `Screen::Canvas` and fires `Cmd::LoadCanvas`. No new CLI subcommand —
  the canvas is reachable from within the existing `tui`.

- **Acceptance:** with a hand-written 3-node `graph.json`, launching the canvas
  renders 3 boxes and their edges. `view` logic that's pure (position math,
  edge endpoints) has unit tests. Manual: boxes appear, no panic on empty graph.

### T5 — Navigation (mouse + keyboard)

In `update/canvas.rs` (new), handle `Msg::Mouse` and `Msg::Key` for
`Screen::Canvas`:
- Click inside a node's rect → select it (hit-test: point-in-rect against
  resolved positions + box sizes, accounting for viewport offset).
- Drag on empty canvas → pan (update `viewport`).
- Scroll wheel → pan vertically (and horizontally with shift, if available).
- Keyboard fallback: arrow keys move selection to nearest node in that
  direction; `Esc` leaves the canvas.
- All in pure `update`; no I/O.

- **Acceptance:** unit tests for hit-testing (point inside/outside a node rect
  with a viewport offset) and for selection-by-arrow direction. Manual: click
  selects, drag pans, arrows move selection.

### T6 — `canvas_mutate` + `canvas_read` engine tools

New tools in `crates/engine/src/tools/canvas/`, registered in
`default_registry` (follow the 3-step "Adding a new tool" recipe in
`tools/mod.rs`):
- `canvas_read` (read-only, always available): returns the current `Graph` as
  JSON so the agent can reason about existing structure.
- `canvas_mutate` (Build-mode write tool, gated like `edit_file`): applies a
  structured delta — `add_node`, `remove_node`, `rename_node`, `add_edge`,
  `remove_edge`, `set_contract`, `set_bind`. Validates: node ids unique on add,
  edges reference existing nodes, removing a node cascades its edges. Persists
  via T2 `save_graph`.
- Design edits should be allowed in **Plan** mode (designing isn't mutating
  code). Decision: register `canvas_mutate` in Plan mode too, since it only
  writes `graph.json`, not source. Note this divergence from the file-write
  tools in a comment.

- **Acceptance:** `cargo test -p mewcode-engine` covers: add_node then
  add_edge; remove_node cascades edges; duplicate id rejected; edge to unknown
  node rejected. Each returns a typed `ToolError`, never a panic. Extend the
  existing tool e2e if cheap.

### T7 — Live redraw: agent mutation → canvas refresh

Wire the prompt bar to the agent and refresh the canvas when the graph changes:
- Prompt submit in `Screen::Canvas` → `Cmd::StartChat` (reuse the existing chat
  stream machinery) with the canvas project context.
- When the stream reports a `canvas_mutate` tool call completed (observe
  `StreamMsg::ToolOutput` for the `canvas_mutate` tool id/name), fire
  `Cmd::LoadCanvas` to reload `graph.json` and re-run auto-layout, preserving
  existing `layout.json` positions.
- Render the agent exchange as Warp-style **blocks** in the bottom strip
  (left accent bar, role label, status chip) driven by the `StreamMsg`
  lifecycle — see `ui-aesthetic.md` §5.
- Show tool activity in the existing toast/status area so the user sees "mew
  added Authenticator."

- **Acceptance:** manual demo (the §1 script step 4) works end-to-end against a
  real model: prompt → agent calls `canvas_mutate` → canvas shows the new node
  and edge without restart. Add an engine-level e2e if the existing
  `agent_tool_e2e.rs` harness makes it cheap (assert the tool round-trips and
  `graph.json` reflects the change).

## 4. Stretch (only if M1 lands early)

- Animate new nodes (fade/slide in) on the 50 ms tick.
- Mascot reaction in a corner when the agent mutates the graph (ties back to
  the earlier `StreamMsg`-driven mascot idea).
- Mermaid export of the current graph (`canvas_export_mermaid`) — cheap, and
  validates the "graph is truth, Mermaid is a projection" model early.

## 5. Explicit ceilings for this milestone (`ponytail:`)

- Auto-layout only; no drag-to-reposition yet (that's M2). Positions the agent
  or auto-layout choose are fine.
- Edge routing is "good enough"; crossings are acceptable. Upgrade path is a
  better layout crate or a routing pass in M2.
- Mouse is an enhancement; every action has a keyboard equivalent so the canvas
  works in terminals without mouse capture.
- No persistence of viewport/selection across sessions.

## 6. Verification checklist before calling M1 done

- [ ] `cargo build` (workspace) green.
- [ ] `cargo test` (workspace) green; new unit tests for T1, T2, T4, T5, T6.
- [ ] Manual demo script (§1) passes against a real model.
- [ ] Mouse capture is disabled on every exit path (no leaked raw mode / mouse
      mode after quit or panic — the `TerminalGuard` `Drop` covers this; verify
      `DisableMouseCapture` is in it).
- [ ] Empty graph and 1-node graph both render without panic.
- [ ] Existing Home/NewSession/Session screens are unchanged (regression check).
