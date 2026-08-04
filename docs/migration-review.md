# Code review in migration runs

Status: process documentation — how code review runs in migration runs.
Deliberately a process change, not an engine gate (no engine change in this run).

## Why

Mew migrates or reconstructs software by extracting behavioral DNA, evolving
an implementation slice by slice, and proving parity. Code review is the
bridge between "the parity report passes" and "the code is worth merging".
PRD §19 (release criteria) already requires a reviewable pull request; this
doc makes the review step explicit in every migration run.

## Reviewer role

PRD §6 defines the Reviewer as a first-class run role: audits evidence,
contracts, plans, and verification results. In practice for each run:

- The human reviewer stays the authority on intent.
- Mew reviews its own candidate work before handoff — the migration author
  must not be the only set of eyes on the produced code.

## Review step in the workflow

After the parity report passes and before handoff (PRD §9.9), run the
`review-pr` skill against the candidate diff:

1. Get the diff: `git diff <baseline>...<candidate>` (or the exported PR).
2. Load the skill: `skill_view(name="review-pr")`, then its
   `references/checklist.md` for the line-by-line checklist.
3. Review against the checklist: correctness, tests, style, naming, error
   handling, public API compatibility.
4. Record findings in the run's `evidence.jsonl` (phase `verify`, action
   `review_findings`) before `final_verification`.
5. Classify each finding: `must-fix` (blocks handoff) or `should` (recorded
   as accepted deviation with approver + contract version, per PRD §9.8).

## Checklist gate

A migration run is complete only when:

- every `must-fix` finding is resolved or approved as a deviation;
- the review findings are recorded with evidence;
- `final_verification` evidence exists (PRD §9.7, step 10).

Keep each finding on one line in the evidence log
(`path:line: severity: problem. fix.`) to stay compact when reviewing
large diffs.

## Out of scope (future)

An engine-level gate that blocks handoff until review evidence exists is a
planned enhancement; this run documents the process only.
