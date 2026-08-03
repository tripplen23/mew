# Skills in Mew: how other harnesses do it, and what Mew adopts

Status: research deliverable — how other harnesses handle skills, and what Mew adopts.

## What a skill is (the Agent Skills standard)

A skill is a folder containing:

- `SKILL.md` (required): instructions in Markdown with YAML frontmatter;
- `scripts/` (optional): executable code;
- `references/` (optional): docs loaded as needed;
- `assets/` (optional): templates, fonts, icons.

Frontmatter requires `name` (kebab-case, matches folder name) and `description`
(what it does + when to use it, under 1024 chars, no XML tags, include trigger
phrases). Optional fields: `license`, `compatibility`, `metadata`.

This format is read by Claude Code, Codex CLI, OpenCode, Cursor, Gemini CLI,
and Pi. It is the closest thing the ecosystem has to a portable skill standard
(agentskills.io). A skill written for one harness generally works in another.

## Progressive disclosure

All three major harnesses use the same three-level model:

1. **L0 — frontmatter**: always in the system prompt; enough to decide when to
   load the skill, without paying its body cost every turn.
2. **L1 — SKILL.md body**: loaded when the skill is relevant; full instructions.
3. **L2 — linked files**: sub-files (`references/`, `scripts/`, `assets/`)
   navigated only as needed.

Mew implements exactly this: `catalog_for_system_prompt` (L0),
`skill_view` without `path` (L1), `skill_view` with `path` (L2) in
`mew-core/crates/engine/src/skills/`.

## How each harness handles skills

### Claude Code

- Skills live in `~/.claude/skills/<name>/SKILL.md`; project skills can shadow
  personal ones.
- Auto-invocation: the model decides from the catalog whether a skill applies.
  Skills can also be triggered explicitly (`/skill-name`, `use_skill(...)`).
- Ecosystem is the largest: skill marketplaces, the official skills guide, and
  a `skill-creator` workflow. Subagents, hooks, and slash commands are
  separate mechanisms that often ship alongside skills.
- Distribution: GitHub repos with `skills/` folders; `skills-lock.json`
  versioning in larger packs (e.g. juliusbrussee/caveman).

### OpenCode

- Skills are markdown files with the same frontmatter, loaded from
  `~/.config/opencode/skills` and `.opencode/skills` (project overrides
  global, same shadowing rule as Mew).
- The skill tool injects the matched skill's body into context on demand
  (progressive disclosure), and the catalog appears in the system prompt.
- `AGENTS.md` carries repo-level behavioral instructions; skills carry
  task-level procedures. Mew's `AGENTS.md` support mirrors this split.

### Pi

- Supports the Agent Skills standard with progressive disclosure and reads
  `~/.claude/skills` for compatibility, so Claude Code skills work in Pi.
- Pi's differentiator is composability: extensions, prompt templates, and
  hooks are first-class; skills are one component of a customizable harness.
- Skills are portable into Pi with no conversion in most cases.

## Porting a skill to Mew (verified by the caveman pilot)

The caveman pack (7 skills, MIT, juliusbrussee/caveman) was ported in this
run. What it took:

1. **Format check** — all seven were standard frontmatter + body; Mew's
   `parse_skill_md` parsed them unchanged.
2. **Adaptation** — only harness-specific mechanics needed changes:
   - `caveman-stats` shipped as a Claude Code hook (Mew has no hook runtime)
     → re-authored as instruction text reading Mew's own token accounting.
   - `caveman-compress` shipped Python scripts calling the Anthropic SDK →
     re-authored to compress in-head; scripts kept out.
   - `cavecrew` named Claude Code subagent presets → generalized to role names.
   - Pure instruction skills (`caveman`, `caveman-commit`, `caveman-help`,
     `caveman-review`) ported as-is.
3. **Attribution** — MIT requires notice; each ported skill keeps
   `license: MIT` and `metadata: {author, source, revision}` in frontmatter.
4. **Quality bar** — per the Anthropic guide: no `README.md` inside skill
   folders, kebab-case names matching `name`, descriptions with triggers.
5. **Placement** — the pack lives in the global config
   `~/.config/mew/skills/`, not in the repo, so every Mew instance (any
   project, any cwd) sees the skills without committing third-party content
   to the product repo. Verified live: `GET /skills` reports the seven with
   `source: global`.

Conclusion: the ecosystem's SKILL.md format is standard enough that Mew can
adopt third-party skills with minimal friction. The recurring porting work is
not format conversion but *harness-mechanics adaptation* (hooks, subagent
names, session logs). A future skill-import command should check for exactly
these patterns.

## Writing skills for Mew

Follow the Anthropic guide's bar:

- Folder: kebab-case, matching `name`; only `SKILL.md` + `scripts/` +
  `references/` + `assets/` inside.
- Description: `[what] + [when] + [key capabilities]` with trigger phrases.
- Body: numbered steps, expected outputs, examples, troubleshooting.
- Progressive: move detail to `references/`; the body tells the model when to
  read them via `skill_view(name=..., path=...)`.
- No XML in frontmatter (it lands in the system prompt — injection surface).
- Where to put them: project skills in `<repo>/.mew/skills/`, personal in
  `~/.config/mew/skills/`. Project shadows global.

## Mew's gap vs. the ecosystem

| Capability | Claude Code | OpenCode | Pi | Mew |
|---|---|---|---|---|
| SKILL.md loading | yes | yes | yes | yes |
| Progressive disclosure L0/L1/L2 | yes | yes | yes | yes |
| Project shadows global | yes | yes | yes | yes |
| Skill authoring guide in-repo | yes | docs | docs | **missing** (this doc starts it) |
| Import/port command | marketplace | no | extensions | **missing** |
| Skill versioning (lockfile) | yes (packs) | no | no | **missing** |
| Harness-mechanics validation on import | marketplace review | no | no | **missing** |

The M0 self-healing skill model in `docs/PRD.md` §15 covers the authoring and
quality side; the import/validation command is a candidate for a later run.

## References

- Anthropic, *The Complete Guide to Building Skills for Claude*:
  https://resources.anthropic.com/hubfs/The-Complete-Guide-to-Building-Skill-for-Claude.pdf
- juliusbrussee/caveman (port pilot): https://github.com/juliusbrussee/caveman
- Pi skill compatibility: https://github.com/disler/pi-vs-claude-code/blob/main/COMPARISON.md
