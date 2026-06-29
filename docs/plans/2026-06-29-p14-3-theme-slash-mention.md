# P14.3 — Theme + Slash command menu + @-mention

> **For Hermes:** Implement PR-by-PR using `test-driven-development`. Each
> PR is independently mergeable. Don't combine them — small PRs.

**Goal:** Complete Phase 14 (TUI polish) with three orthogonal features:
a unified `Theme` extracted from the 30 hardcoded `Color::*` calls, a
slash-command popover that fires on `/`, and a file-mention popover that
fires on `@`.

**Recon findings (not assumptions — verified):**

- `MessagePart::FileMention { path }` already exists in `protocol` and is
  rendered in `runtime/view/session.rs` as `@path` (blue). Only the
  **input** (user picking a file) is missing.
- The submit handler in `update/session.rs` matches `/tools` and
  `/skills` only. No menu UI on typing.
- 30 `Color::*` calls scattered across 7 view files. No `Theme` type.

**Architecture:** All three features are read-only view/UI additions.
The only state-shape change is `SessionState` gaining a draft-`Message`
buffer (replacing the flat `TextArea`) so the input can hold a list of
`MessagePart` items, not just text. That's a single structural change
that unblocks both slash menu and @-mention.

**Tech stack:** ratatui 0.30, tui-textarea 0.7 (read-only mode for
display; we'll render the draft buffer ourselves), existing
`crossterm::event::KeyEvent` handler in `update/session.rs`.

---

## PR 1: P14.3a — Theme extraction (no UX change)

**Why first:** The popovers in PR 2/3 will introduce a dozen new `Style`
values. Doing the theme refactor first means the new popovers are
written in the new style from day one (no second refactor).

**Ceiling:** Single default `Theme`. No variant switching. No user
config. The user said "default for now, add variants later" — so we
build the struct, wire every existing call site to it, and stop. Any
"variant switching" code we add now is YAGNI.

### Files

- Create: `crates/client/src/runtime/view/theme.rs`
- Modify: `crates/client/src/runtime/view/mod.rs` (re-export `Theme`)
- Modify: 7 view files (replace 30 `Color::*` with `theme::*`)
- Test: `crates/client/tests/theme.rs`

### Theme shape

```rust
// theme.rs

/// All visual constants the TUI needs. Single source of truth so
/// `Theme` can grow a `switch(variant)` method later without touching
/// every call site.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub status_bar: Color,         // was: DarkGray
    pub user_label: Color,          // was: Green
    pub assistant_label: Color,    // was: Cyan
    pub tool_label: Color,          // was: Magenta
    pub streaming: Color,           // was: Yellow
    pub file_mention: Color,        // was: Blue
    pub overlay_border: Color,      // was: Cyan
    pub overlay_hint: Color,        // was: DarkGray
    pub tool_call_header: Color,    // was: Cyan
    pub tool_result_ok: Color,      // was: DarkGray
    pub tool_result_error: Color,   // was: Red
    pub toast_info: Color,          // was: Blue
    pub toast_error: Color,         // was: Red
    pub toast_neutral: Color,       // was: DarkGray
}

impl Default for Theme {
    fn default() -> Self { Self::dark() }
}

impl Theme {
    /// The current default. Same colors as today's hardcoded values,
    /// just lifted into a struct.
    pub const fn dark() -> Self { /* … */ }
}
```

### Refactor strategy

Each `Color::*` callsite becomes `theme.<field>`. Each view function
takes a `theme: &Theme` parameter. `render(frame, app)` constructs
`Theme::default()` once and threads it through.

### Test surface

- `default_theme_matches_today()` — assert every field equals the
  current hardcoded `Color::*` value. Pinned in a single test so a
  later aesthetic change is one intentional edit, not a hunt.
- (No visual tests — the existing render tests still pass, and any
  visual diff is in the eye of the user.)

### Acceptance

- 30 `Color::*` calls replaced with `theme.<field>` reads.
- `cargo test -p mewcode-client` green (no behavior change).
- 1 new test file, 1 test.
- Single commit: `refactor(client): extract Theme from 30 hardcoded Color::*`.

---

## PR 2: P14.3b — Slash command menu (popover UI)

**UX target (Hermes/opencode pattern):**

1. Type `/` in input → popover appears above the input bar
2. Type more chars → fuzzy-filter the list
3. `Up`/`Down` to highlight, `Tab` to autocomplete into input
4. `Enter` to autocomplete + submit (executes the command)
5. `Esc` to close popover (but keep `/<partial>` in input)
6. Re-typing `/` in an empty input re-opens
7. Status bar updates: `/tools  /skills  /model  /session  /theme  /help`

**Command registry (initial set):**

| Command | Action | Already wired? |
|---|---|---|
| `tools` | Open `Overlay::Tools` | yes |
| `skills` | Open `Overlay::Skills` | yes |
| `model` | Open a model picker (new screen or overlay) | no — defer or stub |
| `session new` | `Cmd::CreateSession` with title input | partial |
| `session list` | Go home (Esc-equivalent) | yes |
| `theme` | Stub: toast "no variants yet" | new — matches "default for now" |
| `help` | Show keybinding overlay | new — small |
| `quit` | Exit the TUI | no — small |

**Pragmatic cut for this PR:** Ship `/tools`, `/skills`, `/session new`,
`/session list`, `/theme`, `/help`, `/quit` (7 commands, all
testable). **Defer `/model`** — that's its own PR (new screen).
Document `/model` as "coming soon" in the popover with a dimmed
visual treatment, so users know it exists.

### Files

- Modify: `crates/client/src/runtime/model/states/session.rs`
  - Replace `input: TextArea<'static>` with `input: InputDraft` where
    `InputDraft` holds both the raw text and the parsed command
    candidate (so the popover can fuzzy-filter without re-parsing on
    every keystroke).
- Modify: `crates/client/src/runtime/view/session.rs`
  - Render popover above the input bar when the input starts with `/`
    and the input is focused.
- Modify: `crates/client/src/runtime/update/session.rs`
  - Wire `Up`/`Down`/`Tab`/`Esc` when popover is open.
- Create: `crates/client/src/runtime/model/commands.rs`
  - Static `&[SlashCommand]` with name, aliases, short help, handler.
- Test: `crates/client/tests/slash_menu.rs` — new file.
- Test: extend `crates/client/tests/update.rs` — keep existing
  `slash_tools_opens_tools_overlay` and `slash_skills_opens_skills_overlay`
  passing.

### InputDraft shape

```rust
pub struct InputDraft {
    /// What the textarea is editing. `Vec<InputToken>` so we can mix
    /// `Text("hello ")` and `FileMention("src/lib.rs")` parts.
    pub tokens: Vec<InputToken>,
    /// Cursor position (token index + char offset within token). For
    /// PR 2 we only need "which token is the cursor in" — fine-grained
    /// cursor-in-text is overkill until PR 3 forces it.
    pub cursor: usize,
}

pub enum InputToken {
    Text(String),
    FileMention(String),
}
```

**For PR 2 specifically:** the draft only holds `Text` tokens
(FileMention is PR 3). The struct exists in this PR so PR 3 is
purely additive.

### Popover render

- Above the input bar (between transcript and input chunks)
- Width: 60% of the area (matches `Overlay`'s `centered_rect(60, 60)`)
- Max 8 visible rows, scrollable if more
- Highlighted row: `Color::Cyan` background
- Footer: hint text (`Tab to fill · up/down to navigate · Esc to close`)

### Test surface

- `slash_menu_opens_on_typing_slash` — input is empty, user types `/`,
  `draft.menu_open` is `true`, popover renders with all 7 commands.
- `slash_menu_filters_as_user_types` — type `/to`, list narrows to
  `[tools]`.
- `slash_menu_arrow_keys_move_highlight` — `Down` increments
  highlight, `Up` decrements, wraps.
- `slash_menu_tab_autocompletes` — type `/sk`, press `Tab`, input
  becomes `/skills` and popover stays open (until submit or Esc).
- `slash_menu_enter_submits_command` — type `/tools`, press `Enter`,
  `s.overlay == Overlay::Tools` (matches existing behavior).
- `slash_menu_esc_closes_keep_partial` — type `/too`, press `Esc`,
  popover closes, input is unchanged `/too`.
- `slash_menu_quit_command_exits` — type `/quit`, press `Enter`,
  TUI exits.
- `slash_menu_unknown_command_shows_toast` — type `/banana`, press
  `Enter`, toast appears. (Existing behavior.)

### Acceptance

- All 8 tests pass.
- Existing `slash_tools_opens_tools_overlay` and
  `slash_skills_opens_skills_overlay` still pass (the submit path is
  unchanged — the popover is an additive input affordance).
- 1 new file (`commands.rs`), 1 new test file.
- 2-3 commits: input draft refactor, command registry + popover,
  tests.

---

## PR 3: P14.3c — @-mention file picker

**UX target (Hermes/opencode pattern):**

1. Type `@` in input → popover appears above the input bar
2. Type more chars → fuzzy-filter the file list (relative to project
   root)
3. `Up`/`Down` to highlight, `Tab` or `Enter` to insert
4. Inserted mention replaces the `@<partial>` in the input with a
   `FileMention` token; popover closes
5. The mention renders as `@path` in the input bar (with a slightly
   different style to show it's not editable text)
6. On submit, the `Message` carries `MessagePart::FileMention { path }`
   instead of `Text` for the inserted mention.

**File listing source:** New server route `GET /files?prefix=<p>` that
returns `{ "files": [".mewcode/agent.md", "Cargo.toml", ...] }` relative
to the project root. Capped at, say, 50 results, no recursion (user
types more chars to narrow).

**Pragmatic cut for this PR:**

- Server: 1 new route, 1 new test.
- Client: extend `InputDraft` so `FileMention(String)` is a real
  token type (not just `Text`).
- Render: the input bar renders tokens (text + mentions).
- Popover: reuses the popover scaffolding from PR 2 (extracted to
  `popover.rs` in PR 2 so PR 3 doesn't duplicate it).

### Files

- New server route: `crates/server/src/routes/files.rs` (1 file)
- Modify: `crates/server/src/routes/mod.rs` (mount the route)
- Modify: `crates/server/src/lib.rs` (state — already has project root)
- Modify: `crates/client/src/runtime/view/session.rs` (render tokens
  in the input bar; show popover on `@`)
- Modify: `crates/client/src/runtime/update/session.rs` (handle
  popover nav for `@`, and final message composition uses the draft)
- Test: `crates/server/tests/files_route.rs`
- Test: `crates/client/tests/at_mention.rs`

### Wire surface

- The existing `Cmd::StartChat` carries a `Vec<Message>` where the
  user message has `Vec<MessagePart>`. The current code in
  `update/session.rs:108` builds `vec![MessagePart::Text { text: ... }]`
  from the textarea. We replace that with a function
  `draft_to_message_parts(draft) -> Vec<MessagePart>` that walks the
  `InputDraft.tokens` and produces the right mix.

### Test surface (server)

- `files_route_returns_root_listing` — `GET /files?prefix=` returns
  a non-empty list of project files.
- `files_route_filters_by_prefix` — `GET /files?prefix=src/` returns
  only `src/` entries.
- `files_route_rejects_path_escape` — `GET /files?prefix=../` returns
  400 (path stays inside project root, per `resolve_inside_root`).

### Test surface (client)

- `at_popover_opens_on_typing_at` — input is empty, user types `@`,
  draft has `at_popover_open == true`, and the popover renders a
  list of files (from a fixture).
- `at_popover_inserts_filemension_on_select` — type `@sr`, `Down` to
  highlight `src/`, `Enter`, draft.tokens == `[Text(""), FileMention("src/"), Text("")]`.
- `submit_carries_filemension_part` — type `@sr`, select, submit,
  the resulting `Message` has a `MessagePart::FileMention` not
  `MessagePart::Text` for the path.
- `filemension_renders_as_at_path_in_input_bar` — terminal output
  contains `@src/` in the input area with the mention style.

### Acceptance

- 4 client tests + 3 server tests pass.
- 1 new server route, 1 new client test file.
- 2-3 commits: server route, client draft extension + popover, tests.

---

## Sequencing and dependencies

```
PR 1 (theme)   -- independent
PR 2 (slash)   -- depends on PR 1 (new popover widgets use theme)
PR 3 (@-mention) -- depends on PR 1 + PR 2 (reuses popover, reuses InputDraft)
```

PR 1 must land first because the popovers introduce ~15 new `Style`
calls — refactoring first means we only do it once.

PR 2 must land before PR 3 because PR 3 reuses the popover
infrastructure from PR 2 (extracted to a shared `popover.rs` module).

**Why 3 PRs, not 1 big one:** each is independently reviewable, each
can be reverted without taking the others down, and the diffs stay
small enough to read end-to-end (matching the project's small-PR
convention from MEMORY.md).

---

## Out of scope (explicit)

- `/model` command — needs a model picker screen, its own PR.
- Theme variants — `Theme::default()` is the only impl for now.
- File-tree depth — listing is single-level (`src/`, not `src/foo/bar.rs`).
  User narrows by typing more.
- File preview — selecting a file inserts it; doesn't show contents.
- Animated popover open/close — static open, no fade.
- Per-token cursor in the input bar — cursor position is approximated
  to "which token" granularity until a real use case forces finer
  resolution.
- HTTP timeout on `/files` — covered by issue #27 (separate concern).

---

## Files at a glance

| PR | New files | Modified files | Tests added |
|---|---|---|---|
| P14.3a | `view/theme.rs` | 7 view files + `view/mod.rs` | 1 |
| P14.3b | `model/commands.rs`, `view/popover.rs` | `states/session.rs`, `view/session.rs`, `update/session.rs` | 8 |
| P14.3c | `server/routes/files.rs` | `server/routes/mod.rs`, `server/lib.rs`, `view/session.rs`, `update/session.rs` | 7 |

Total: 4 new files, ~10 modified files, 16 new tests.

---

## Verification at the end of each PR

```
cargo fmt --check
cargo clippy -p mewcode-client --tests -- -D warnings
cargo clippy -p mewcode-server --tests -- -D warnings
cargo test -p mewcode-client
cargo test -p mewcode-server
cargo build --workspace
```

All must be green before the PR is opened.
