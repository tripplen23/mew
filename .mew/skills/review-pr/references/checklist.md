# PR Review Checklist

A condensed reference for the review-pr skill. The body of `SKILL.md`
walks you through the procedure; this file is the actual checklist
the model should tick off line by line. Read with
`skill_view(name="review-pr", path="references/checklist.md")`.

## Correctness
- [ ] No off-by-one errors in slice indices or loop bounds
- [ ] Null / `None` / `Err` paths are handled (no `.unwrap()` on user input)
- [ ] No race conditions: shared state is either immutable, behind a
      mutex, or behind an actor / channel
- [ ] Async: every `.await` is on a `Send` future; no `Rc` across awaits
- [ ] API usage matches the crate's docs (not just compiles)

## Tests
- [ ] Every new code path has a test
- [ ] Tests are *meaningful* (exercise the change, not just compile)
- [ ] Failing case is included for each `match` / `if let` branch
- [ ] Property / fuzz test for any non-trivial input parser

## Style
- [ ] Diff matches the surrounding code's conventions
- [ ] No new `#[allow(...)]` / `#[cfg(...)]` without a comment
- [ ] Public items are documented; private items have a one-line
      WHY-comment when the name is not enough
- [ ] No new dependencies without a comment explaining why

## Naming
- [ ] Functions are verbs, types are nouns, modules are short and
      domain-flavoured
- [ ] No `foo`, `bar`, `data`, `info` except in tests / mocks
- [ ] No abbreviations unless they appear in the rest of the file

## Error handling
- [ ] Errors are specific enough to be actionable (no `anyhow!("oops")`)
- [ ] User-facing errors are not raw `Debug` prints
- [ ] The error path is exercised by a test

## Public API
- [ ] Backwards compatible (no removed items, no signature changes)
- [ ] Migration path documented in the commit message if not
- [ ] Changelog / docs updated
