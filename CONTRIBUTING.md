# Contributing

Thanks for contributing to Mew.

## Build

All build, test, and lint tasks go through the root `Makefile`. Run `make help` for the full list.

### Quick reference

| Task | Command |
|------|---------|
| Build everything | `make build` |
| Run server + TUI | `make run` |
| Run server only (foreground, logs) | `make run-server` |
| Run TUI only (needs running server) | `make run-tui` |
| Run all tests | `make test` |
| Lint everything | `make lint` |
| Auto-format | `make fmt` |
| CI gate (fmt-check + lint + test) | `make check` |
| Full CI gate (build + check + docs) | `make ci` |
| Clean build artifacts | `make clean` |

Per-language targets are also available if you only want to touch one side:

- Rust workspace (`mew-core/`): `make build-core`, `make test-core`, `make lint-core` (clippy).
- Go MCP server (`mew-mcp/`): `make build-mcp`, `make test-mcp`, `make lint-mcp` (`go vet`).

## Architecture

The Rust workspace lives in `mew-core/`. It is a Cargo workspace with four crates:

- `mew-core/crates/protocol` — wire-protocol types. No I/O. The single source of truth for messages, models, tools, skills, and the streaming event shape.
- `mew-core/crates/engine` — AI agent harness. Talks to OpenCode Go, registers local tools, runs the tool-calling loop.
- `mew-core/crates/server` — axum backend. Local filesystem session storage, SSE chat streaming, OpenCode Go proxy.
- `mew-core/crates/client` — ratatui TUI.

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

**All tests live in external `tests/*.rs` files — never as `#[cfg(test)] mod tests` blocks inside source files.** Source files are 100% production code; reading `mew-core/crates/<crate>/src/<file>.rs` from top to bottom should show you only the API, never the tests that exercise it.

Layout per crate:

```
mew-core/crates/<crate>/
├── src/<file>.rs          ← production code only
└── tests/
    ├── <area>.rs          ← per-area integration tests, one file per area
    └── prop_<topic>.rs    ← property-based tests (proptest), one file per topic
```

Rationale:

- **One rule, easy to teach.** A single rule ("tests live under `tests/`") is easier to remember and to enforce than a sliding scale ("small tests inline, large tests external").
- **Source files stay focused.** No test scaffolding interleaved with the production code. The test/code ratio of a source file tells you nothing about its design.
- **External tests exercise the public API.** That catches accidental breakage of the contract a downstream consumer sees, which `#[cfg(test)] mod tests` inside the source file can't (it has private access).
- **Black-box by default.** If a test genuinely needs a private item, that's a signal to make the item `pub` (so an external test crate can reach it) and write a doc comment explaining *why* it's exposed. `pub(crate)` does **not** help here — files under `tests/` are a *separate* crate, not a submodule of the crate under test. Don't reach inside the module with `use super::*`; that's a code smell that usually means a `pub` item is missing.

Adding a new test: create `mew-core/crates/<crate>/tests/<area>.rs` and `use mewcode_<crate>::...` like any downstream user would. No `use super::*`.

#### Worked example: a renderer and its tests

Suppose `mew-core/crates/client/src/runtime/view/foo.rs` defines a renderer. The tests for it go in `mew-core/crates/client/tests/foo.rs`:

```rust
// mew-core/crates/client/src/runtime/view/foo.rs
//
// `pub` (not `pub(super)`) so external tests can reach it. The two
// helpers are test surface only — `#[doc(hidden)]` keeps them out of
// `cargo doc` so downstream code doesn't depend on their exact shape.
pub fn render_foo(input: &Input) -> Line<'static> { /* ... */ }

#[doc(hidden)]
pub fn foo_helper(s: &str) -> String { /* ... */ }
```

```rust
// mew-core/crates/client/src/runtime/view/mod.rs
mod foo;

pub use foo::{render_foo, foo_helper};  // re-export at the view root
```

```rust
// mew-core/crates/client/tests/foo.rs

use mewcode_client::runtime::view::{render_foo, foo_helper};

#[test]
fn render_foo_handles_empty_input() {
    let line = render_foo(&Input::default());
    assert_eq!(line_text(&line), "…");
}
```

The test file imports through the re-exports on `mewcode_client::runtime::view`, the same path a downstream crate would use. No `use super::*`, no private-module access, no `#[cfg(test)]` block in `foo.rs`.

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

**Every section of the system prompt MUST be wrapped in XML-style tags** (`<tag>...</tag>`). Tags serve as named boundaries that help the model navigate between distinct rule domains — cleaner and more reliable than pure markdown headers.

```xml
<identity>
You are Mew, an expert software engineer...
</identity>

<mode>
You are in BUILD mode...
</mode>

<rules>
1. **Be decisive.** ...
2. **Never re-read files** ...
</rules>

<tools>
### `read_file`
...

### `write_file`
...
</tools>

<skills>
- **review-pr** — Review a pull request.
- **write-migration** — Write a SQL migration.
</skills>
```

**Tag names must be lowercase, descriptive, and use underscores as word separators.** Each tag wraps a single conceptual section — don't nest tags within tags.

Dynamic sections (tool descriptors, skill catalog) are built at runtime; the tag opening goes in the section's header string, the closing tag is appended after building the dynamic content so every section is always properly closed.

Static sections live in `&'static str` helpers — each function returns a self-contained tagged block. Canonical examples: `mew-core/crates/engine/src/agent/prompt.rs` and `mew-core/crates/engine/src/skills/catalog.rs`.

## Project conventions

- **No emoji in code, comments, or commits** unless explicitly asked. The
  one current exception: `🛠️` is the chosen marker for the P14.2 tool card
  header in `mew-core/crates/client/src/runtime/view/tool_card.rs`, in tests that
  assert on it, and in the commit that introduced it — see PR #30.
- **Don't add comments unless asked** (per the project AGENTS.md).
- **Match existing style** when editing. If nearby code uses `///` doc comments, you use `///` doc comments. If nearby code doesn't, neither do you.
- **Touch only what you must.** Refactors should be motivated by a concrete need, not by aesthetics.
- **All tests live in external `tests/*.rs` files** — never as `#[cfg(test)] mod tests` blocks inside source files. Source files are 100% production code.
- **The CLI is `mewcode`** (not `mewcode-tui` or `mewcode-client`). The server is `mewcode-server`.

## Pull requests

- Title: one-line summary following [Conventional Commits](https://www.conventionalcommits.org/) format (see below).
- Body: 2–3 sentences on *why*. If you're fixing a bug, link the issue.
- Run `make check` (or at minimum `make fmt-check lint test`) before opening.
- If you change the public protocol (`protocol::` types, `StreamEvent`, etc.), call it out in the description — downstream consumers need to know.

### Commits and PR titles

PR titles and squash-merge commit titles are the **source of the changelog and the next version number**. The repo uses [release-please](https://github.com/googleapis/release-please-action) which reads these titles automatically on every merge to `master`.

Format:

```text
feat: add schema_validate worker tool
fix: keep baseline files immutable on read-only workers
docs: clarify builder modes in PRD
chore: bump tokio to 1.40
feat!: replace global project root with per-run RunContext
```

Rules:

- One of `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `chore`, `build`, `ci` at the start.
- Lowercase summary, no trailing period, ≤72 characters.
- Breaking changes: append `!` after the type (e.g. `feat!:`) and explain under the **Breaking change** heading in the PR template.
- Version bumps: `feat` → minor, `fix`/`perf` → patch, `feat!`/`fix!` → major.

### Stacked PRs

When a feature has a clear "first slice" that stands on its own and one or more
follow-up slices that depend on it, open the slices as stacked PRs instead of
packing everything into one. The base branch of each follow-up PR is the
**head branch** of the slice it builds on, not `master`.

**Why** — each PR stays small enough to review in one sitting, and reviewers
can merge the foundation without re-reading the whole feature.

**Mechanics**

1. **First slice** — base `master`. This slice is mergeable on its own.
2. **Second slice** — base `<first-slice-branch>`. Title and body should make
   the dependency explicit ("Builds on #N — rebase once #N merges").
3. **Third slice and beyond** — same pattern, base the previous head.
4. After the foundation PR merges, rebase the dependent branch onto `master`
   locally, force-push, then change the base branch on GitHub to `master` so
   the dependency can be unwound before merge.

**In this repo** the slash-command TUI work followed this pattern: the first
PR (`feat(tui): /model and /session slash commands`) ships the picker
overlays and `PATCH /sessions`, and the follow-up (`@-mention` popover)
branches off that first PR's head because the mention picker needs the
picker-overlay machinery the first slice introduced.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE) at the root of this repository.
