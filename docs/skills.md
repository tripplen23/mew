# Skills in Mew

How to write skills for Mew and how to port third-party skills into it.

A skill is a folder containing `SKILL.md` (required) with YAML frontmatter,
plus optional `scripts/`, `references/`, and `assets/`. Frontmatter requires
`name` (kebab-case, matches the folder name) and `description` (what it does +
when to use it, under 1024 chars, no XML tags, include trigger phrases).

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

## Porting a skill to Mew (verified by the caveman pilot)

The caveman pack (7 skills, MIT, juliusbrussee/caveman) was ported in a pilot
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
4. **Quality bar** — no `README.md` inside skill folders, kebab-case names
   matching `name`, descriptions with triggers.
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

## References

- Anthropic, *The Complete Guide to Building Skills for Claude*:
  https://resources.anthropic.com/hubfs/The-Complete-Guide-to-Building-Skill-for-Claude.pdf
- juliusbrussee/caveman (port pilot): https://github.com/juliusbrussee/caveman
