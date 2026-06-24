# Contributing

Thanks for contributing to mewcode.

## Build

```bash
cargo build
cargo test
cargo clippy --workspace --all-targets
```

## Architecture

A Cargo workspace with four crates:

- `crates/protocol` — wire-protocol types. No I/O. The single source of truth for messages, models, tools, skills, and the streaming event shape.
- `crates/engine` — AI agent harness. Talks to OpenCode Go, registers local tools, runs the tool-calling loop.
- `crates/server` — axum backend. Local filesystem session storage, SSE chat streaming, OpenCode Go proxy.
- `crates/client` — ratatui TUI.

The dependency direction is a partial order, not a strict tree. The actual graph:

```
client ────────────────► protocol
server ──► engine ────► protocol
server ────────────────► protocol
```

`protocol` is the bottom layer — it has no mewcode dependencies, so any other crate can use it. The only hard rule is **no back-edges**: nothing depends on `client` or `server`. If you find yourself wanting `crate::client::...` inside `engine`, that's a design problem.

## Code style

### Docstrings

**Concise, elegant, enough information.** A docstring should tell the reader what the item is *for* and any non-obvious trade-off, not narrate what the code already says.

- One to three sentences is the right length for almost every item.
- Don't restate the function signature. `/// Build a new harness.` above `pub fn new() -> Self` is noise.
- Don't paste in long bulleted lists. A single sentence with the *why* is better.
- Module-level doc comments: 3–10 lines. State the module's purpose, link to an external spec if relevant, and (for layout-affecting modules) explain the file layout.
- Public types and functions should have a doc comment. Private items usually don't — the name should be enough.
- Cross-reference with `[`Type`]` and `[`crate::module::Type`]`. Use the link so `cargo doc` is useful.

### Inline comments

**Explain *why*, not *what*.** A comment that says "what" is duplicating the code; a comment that says "why" is capturing a decision the code can't.

Good (why):
```rust
// Tools are loaded wholesale (not progressively disclosed) per the
// Anthropic guide — the model needs the schema to call them.
```

Bad (what):
```rust
// Set the tool block.
let tool_block = format_tool_descriptors(tools);
```

When *what* and *why* coincide, prefer the code; add a comment only when the comment adds information the reader can't recover by reading the code.

When in doubt, leave it out. Code is read more often than it's written; lean toward less prose.

**Docstring vs inline — the default for a function, struct, enum, or `impl` block is a `///` docstring placed *above* the definition, not an inline `//` comment inside the body.** The docstring gets picked up by `cargo doc`, IDE tooltips, and `rust-analyzer` hovers; the inline comment doesn't. Section-divider comments (`// --- transcript ---`) and labels that restate the next line of code are also discouraged — the code is the section header.

### Third-party documentation references

When a docstring names a crate we depend on, link the specific item on docs.rs so the reader can jump to the upstream API in one click — `[`CompletionModel`](https://docs.rs/rig-core/latest/rig_core/completion/trait.CompletionModel.html)`, not a bare `CompletionModel`. The form is the rustdoc markdown link; the URL is the canonical `https://docs.rs/<crate>/latest/<crate>/…` for the crate root or the per-item anchor (`/struct.Foo.html`, `/trait.Bar.html`, `/enum.Baz.html`, `/fn.qux.html`) when one item is the subject.

### Tests

**All tests live in external `tests/*.rs` files — never as `#[cfg(test)] mod tests` blocks inside source files.** Source files are 100% production code; reading `crates/<crate>/src/<file>.rs` from top to bottom should show you only the API, never the tests that exercise it.

Layout per crate:

```
crates/<crate>/
├── src/<file>.rs          ← production code only
└── tests/
    ├── <area>_smoke.rs    ← fast, opt-in by default
    └── <area>_tests.rs    ← per-area integration tests
```

Rationale:

- **One rule, easy to teach.** A single rule ("tests live under `tests/`") is easier to remember and to enforce than a sliding scale ("small tests inline, large tests external").
- **Source files stay focused.** No test scaffolding interleaved with the production code. The test/code ratio of a source file tells you nothing about its design.
- **External tests exercise the public API.** That catches accidental breakage of the contract a downstream consumer sees, which `#[cfg(test)] mod tests` inside the source file can't (it has private access).
- **Black-box by default.** If a test genuinely needs a private item, that's a signal to make the item `pub(crate)` and write a doc comment explaining *why* it's exposed. Don't reach inside the module.

Adding a new test: create `crates/<crate>/tests/<area>.rs` and `use mewcode_<crate>::...` like any downstream user would. No `use super::*`.

### Magic strings

**No unnamed string with semantic meaning in source code.** A magic string is a string literal that carries domain meaning (URL, env-var name, default value, route path, file name, log level, mode name, model id) and shows up either more than once or where the next reader will want to know what it means. Name it as a `pub const`.

**Where they live**
- **Cross-crate conventions** (route paths, env-var names, config file name, default model id) go in `protocol` — `protocol::routes`, `protocol::env`, `protocol::model`. Part of the public API.
- **Per-crate defaults** (default host/port, env-var prefix, default theme, default base URL) go in the owning crate's `config.rs`.
- **Exempt:** test data, log prose, format strings, literal output, and `#[serde(rename = "...")]` arguments (serde needs a literal; use a sibling `pub const` for the runtime value).

```rust
// good
.route(HEALTH, axum::routing::get(routes::health::health))
let api_key = env::var(OPENCODE_GO_API_KEY)?;

// bad: same route and env var, but as raw string literals
.route("/health", axum::routing::get(routes::health::health))
let api_key = env::var("OPENCODE_GO_API_KEY")?;
```

**Why** — one source of truth, renames are one compiler-checked diff, `cargo doc` lands on the constant.

### Prompt format

System prompts are built from named `&'static str` helpers for static sections and `writeln!` for dynamic lines; each section includes its own leading blank line. Don't chain `push_str("...")` on inline literals — the source becomes unreadable and the layout drifts. Canonical examples: `crates/engine/src/agent/prompt.rs` and `crates/engine/src/skills/catalog.rs`.

```rust
// good
fn mode_section(mode: Mode) -> &'static str {
    Mode::Build => "\n\n## Mode: BUILD\n..."
}
let mut out = String::new();
out.push_str(intro());
out.push_str(mode_section(mode));
for d in &descriptors {
    let _ = writeln!(out, "{}", format_tool_descriptor(d));
}
```

## Project conventions

- **No emoji in code, comments, or commits** unless explicitly asked.
- **Don't add comments unless asked** (per the project AGENTS.md).
- **Match existing style** when editing. If nearby code uses `///` doc comments, you use `///` doc comments. If nearby code doesn't, neither do you.
- **Touch only what you must.** Refactors should be motivated by a concrete need, not by aesthetics.
- **All tests live in external `tests/*.rs` files** — never as `#[cfg(test)] mod tests` blocks inside source files. Source files are 100% production code.
- **The CLI is `mewcode`** (not `mewcode-tui` or `mewcode-client`). The server is `mewcode-server`.

## Pull requests

- Title: one-line summary of the change.
- Body: 2–3 sentences on *why*. If you're fixing a bug, link the issue.
- Run `cargo test` and `cargo clippy` before opening.
- If you change the public protocol (`protocol::` types, `StreamEvent`, etc.), call it out in the description — downstream consumers need to know.
