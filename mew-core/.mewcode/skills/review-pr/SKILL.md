---
name: review-pr
description: Review a pull request for correctness, style, and test coverage. Use when the user asks you to review code or a diff.
---

# How to review a pull request

When you are invoked via `use_skill("review-pr")`, follow this procedure:

1. **Get the diff.** Use `bash` to run `git diff main...HEAD` (or whatever the
   base branch is). If the repo has no git history, ask the user how to
   obtain the diff.

2. **Read the surrounding code.** For every changed file, read at least
   one related file to understand the conventions the project uses.
   Never review code in isolation.

3. **Checklist.** Walk through each of these for the diff:
   - **Correctness.** Are there off-by-one errors, null/undefined handling
     problems, race conditions, or wrong API usage?
   - **Tests.** Is every new code path covered? Are the tests meaningful
     (do they actually exercise the change, not just compile)?
   - **Style.** Does the diff match the conventions of the surrounding
     code? Flag any new lint suppressions.
   - **Naming.** Are functions, variables, and types named clearly?
   - **Error handling.** Are errors handled, or swallowed? Are they
     specific enough to be actionable?
   - **Public API.** If the diff changes a public API, is it backwards
     compatible? If not, is the migration path clear?

4. **Format your response.** Group feedback by file. Use this structure:
   ```
   ## <file path>
   - <line range or symbol>: <one-sentence finding>
   ```
   End with a single `Verdict: LGTM` line, or a list of `Must-fix:`
   items.

5. **Tone.** Be specific and constructive. Never say "this is wrong" —
   say "this would crash if `foo` is `None`; consider `foo.unwrap_or(...)`".

## What NOT to do

- Don't read files unrelated to the diff.
- Don't run the project's tests unless the user asks — just check that
  they exist and look reasonable.
- Don't propose refactors outside the diff's scope.
- Don't be polite at the cost of clarity: if something is broken, say so.
