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

## Porting a skill to Mew

The Agent Skills format is standard across harnesses (Claude Code, OpenCode,
Pi), so third-party skills usually port with only harness-mechanics
adaptation, not format conversion:

1. **Format check** — standard frontmatter + body parses unchanged via
   `parse_skill_md`.
2. **Adaptation** — look for harness-specific mechanics and re-author them:
   - hooks (Mew has no hook runtime) → instruction text;
   - scripts calling a harness SDK → re-author in-skill;
   - subagent presets named for another harness → generalize to role names;
   - pure instruction skills → port as-is.
3. **Attribution** — keep the upstream `license` and add
   `metadata: {author, source, revision}` to the frontmatter.
4. **Quality bar** — no `README.md` inside skill folders, kebab-case names
   matching `name`, descriptions with triggers.
5. **Placement** — personal skills go in `~/.config/mew/skills/` (not the
   repo), so every Mew instance sees them without committing third-party
   content to the product repo.

A future skill-import command should check for exactly these patterns.

## References

- Anthropic, *The Complete Guide to Building Skills for Claude*:
  https://resources.anthropic.com/hubfs/The-Complete-Guide-to-Building-Skill-for-Claude.pdf
