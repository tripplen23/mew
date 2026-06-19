# mewcode — implementation phases

This file tracks the build order agreed in the kickoff plan. Each phase ends with a checkpoint
(stated below) so progress is reviewable in small slices.

[tool-guide]: https://www.anthropic.com/engineering/writing-tools-for-agents
[skills-guide]: https://resources.anthropic.com/hubfs/The-Complete-Guide-to-Building-Skill-for-Claude.pdf

## Phase 1 — Workspace skeleton ✅
- [x] Cargo workspace at `/home/binhfnef/Projects/personal/mew_code/mewcode`
- [x] Toolchain pin (`rust-toolchain.toml`, stable, edition 2024)
- [x] `Cargo.toml` workspace manifest with shared dep versions
- [x] `.gitignore`, `.env.example`, `rustfmt.toml`
- [x] README with project status
- [x] Four crates: `protocol`, `engine`, `server`, `client`
- [x] Two binaries: `mewcode` (CLI dispatcher / TUI), `mewcode-server`
- [x] Wire-protocol types: `Message`, `MessagePart`, `ToolCall`, `ToolResult`, `ModelId`, `Mode`, `StreamEvent`, `ChatRequest`
- [x] Server endpoints: `GET /health`, `GET /models`, `GET/POST /sessions`, `GET /sessions/{id}`, `POST /chat` (SSE)
- [x] Engine `Harness` with placeholder streaming reply (drives the wire protocol end-to-end)
- [x] `mewcode` CLI subcommands: `tui`, `server`, `version`, `hello`
- [x] 8 unit tests passing in `mewcode-protocol`
- [x] `cargo clippy --workspace --all-targets` clean (2 non-blocking `result_large_err` warnings on figment)

Checkpoint: `cargo build` succeeds, all 8 unit tests pass, and the end-to-end SSE
chat pipeline works against the in-memory placeholder (verified with curl).

## Phase 2 — Anthropic-aligned tools + Skills skeleton ✅
- [x] `protocol::tool` rewritten following the [Anthropic tool guide][tool-guide]:
  - [x] `ToolDescriptor { name, description, input_schema, annotations, examples, max_response_chars }`
  - [x] `ToolAnnotations { read_only, destructive, open_world, idempotent }` (MCP-style)
  - [x] `ResponseFormat` enum (`concise` / `detailed`)
  - [x] `ToolExample { description, input }`
  - [x] `ToolError` variants with optional actionable `hint`
  - [x] `ToolErrorPayload` JSON returned to the model on error
  - [x] `truncate_with_marker()` helper for token-efficient responses
  - [x] `resolve_inside_root()` helper for path safety
  - [x] snake_case tool names (`read_file`, `write_file`, `list_directory`, `glob`, `grep`, `edit_file`, `bash`)
  - [x] 11 unit tests covering all of the above
- [x] `protocol::skill` skeleton following the [Anthropic Skills guide][skills-guide]:
  - [x] `Skill { name, description, body, location, assets }`
  - [x] `parse_skill_md()` for `SKILL.md` YAML frontmatter + markdown body
  - [x] `SkillError` (Read / MalformedFrontmatter / MissingField)
  - [x] Constants: `SKILL_FILE`, `GLOBAL_SKILLS_DIR`, `PROJECT_SKILLS_DIR`
  - [x] 5 unit tests
- [x] `engine::skills` module:
  - [x] `SkillRegistry` with `load_defaults()` (global `~/.config/mewcode/skills/`, project `.mewcode/skills/`, plus dev `./skills/`)
  - [x] `LoadedSkill { skill, source: SkillSource }`
  - [x] `find_project_skills_dir_from()` (walks up to repo root)
  - [x] `find_dev_skills_dir_from()` (dev convenience)
  - [x] `catalog_for_system_prompt()` renders the Anthropic-recommended catalog
  - [x] `resolve_body()` for the `use_skill` tool
  - [x] 6 unit tests
- [x] `engine::tools` module:
  - [x] `ToolRegistry` (registry + dispatch returning `ToolErrorPayload`)
  - [x] `ProjectContext` shared with every tool
  - [x] `ReadFileTool` — first real working tool, with all Anthropic guidance
  - [x] `ListDirectoryTool`
  - [x] `GlobTool` (uses `globset` + `ignore`)
  - [x] `UseSkillTool` — the only way the model can read a skill body
  - [x] `default_registry()` factory
  - [x] 5 unit tests
- [x] `engine::agent`:
  - [x] `build_system_prompt(mode, &skills)` — mode-aware, injects skill catalog
  - [x] 4 unit tests (BUILD mentions write tools, PLAN doesn't, skills inject, no-skills = no catalog)
- [x] `engine::harness`:
  - [x] Takes `Arc<SkillRegistry>` + `Arc<ToolRegistry>`
  - [x] `system_prompt()`, `skill_count()`, `tool_names()` accessors
  - [x] Placeholder reply advertises skill count + tool list
- [x] Two bundled sample skills at `skills/review-pr/SKILL.md` and `skills/write-rust-error/SKILL.md`
- [x] Smoke test that confirms the bundled skills load via `load_defaults()`
- [x] **Progressive disclosure** wiring:
  - [x] `format_tool_descriptors(&ToolRegistry)` in `engine::agent` renders full descriptors
    (name, description, safety, schema, examples) sorted alphabetically
  - [x] `build_system_prompt` accepts `&ToolRegistry` and injects the descriptors after
    the mode-specific prose
  - [x] Skill catalog (name + description only) appended last; body remains on demand
    via `use_skill`
  - [x] `dump_system_prompt` example lets you eyeball the result
  - [x] Tests:
    - `tool_descriptors_are_injected_when_present`
    - `empty_registry_yields_no_tool_block`
    - `tools_are_sorted_alphabetically`
    - `build_mode_includes_tool_descriptors`
    - `plan_mode_excludes_write_tool_descriptors`

Checkpoint: 33 unit tests pass (15 engine + 18 protocol), workspace builds clean,
the SSE chat pipeline advertises its tool + skill count in the placeholder
reply, and the Anthropic-aligned tool design is wired through the
registry.

## Phase 3 — `server` skeleton ✅
- [x] axum app with `GET /health`
- [x] Config loader (figment: env + optional toml)
- [x] Error type with `IntoResponse`

## Phase 4 — Persistence layer (filesystem) ✅
- [x] `SessionStore` trait with `FsStore` (default) + `MemoryStore` backends
- [x] Sessions persist as `meta.json` + `messages.jsonl` per session under the XDG data dir
- [x] Routes: `GET /sessions`, `POST /sessions`, `GET /sessions/{id}`, `DELETE /sessions/{id}`, `GET /storage/status`

## Phase 5 — `client` shell ✅
- [x] ratatui event loop, root layout
- [x] Home screen lists sessions from server
- [x] First `insta` snapshot of home screen

## Phase 6 — New session flow ✅
- [x] Title / mode / model picker dialogs
- [x] POST to server, navigate to session screen

## Phase 7 — Engine v0 ✅
- [x] rig Anthropic-compat client for `https://opencode.ai/zen/go/v1/messages`
- [x] First end-to-end smoke test

## Phase 8 — Streaming
- Wire rig streaming completion into SSE on the server
- Tokens stream live to the TUI

## Phase 9 — First tool
- `read_file` as `#[rig::tool]`, exercised end-to-end with tracing span
- Ref: [Anthropic tool guide][tool-guide]

## Phase 10 — Remaining tools
- `write_file`, `edit_file`, `list_dir`, `glob`, `grep`, `bash`
- PLAN mode gate
- Tracing span on every tool
- Ref: [Anthropic tool guide][tool-guide]

## Phase 11 — Skills runtime
- Skill hot-reload: pick up new or changed `SKILL.md` files without restarting
- Skill assets: bundle files alongside the body, exposed via `use_skill`
- Lint `SKILL.md` frontmatter on load, surface errors at boot
- More bundled sample skills (`explain-error`, `refactor-rust`)
- Ref: [Anthropic Skills guide][skills-guide]

## Phase 12 — TUI polish
- Markdown rendering (`tui-markdown`)
- Code blocks with `syntect`
- Tool cards, theme switcher, slash command menu, @-mention popover
- Toast, trace pane, animations

## Phase 13 — Session resume
- Load history from the session store, replay into the agent

## Phase 14 — Config & persistence
- `~/.config/mewcode/config.toml`
- Last-used model, theme, recent sessions

## Phase 15 — Hardening
- Error toasts, Ctrl-C graceful shutdown, retries, command palette
