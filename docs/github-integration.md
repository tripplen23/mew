# Mew + GitHub: review bot, auth, and CLI/TUI review comments

Status: research deliverable — GitHub App + auth flows for the review bot (W003/W004).
Implementation of these flows is tracked in GitHub issues; this doc is the
design evidence those issues reference.

## 1. @Mew in pull requests (GitHub App flow)

### Webhook events

| Event | Required repo permission | Use |
|---|---|---|
| `pull_request` | Pull requests (read) | PR opened/synchronized — main trigger |
| `pull_request_review` | Pull requests (read) | Review submitted |
| `pull_request_review_comment` | Pull requests (read) | Line/thread comments |
| `issue_comment` | Issues (read) | PR body/timeline comments — where `@Mew` mentions land |

### @mention trigger design

GitHub Apps do not receive "mentions" as an event. Standard pattern:

1. App subscribes to `issue_comment` (and optionally `pull_request`).
2. On delivery, parse the comment body for `@<app-slug>`.
3. If mentioned, fetch the PR diff:
   `GET /repos/{owner}/{repo}/pulls/{number}` +
   `GET /repos/{owner}/{repo}/pulls/{number}/files` (or `gh pr diff`).
4. Run Mew's `review-pr` skill on the diff.
5. Post results: `POST /repos/{owner}/{repo}/pulls/{number}/reviews`
   (event: `COMMENT` / `REQUEST_CHANGES` / `APPROVE`) with inline comments via
   `POST /repos/{owner}/{repo}/pulls/{number}/comments`.

### App setup

- Create a GitHub App with repo permissions: Pull requests (write to post
  reviews), Issues (read for mentions), Contents (read).
- Public webhook endpoint (server), or GitHub Actions as the simpler host.
- Verify every payload with `X-Hub-Signature-256` HMAC (webhook secret).
- Auth: app private key → JWT (RS256, ≤10 min lifetime) →
  `POST /app/installations/{id}/access_tokens` → installation token (~1h).
  All REST calls use `Authorization: Bearer <installation_token>`.

### Simpler alternative

A GitHub Actions workflow with `on: pull_request` that calls Mew headless.
Caveat: the default `GITHUB_TOKEN` cannot trigger other workflows and has
restricted write scope for reviews; a PAT or app token is needed for real
review posting. The App route is the proper end state.

### Security

Webhook payloads are attacker-controlled (PR titles/bodies can carry prompt
injection). Treat PR content as data, never as instructions. This matches
PRD NFR-3 (hostile inputs, least privilege).

Reference: earendil-works/pi-review is a minimal single-file TypeScript
review bot and the model for a later Mew extension (issue #176).

## 2. GitHub auth harness point

| Option | Best for | Notes |
|---|---|---|
| `gh` CLI (device flow) | First step, local use | `gh auth login` stores a PAT in the keyring; Mew shells out (`gh pr diff`, `gh pr comment`). Zero new credential storage. Same pattern Hermes uses. |
| GitHub App installation token | The @Mew bot (see §1) | Scoped per-installation, ~1h lifetime, revocable, no personal PAT. |
| PAT | Local dev only | Broadest scope; discourage beyond that. |

Harness integration: Mew already has a credential abstraction
(`mew-core/crates/protocol/src/credential.rs`,
`mew-core/crates/server/src/credential.rs`) used for provider keys. A GitHub
token should slot into the same pattern — stored in git-ignored config,
never in run artifacts (PRD NFR-3). CLI surface would be `mew github auth
login` (device flow) or transparent `gh` reuse when `gh` is installed.

## 3. CLI/TUI code review comments — feasibility

Feasible, two shapes:

1. **TUI inline comments (recommended)**: Mew's TUI already renders diffs
   (tool-card and diff display flows). A review mode could attach a finding to
   a diff line (`L42: 🔴 bug: ...`), keep them in memory, then export as a
   GitHub review payload or markdown. This is a TUI-client feature; the
   engine already exposes the review logic via the `review-pr` skill.
2. **CLI command**: `mew review` on a branch/diff, printing findings to
   stdout in the review-pr format. Low cost, testable headless, useful for
   CI.

Recommendation: ship `mew review` (CLI) first — it is the same engine path
the GitHub App will use, and the TUI can reuse its output rendering.

## 4. Recommended sequence

1. `mew review` CLI command (local diff review, no GitHub API needed).
2. GitHub auth harness point: `mew github auth` via `gh` reuse, then app
   installation tokens.
3. GitHub App with webhook → review → comment posting (the @Mew bot).
4. TUI inline review comments (reuses 1's output).

Each step is tracked as a GitHub issue; this doc is the shared reference.
