# Contributing to Mew

Thanks for your interest. Mew is in the **M0 (product contract and evaluation laboratory)** stage. Contributions that strengthen the workflow, the schemas, the skill pack, and the durable runtime primitives are welcome; speculative rewrites or new product surfaces are not.

## Workflow

1. Fork and clone.
2. Create a topic branch from `master`:
   ```text
   feat/<short-name>
   fix/<short-name>
   docs/<short-name>
   chore/<short-name>
   ```
3. Make your change in small commits.
4. Run the local checks (see below) before opening a PR.
5. Open a PR against `master` using `.github/PULL_REQUEST_TEMPLATE.md`.
6. Make sure `cargo`, `go`, and `markdownlint` checks pass.
7. A maintainer will review; address review comments with follow-up commits, not force-pushes, unless asked.

## Commits and PR titles

PR titles and squash-merge commit titles are the **source of the changelog and the next version number**. They must follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
feat: add schema_validate worker tool
fix: keep baseline files immutable on read-only workers
docs: clarify builder modes in PRD
chore: bump tokio to 1.40
```

Rules:

- One of `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `chore`, `build`, `ci` at the start.
- Lowercase summary, no trailing period, ≤72 characters.
- Breaking changes: append `!` after the type and explain under the `Breaking change` heading in the PR template.
- `release-please` reads these titles; a PR that uses `feat:` bumps the minor version, `fix:` bumps the patch, `feat!:` triggers a major bump and a dedicated release notes section.

## Development

The repository is a Cargo workspace with a Go MCP adapter. Both must pass locally before pushing.

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace

# Go (mew-mcp)
cd mew-mcp
go vet ./...
go test ./...
go build ./cmd/mew-mcp

# Markdown
npx --yes markdownlint-cli --disable MD013 -- '**/*.md'
```

For the `mew-skills` companion pack, validate skills with the script shipped inside the pack:

```bash
bash mew-skills/scripts/validate.sh
```

## Scope

Mew is intentionally narrow. **Helpful contributions** at M0 look like:

- Strengthening schemas, evidence types, or parity-report fields in `mew-skills`.
- Promoting a skill mechanic that has cleared the promotion rule into a typed runtime service.
- Writing a golden task that exercises one of the three M0 categories: deterministic library/CLI/HTTP, framework migration, or authorised website reconstruction.
- Fixing tool, protocol, or TUI defects listed in the Foundation backlog of `PHASES.md`.
- Improving docs and traceability in `docs/PRD.md` and `PHASES.md`.

**Out of scope right now:**

- New product surfaces not present in `docs/PRD.md`.
- Recursive-self-improvement or autonomous AGI framing.
- Browser/desktop reconstruction code (M8/M9).
- Anything that requires bypassing the baseline/candidate isolation rules.

## Communication

- Open an issue before substantial work to align on direction.
- Use the PR template's **Verification** section with a run id, golden task, or test command.
- Be explicit about risk, rollback, and any breaking change.

## License

By contributing, you agree that your contributions will be licensed under the MIT License at the root of this repository.
