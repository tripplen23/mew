# mewcode — implementation phases

| # | Phase | Status |
|---|---|---|
| 1 | Workspace skeleton (4 crates, 2 binaries, wire protocol) | ✅ |
| 2 | Anthropic-aligned tools + Skills skeleton | ✅ |
| 3 | `server` skeleton (axum + figment) | ✅ |
| 4 | Persistence (filesystem): `SessionStore` trait, `FsStore` + `MemoryStore`, XDG layout | ✅ |
| 5 | `client` shell (ratatui event loop, home screen) | ✅ |
| 6 | New session flow (title / mode / model pickers) | ✅ |
| 7 | Engine v0 (rig Anthropic-compat client, e2e smoke) | ✅ |
| 8 | Conversation history + session resume (`HistoryStrategy::Raw`) | ✅ |
| 9 | Durable memory scaffold (fact store, `# Memory` preamble, `mewcode_memory` tool) | ✅ |
| 10 | Streaming (rig → SSE → TUI live tokens) | ✅ |
| 11 | Tool-calling loop (`RigToolAdapter`, `MAX_AGENT_TURNS=10`, `agent_tool_e2e.rs`) | ✅ |
| 12 | Remaining tools + PLAN mode gate + Anthropic prompt caching | ✅ |
| 13 | Skills runtime (2-tool progressive disclosure + external dirs) | ⬜ (active) |
| 14 | TUI polish (markdown, code blocks, tool cards, theme, slash menu, @-mention) | ⬜ |
| 15 | Config & persistence (`~/.config/mewcode/config.toml`, recent sessions) | ⬜ |
| 16 | Hardening (error toasts, Ctrl-C graceful shutdown, retries, command palette) | ⬜ |
| 17 | Trace ingestion latency | ⬜ (active) |

## Phase 13 — Skills runtime
- Skill hot-reload: pick up new or changed `SKILL.md` files without restarting
- Skill assets: bundle files alongside the body, exposed via `use_skill`
- Lint `SKILL.md` frontmatter on load, surface errors at boot
- More bundled sample skills (`explain-error`, `refactor-rust`)
- Ref: [Anthropic Skills guide][skills-guide]

## Phase 13 — Skills runtime (active)

**Why:** follow the [Anthropic Skills guide][skills-guide] progressive-disclosure
pattern correctly, support path-based skill loading so other clients can add
their own skills without touching source, and minimise per-turn token spend.

**2-tool API** (Hermes / agentskills.io compatible — not 3):
- `skills_list(name?: string)` — Level 0 catalog. Returns
  `[{name, description, source, assets}, ...]`. ~3 k tokens flat, the only
  part the model must always see (in the system prompt as a compact list,
  not via this tool — the tool is for the model to *re-read* the catalog).
- `skill_view(name: string, path?: string)` — Level 1 (no `path`): full
  `SKILL.md` body. Level 2 (with `path`): one sub-file under the skill
  directory (e.g. `references/forms.md`, `scripts/build.sh`).

The 3-tool split originally proposed (`load_skill` / `read_skill` /
`use_skill`) was rejected because (a) it ships 3 tool descriptors for what
is one mental action, (b) it does not match the agentskills.io open
standard Hermes already conforms to, and (c) the user explicitly invited
a "more minimal" alternative.

**Path-based skill loading** (no source code edit needed to add a skill):
- A skill is a directory containing a `SKILL.md` (YAML frontmatter
  `name` + `description` required) plus any number of sub-folders
  (`references/`, `scripts/`, `assets/`).
- `SkillRegistry::load` is the single entry point and takes an
  `SkillLoadConfig { bundled_dir?, project_dirs: [..], external_dirs: [..] }`.
  Order of precedence: **project > external > bundled** (project shadows
  external shadows bundled on name collision).
- Default discovery: `~/.config/mewcode/skills/` (global),
  `<project>/.mewcode/skills/` (walks up to the nearest), `./skills/`
  (dev). External dirs are added from `mewcode.toml` /
  `MEWCODE_SKILLS__EXTERNAL_DIRS` (CSV) and `${VAR}` is expanded.
  Non-existent paths are silently skipped (Hermes behaviour).

**Token-efficiency** (progressive disclosure, in the order the model hits
each level):
- L0: system prompt gets a *compact* one-line-per-skill list with no body
  (`~80 bytes/skill` vs the current ~250). Tool descriptors are
  shorter — only the two tool names + one-line description, with a
  pointer to the system-prompt catalog for the full per-skill index.
- L1: `skill_view` returns the body but **truncated to a budget**
  (default 100 000 chars; `truncate_with_marker` adds an explicit
  truncation footer so the model knows more is available).
- L2: `skill_view(name, path)` returns a single sub-file. A skill author
  who writes 200 kB of references across 20 files can keep the L0/L1
  cost flat and the L2 cost bounded by what the model actually
  requested.

**Sub-folder discovery** (today assets are tracked but not exposed via
tool). New `skill_view(name, path)` makes them reachable.

**E2E verification**
- A unit test that `load(SkillLoadConfig::default())` discovers
  `./skills/review-pr` and `./skills/write-rust-error`.
- A unit test for external_dir precedence: external `foo` is shadowed
  by project `foo`.
- A unit test that `skill_view("review-pr", "references/checklist.md")`
  returns that file's contents.
- The existing Phase-12 e2e should be extendable: an extra turn
  invoking `skills_list` then `skill_view` against a fresh e2e skill
  and asserting the round-trip works against a real LLM.

[skills-guide]: https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills
[hermes-skills]: https://hermes-agent.nousresearch.com/docs/user-guide/features/skills
[agentskills.io]: https://agentskills.io/

## Phase 14 — TUI polish
- Markdown rendering (`tui-markdown`)
- Code blocks with `syntect`
- Tool cards, theme switcher, slash command menu, @-mention popover
- Toast, trace pane, animations

## Phase 15 — Config & persistence
- `~/.config/mewcode/config.toml`
- Last-used model, theme, recent sessions

## Phase 16 — Hardening
- Error toasts, Ctrl-C graceful shutdown, retries, command palette

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

## Key entry points

| Concern | Location |
|---|---|
| Wire protocol | `crates/protocol/src/` |
| Harness | `crates/engine/src/harness/` |
| System prompt | `crates/engine/src/agent/prompt.rs` |
| Trace helpers | `crates/engine/src/harness/trace.rs` |
| Tool adapter (rig ↔ mewcode) | `crates/engine/src/tools/adapter.rs` |
| Tool registry / mode gate | `crates/engine/src/tools/mod.rs` |
| Memory store | `crates/engine/src/memory.rs` |
| OTel/Langfuse init | `crates/server/src/main.rs:73-120` |
| `/chat` SSE | `crates/server/src/routes/chat.rs` |
| E2E (real LLM + Langfuse) | `crates/server/tests/agent_tool_e2e.rs` |
