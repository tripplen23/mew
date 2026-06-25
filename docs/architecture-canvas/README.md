# Architecture Canvas — Design Overview

> Working codename: **mewdraw**. A promptable, terminal-native architecture
> design surface where a human and the agent (`mew`) co-design an app's
> architecture as a graph, then generate and keep code structurally in sync
> with that graph.

**Status:** design / not started. This folder is the source of truth for the
build. Implement milestone-by-milestone; each milestone doc lists tasks with
acceptance criteria suitable for handing to another coding agent.

---

## 1. The one-sentence thesis

> A semantic **graph is the source of truth**; Mermaid, the TUI canvas, and
> code are all **projections** of it; the **agent is the reconciler** that
> keeps the structure of the code honest against the graph.

Every design decision below falls out of that sentence. If a proposed feature
breaks "graph is truth, agent reconciles structure," it does not belong in v1.

## 2. Why this is worth building (and why now)

Model-driven development (UML round-tripping, MDA) has been tried since the
early 2000s and mostly failed. The failure modes are well documented: rigid
generated scaffolds developers couldn't touch, models that drifted from code
the moment either side was edited, and diagrams too lossy to map back to real
symbols ([Quora: why MDA/MDD failed](https://www.quora.com/Why-did-model-driven-architecture-development-fail)).
Every one of those was a *manual reconciliation* problem.

The thing that changed: an agent can now do the reconciliation. Recent writing
on AI-generated code drift lands on the same prescription we arrived at
independently — keep the plan/model as the source of truth and regenerate from
it rather than hand-patching output, so drift always has a reset point
([MindStudio, 2025](https://www.mindstudio.ai/blog/ai-generated-code-drift-cost-analysis/)).
The emerging "architectural drift" problem — code that compiles and passes
tests but quietly violates the intended design — is exactly what a
graph-bound-to-symbols guardrail catches
([Toward Next AI, 2025](https://medium.com/toward-next-ai/ai-coding-agent-architecture-guardrails-how-to-stop-agents-from-passing-tests-while-breaking-7c66927cb6a3)).
*Content rephrased for compliance with licensing restrictions.*

The wedge versus end-to-end code generators: our artifact is a **readable,
editable, performable** graph the human steers, not a black box.

## 3. Settled decisions (the forks)

These were decided during brainstorming and are now fixed for v1. Changing one
is a design change, not an implementation detail.

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| 1 | Source of truth | **Semantic graph model**, not Mermaid | Nodes must carry `bind` + `contract`; Mermaid's grammar can't hold that without abuse. Mermaid becomes an *export*. |
| 1b | Geometry | **Separate layout overlay**, optional | A box nudge is a presentation event, not an architecture change. Keeps semantic diffs clean and keeps the agent from seeing layout churn as design churn. |
| 2 | Sync scope | **Structure-only round-trip** | Modules, types, signatures, dependencies. *Not* semantic intent or behaviour — promising that rebuilds the graveyard. |
| 3 | Beachhead | **General app architecture** | C4-style components and dependencies (see §5). |
| 4 | First milestone | **Promptable canvas** (this is the hard UI) | See `milestone-1-promptable-canvas.md`. |

Non-goals for v1, stated explicitly:

- No generation of function **bodies** from the graph. The graph owns
  **contracts** (boundaries, signatures, wiring); bodies are human/agent code.
  Generating bodies from diagrams is failure mode #1 of MDA — do not.
- No semantic/behavioural round-trip. Structure only.
- No auto-deletion of code. Design→code deletions are *proposed diffs* a human
  approves.

## 4. Prior art & what to reuse (lazy-senior: don't reinvent)

Research notes with what we take from each. Full sources at the bottom.

**Graph layout — do NOT write a layout algorithm.** It's a solved, hard problem
and there are Rust crates:

- [`ascii-dag`](https://lib.rs/crates/ascii-dag) — places nodes and routes
  edges in a **fixed-width grid**. Terminal-native; the closest match to what
  we render. *Primary candidate.*
- [`layout-rs`](https://lib.rs/crates/layout-rs) — Graphviz-style layout, mature.
- [`rust-sugiyama`](https://lib.rs/crates/rust-sugiyama) /
  [`dagre`](https://lib.rs/crates/dagre) — layered (Sugiyama) layout over
  `petgraph`, good for dependency DAGs.
- Decision deferred to milestone 1 spike: evaluate `ascii-dag` first; fall back
  to `layout-rs` if edge routing in a character grid is weak.

**Terminal mouse + canvas — proven, not built-in to ratatui:**

- Ratatui has no built-in hit-testing; mouse capture is enabled through the
  crossterm backend ([ratatui mouse capture](https://ratatui.rs/concepts/backends/mouse-capture/)).
- [`ratatui-interact`](https://lib.rs/crates/ratatui-interact) fills the
  focus/click gap if we want help; optional.
- [TermiPaint](https://lib.rs/crates/termipaint) is an existing mouse-driven
  TUI paint app (click+drag, shape preview, undo/redo) — proof the interaction
  model works, and a reference implementation to read.
- [`askii`](https://github.com/nytopop/askii) (Rust TUI ASCII diagram editor)
  and [MonoSketch](https://github.com/tuanchauict/MonoSketch) are prior art for
  ASCII box/line drawing — read for rendering tricks. Neither is promptable or
  code-bound; that gap is our novelty.

**Node taxonomy — adopt C4, don't invent one:**

- The [C4 model](https://en.wikipedia.org/wiki/C4_model) (Context, Container,
  Component, Code) is the standard vocabulary for app architecture, and
  [Structurizr](https://docs.structurizr.com/as-code) already proves "models as
  code" with a single model projecting many diagrams. We adopt C4's node kinds
  (System / Container / Component) and the "one model, many views" principle.

**Code → graph extraction (structure-only sync):**

- For Rust, [`syn`](https://docs.rs/syn) parses modules/types/fn signatures
  natively — use it for the first language.
- For multi-language later, [tree-sitter](https://understandingdata.com/posts/tree-sitter-turned-everyone-into-a-toolsmith/)
  with S-expression queries extracts symbols across languages with one
  approach; [`ast-grep`](https://github.com/ast-grep/ast-grep) is a Rust
  structural-search tool worth evaluating.

**Aesthetic reference (Bevy):** Bevy UI's stated audience is "games, tools for
games, and game-like things needing CAD-like rendering," and its design notes
emphasise an artist-friendly workflow with **hot-reload** and **separation of
style from structure** ([Bevy UI vision](https://hackmd.io/@bevy/HkjcMkJFC)).
Takeaways we adopt: a themeable style layer kept separate from graph data, and
hot-reload of the graph file so edits (human, agent, or git) reflect live.
*Content rephrased for compliance with licensing restrictions.*

## 5. Data model

Two files per project, under `<project>/.mewcode/canvas/`:

### `graph.json` — semantic layer (THE source of truth)

```jsonc
{
  "version": 1,
  "nodes": [
    {
      "id": "auth",                       // stable, never reused
      "kind": "component",                // C4: system | container | component
      "name": "Authenticator",
      "bind": "crates/engine/src/auth.rs#Authenticator",  // path#symbol, optional until generated
      "contract": [                       // structural promises, language-neutral strings
        "fn verify(&self, token: &str) -> Result<Claims, AuthError>"
      ],
      "tech": "rust",                     // optional hint for codegen
      "desc": "Validates bearer tokens"   // free text for humans + agent
    }
  ],
  "edges": [
    { "from": "auth", "to": "session_store", "kind": "depends" }  // depends | calls | implements | owns
  ]
}
```

Rules:
- `id` is stable and opaque. Renaming `name` never changes `id`.
- `bind` is null until code is generated or a human binds it.
- `contract` is the **only** thing drift detection compares against code.
- This file is what the agent reads/writes and what gets diffed in git.

### `layout.json` — presentation overlay (NOT truth)

```jsonc
{
  "version": 1,
  "positions": { "auth": { "x": 12, "y": 4 }, "session_store": { "x": 12, "y": 10 } },
  "theme": "default"
}
```

Rules:
- Missing position → auto-layout computes one. v1 can ship with this file empty.
- Editing this file never triggers codegen or drift. It is pure view state.
- Drift detection **ignores** this file entirely.

This split is decision 1b and is load-bearing: it's what lets "drag a box" and
"agent edits the graph as text" coexist without lying to the user.

## 6. How it maps onto the existing crates

Nothing here throws away the harness. It's additive.

| Crate | Additions |
|-------|-----------|
| `protocol` | `Graph`, `Node`, `Edge`, `NodeKind`, `EdgeKind`, `Layout` types (serde). Pure data, no I/O — fits the crate's existing "no I/O" rule. |
| `engine` | New tools in `tools/canvas/`: `canvas_read`, `canvas_mutate` (add/remove/rename node or edge, set contract/bind), and later `canvas_gen_code`, `canvas_check_drift`. Registered in `default_registry` exactly like existing tools. Drift/extraction logic in a new `engine/src/canvas/` module (uses `syn`). |
| `server` | Persist `graph.json`/`layout.json` via the existing `SessionStore` pattern; optionally an SSE channel for graph updates (mirrors `/chat`). |
| `client` | The big one: a new `Screen::Canvas` with mouse-driven rendering and a prompt bar. See milestone 1. |

The agent path reuses everything: `canvas_mutate` is just another tool on the
existing registry, gated by the existing Plan/Build mode (design edits are
fine in Plan mode; codegen is a Build-mode tool).

## 7. Milestone roadmap

Build in this order. Each milestone is independently demoable.

| M | Name | Outcome | Doc |
|---|------|---------|-----|
| **1** | **Promptable canvas** | Render a `graph.json` as boxes+edges in the TUI; mouse select/pan; a prompt bar where the agent mutates the graph and you watch it redraw live. **No codegen yet.** | `milestone-1-promptable-canvas.md` |
| 2 | Manual editing | Mouse create/connect/rename nodes; `layout.json` drag-to-move; undo/redo. | TBD |
| 3 | Forward codegen | `canvas_gen_code`: graph → crate/module/trait/struct skeleton (contracts, not bodies). Dogfood by regenerating mewcode's own crate layout. | TBD |
| 4 | Drift detection | `canvas_check_drift`: parse bound symbols with `syn`, diff signatures vs `contract`, report divergence read-only. | TBD |
| 5 | Code → graph sync | On commit, re-extract structure and update/annotate affected nodes; propose graph deltas for human approval. | TBD |

Milestone 1 is deliberately the hard UI problem and contains **zero codegen**,
so it proves the canvas + agent-mutation loop in isolation before any of the
MDA-risky machinery is added.

The visual target for the canvas (draw.io × Warp aesthetic, themes, blocks,
honest terminal ceilings) is specified in `ui-aesthetic.md`.

## 8. Risks & ceilings

- **Layout in a character grid.** Edge routing without crossings is the weak
  point of every ASCII layout engine. Mitigation: spike `ascii-dag` in M1
  before committing; accept "good enough" routing as a known ceiling.
- **Round-trip ambition creep.** The instant someone asks for behavioural sync,
  point them back at decision 2. Structure only.
- **Terminal mouse portability.** Mouse capture support varies across
  terminals/multiplexers (tmux, some Windows consoles). Mitigation: every
  mouse action must have a keyboard equivalent; mouse is an enhancement, not a
  requirement.
- **Drift false-positives.** Comparing signature strings is brittle
  (formatting, generics). Mitigation: normalise via `syn` AST, not text.

## 9. Open questions (resolve before the milestone that needs them)

- M3: target language(s) for codegen v1 — Rust-only first? (Recommend yes.)
- M3: codegen idempotency — how to regenerate without clobbering human edits to
  generated files (marker comments? a generated/ split? owned regions?).
- M5: git integration mechanism — post-commit hook, or poll `git diff` on
  demand from a tool? (Recommend on-demand tool first; hook later.)

## Sources

- Why MDA/MDD failed — https://www.quora.com/Why-did-model-driven-architecture-development-fail
- AI code drift, plan-as-source-of-truth — https://www.mindstudio.ai/blog/ai-generated-code-drift-cost-analysis/
- Architectural drift guardrails — https://medium.com/toward-next-ai/ai-coding-agent-architecture-guardrails-how-to-stop-agents-from-passing-tests-while-breaking-7c66927cb6a3
- C4 model — https://en.wikipedia.org/wiki/C4_model
- Structurizr (models as code) — https://docs.structurizr.com/as-code
- Ratatui mouse capture — https://ratatui.rs/concepts/backends/mouse-capture/
- ascii-dag — https://lib.rs/crates/ascii-dag
- layout-rs — https://lib.rs/crates/layout-rs
- rust-sugiyama — https://lib.rs/crates/rust-sugiyama
- TermiPaint (mouse TUI reference) — https://lib.rs/crates/termipaint
- askii (ASCII diagram editor) — https://github.com/nytopop/askii
- Bevy UI vision — https://hackmd.io/@bevy/HkjcMkJFC
- tree-sitter / ast-grep — https://github.com/ast-grep/ast-grep
