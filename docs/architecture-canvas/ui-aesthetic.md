# UI & Aesthetic — draw.io × Warp, in a terminal

> The look and feel target for the Architecture Canvas: the **spatial,
> shape-and-connector canvas of draw.io** fused with the **polished, blocks +
> command-palette + theming UX of Warp** — rendered in a terminal with
> `ratatui`.

Read `README.md` (data model, decisions) and `milestone-1-promptable-canvas.md`
(tasks) first. This doc defines the *visual system* those tasks render into,
and what is M1 vs deferred.

---

## 1. The honest ceiling (read this before designing anything)

**Warp is not a TUI.** It's a GPU-rendered native app (custom Rust UI on the
GPU) that happens to be a terminal. That's how it gets smooth text selection,
pixel-perfect rounded blocks, and buttery scrolling
([How Warp Works](https://www.warp.dev/blog/how-warp-works)). We are building a
**character-grid TUI**. So we borrow Warp's *interaction patterns and
information design*, not its rendering fidelity.

What a terminal grid can and cannot do:

- ✅ Rounded "cards" — via Unicode box-drawing `╭ ╮ ╰ ╯` (`ratatui`'s
  `BorderType::Rounded`, built in).
- ✅ Rich color — 24-bit truecolor per cell where the terminal supports it
  (`COLORTERM=truecolor`); ratatui ships an RGB example.
- ✅ Approximate gradients/glow — per-cell truecolor ramps + half/quarter block
  glyphs `▀ ▄ █ ░ ▒ ▓`. Looks great in modern terminals, coarse in old ones.
- ✅ A dotted canvas grid — light `·` glyphs on the background.
- ❌ Sub-character positioning, true antialiasing, free-floating overlap with
  alpha. Everything snaps to the cell grid.
- ⚠️ All of the above **degrades**: detect color support and fall back
  truecolor → 256 → 16. Never assume truecolor.

Design rule: **fancy where it's free, graceful where it's not.** A node is a
rounded card with a colored title bar regardless of color depth; the gradient
glow is an enhancement that simply turns off on a 16-color terminal.

## 2. What we take from each (distilled, cited)

**From draw.io** ([editor grid](https://about.draw.io/docs/manual/editor/panels/editor-grid-change/),
[connectors](https://www.drawio.com/doc/faq/connectors),
[format panel](https://www.drawio.com/doc/faq/format-panel-show-hide)):
- A **dotted grid** canvas background that shapes align to.
- A **left shape palette** rail — here, the C4 node kinds you can add.
- A **right format/properties panel** for the selected element (name, kind,
  contract, bind).
- **Connectors with arrowheads** and a clear source→target direction.
- *Content rephrased for compliance with licensing restrictions.*

**From Warp** ([how Warp works](https://www.warp.dev/blog/how-warp-works),
[editor](https://docs.warp.dev/terminal/editor/),
[themes](https://docs.warp.dev/terminal/appearance/themes/),
[command palette](https://docs.warp.dev/)):
- **Blocks**: the agent conversation is grouped into discrete blocks
  (prompt → response → tool calls), not a raw scrollback stream.
- A **real input editor** for the prompt (selection, cursor movement,
  multi-line) — you already use `tui-textarea`, which fits.
- A **command palette** (e.g. `Ctrl-P` / `/`) with visual menus for actions.
- A **named-slot theme system** (background, accent, fg, success, warning,
  error, etc.) so users can re-skin without code edits.
- *Content rephrased for compliance with licensing restrictions.*

## 3. Screen layout

A draw.io-style three-pane shell with a Warp-style prompt/block strip docked at
the bottom. Panes are toggleable for narrow terminals.

```text
┌ mew · architecture canvas ───────────────────────────── ⌘P palette ─ ◑ theme ┐
│ PALETTE  │  · · · · · · · · · · · · · · · · · · · · · ·  │ INSPECTOR          │
│          │  · · · ╭───────────╮ · · · · · · · · · · · ·  │ Authenticator      │
│ ▢ System │  · · · │ Gateway   │·······╮· · · · · · · ·   │ kind: component    │
│ ▣ Cont.  │  · · · ╰─────┬─────╯ · · · ·│· · · · · · · ·  │ bind: …/auth.rs#…  │
│ ◆ Comp.  │  · · · · · · │ depends · · ·▼· · · · · · · ·  │ contract:          │
│          │  · · · ╭─────┴──────╮ ╭───────────╮ · · · ·   │  fn verify(&self…) │
│ + add    │  · · · │ SessionSvc │ │ Authentr. │ · · · ·   │                    │
│          │  · · · ╰────────────╯ ╰───────────╯ · · · ·   │ [edit] [unbind]    │
├──────────┴───────────────────────────────────────────────┴────────────────────┤
│ ▍ block ─ you: add an auth component that depends on the session store          │
│ ▍ mew: added Authenticator (component) + edge → SessionSvc        ✓ 1.2s        │
│ ▸ ____________________________________________________________________  ⏎ send  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

- **Top bar:** title, palette hint, theme toggle. Thin, high-contrast.
- **Left palette:** C4 node kinds + "add" (in M1 these are visual; mouse-create
  is M2 per the scope guard).
- **Center canvas:** dotted grid, rounded node cards, routed connectors.
- **Right inspector:** properties of the selected node (read-only in M1).
- **Bottom block strip:** Warp-style blocks of the agent exchange + the prompt
  editor.

## 4. Visual system

### Theme (named slots, Warp-style)

A `Theme` struct with semantic slots, loaded from a TOML theme file (ties into
Phase 15 config). Reuse or vendor [`ratatui-themes`](https://github.com/ricardodantas/ratatui-themes)
for ready palettes, or define our own slots:

```text
bg, surface, grid_dot, fg, fg_dim,
accent, accent_alt,
node_system, node_container, node_component,   // C4 color coding
edge, edge_selected, arrowhead,
success, warning, error, block_bar
```

Every view reads colors from `Theme`, never hardcodes. Color depth is detected
once at startup; the theme resolves each slot to the best available
representation.

### Nodes as cards

- Rounded border (`BorderType::Rounded`).
- A **colored title bar** by C4 kind (`node_system` / `node_container` /
  `node_component`) — instant visual taxonomy.
- Selected node: `edge_selected`/`accent` border + subtle title-bar emphasis.
- Optional truecolor "glow" row under the card as an enhancement (off below
  truecolor).

### Connectors

- Routed by the layout engine (M1 task T2); drawn with box-drawing lines and an
  arrowhead glyph (`▶ ◀ ▲ ▼`) at the target.
- Edge kind conveyed by style: `depends` solid, `calls` thin, `implements`
  could use a different glyph/color later (defer styling variety to M2).

### Canvas grid

- Background `·` dots on `grid_dot`, spaced every N cells, scrolled with the
  viewport so it reads as a real plane.

### Optional flourish (`ratatui-glamour`)

[`ratatui-glamour`](https://lib.rs/crates/ratatui-glamour) brings Lip
Gloss-style expressive borders, gradients, and layered composition with little
code. Candidate for the top bar accent and node glow. *Optional dependency —
only pull it in if the hand-rolled theme can't get the look; justify with a
`// ponytail:` note if added.*

## 5. Warp-isms in the block strip

- **Blocks:** each agent turn renders as a bordered block with a left accent bar
  (`▍`), a role label, and a status chip (`✓ 1.2s`, `⏳`, `✗`). This maps
  cleanly onto your existing `StreamMsg` lifecycle (`Started`/`Delta`/
  `ToolInput`/`ToolOutput`/`Finished`/`Failed`).
- **Prompt editor:** keep `tui-textarea`; style it as a single rounded input
  with a `▸` prompt glyph and a `⏎ send` affordance.
- **Command palette:** a centered overlay list (reuse the existing `Overlay`
  pattern in `runtime/view/overlay.rs`) triggered by `Ctrl-P`, listing canvas
  actions (add node, toggle panes, switch theme, export Mermaid). Fuzzy filter.

## 6. How this layers onto Milestone 1

Keep M1 shippable; don't gold-plate. Concrete split:

**In M1 (fold into existing tasks):**
- A `Theme` struct + color-depth detection. Small, foundational, everything
  else reads from it. Add as a sub-step of **T4** (render) — call it **T4a**.
- Rounded node cards with C4 title-bar colors, dotted grid, arrowhead
  connectors. This is just *how* T4 renders; not extra scope.
- The three-pane + bottom-block shell layout (static panes; toggling can be
  minimal). Part of **T4**.
- Block-style rendering of the agent exchange reusing `StreamMsg` — fold into
  **T7**.

**Deferred to M2+ (explicitly out of M1):**
- Command palette overlay.
- Minimap.
- `ratatui-glamour` gradients/glow, node enter animations.
- Theme switching UI + multiple shipped themes (M1 ships one good default).
- Edge-kind styling variety.

This respects the M1 scope guard: the canvas should look intentional and
polished on day one (cards, grid, color taxonomy, blocks), but the heavy
flourish and interactive chrome wait until the core loop is proven.

## 7. Ceilings (`ponytail:`)

- Truecolor-dependent flourishes (gradients, glow) are enhancements; the UI must
  be fully legible and usable at 16 colors. Detect and degrade.
- No sub-cell rendering; everything snaps to the grid. "Rounded" = box-drawing
  glyphs, not real curves.
- Mouse/animation smoothness is capped by the 50 ms tick and terminal redraw —
  fine for selection/pan, not for 60fps motion. Don't promise Warp smoothness.
- Narrow terminals: panes must collapse (palette/inspector hide < some width)
  so the canvas stays usable. Define breakpoints in T4.

## Sources

- How Warp works (blocks, GPU UI, input editor) — https://www.warp.dev/blog/how-warp-works
- Warp themes — https://docs.warp.dev/terminal/appearance/themes/
- Warp editor — https://docs.warp.dev/terminal/editor/
- draw.io grid — https://about.draw.io/docs/manual/editor/panels/editor-grid-change/
- draw.io connectors — https://www.drawio.com/doc/faq/connectors
- draw.io format panel — https://www.drawio.com/doc/faq/format-panel-show-hide
- ratatui-glamour (gradients, expressive borders) — https://lib.rs/crates/ratatui-glamour
- ratatui-themes — https://github.com/ricardodantas/ratatui-themes
- ratatui RGB/truecolor example — https://github.com/ratatui/ratatui/blob/main/examples/apps/colors-rgb/README.md
- Codex TUI (ratatui AI-agent TUI reference) — https://openai-codex.mintlify.app/architecture/tui
