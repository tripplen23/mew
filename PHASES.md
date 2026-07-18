# Mew roadmap

New phases follow the product workflow defined in [`docs/PRD.md`](docs/PRD.md): extract behavioral DNA, approve it, evolve an implementation, and prove the result.

A phase is complete only when its acceptance criteria pass in a clean environment. Dates are deliberately omitted; evidence decides when a phase is ready to close.

## Current status

| Milestone  | Outcome                                     | Status   |
| ---------- | ------------------------------------------- | -------- |
| Foundation | General local coding-agent substrate        | Complete |
| M0         | Product contract and evaluation laboratory  | Active   |
| M1         | Durable migration runs                      | Planned  |
| M2         | Isolated acquisition and reproduction       | Planned  |
| M3         | Behavioral DNA extraction                   | Planned  |
| M4         | Contract review and evolution planning      | Planned  |
| M5         | Task orchestration and context lifecycle    | Planned  |
| M6         | Checkpointed implementation loop            | Planned  |
| M7         | Differential verification and handoff       | Planned  |
| M8         | Browser-observed application reconstruction | Planned  |
| M9         | Computer-use and non-web drivers            | Planned  |
| M10        | Overnight reliability and team operation    | Planned  |
| M11        | Installable product and operator CLI        | Planned  |

## Foundation: coding-agent substrate

The completed foundation is retained rather than rewritten.

### Delivered capabilities

- Rust workspace split into protocol, engine, server, and client crates.
- Typed message, model, tool, skill, and streaming event contracts.
- Axum server with configuration, session storage, memory API, and SSE chat.
- Ratatui client with session, model, streaming, markdown, theme, and tool-card flows.
- Rig-based provider abstraction and multi-turn tool-calling loop.
- Read, write, edit, glob, grep, shell, memory, and skill tools.
- Plan and build mode gating.
- Progressive skill discovery with project and external directories.
- Conversation persistence and resume.
- OpenTelemetry and Langfuse integration.
- Go MCP adapter using the official Model Context Protocol SDK.

### Foundation backlog

UI polish, extra tool-card markers, command-palette work, and similar improvements remain valid but no longer define the product's critical path. They should ship only when they improve an active migration workflow.

- Add tool-card expansion only when migration artifacts or policy failures need more than the current compact preview.

## M0: product contract and evaluation laboratory

**Goal:** prove the workflow before turning every step into Rust infrastructure.

### Deliverables (M0)

- Product requirements document and shared vocabulary.
- Separate versioned skill pack for repository mapping, behavioral-contract extraction, evolution planning, and differential verification.
- Machine-readable schemas for run manifests, evidence, contracts, plans, and verification reports.
- At least three golden tasks spanning more than one system shape.
- A Hermes laboratory profile with minimal tools and explicit safety policy.
- A record of human approvals, failed assumptions, repeated mechanics, and missing runtime primitives from real runs.
- Evaluation of manual, supervised, and delegated builders against the same contract and verification criteria.
- Per-task context, input-token, output-token, retrieval, retry, and wall-clock measurements for the golden runs.

### Golden task categories (M0)

1. deterministic library, CLI, or HTTP migration;
2. framework migration with a public contract;
3. small authorized website reconstruction with a deliberate new art direction.

### Acceptance criteria (M0)

- Three tasks produce the required manifest, contract, plan, evidence, and verification artifacts from intake through handoff.
- Every important contract claim points to evidence or is explicitly marked unknown, using the definition in PRD section 8.7.
- Each task succeeds twice from a clean workspace with pinned source and tools.
- Unexplained contract deviations are zero.
- Every workflow mechanic observed in at least two tasks is recorded with an owner and classified by the promotion rule below as runtime candidate or skill-level reasoning.
- No task writes outside its approved workspace or leaks a secret into artifacts.
- At least one task rotates to a fresh worker and completes from structured artifacts without injecting the prior transcript.
- Any context optimization reports total token cost and task outcome against an uncompressed baseline; prompt-size reduction alone is not a pass.

## M1: durable migration runs

**Goal:** make a migration run a first-class durable object independent from chat history.

**PRD coverage:** FR-1, FR-3, FR-12, FR-16; NFR-1, NFR-4, NFR-6.

### Deliverables (M1)

- Protocol types for run ID, phase, status, policy summary, approval request, artifact reference, checkpoint, and failure reason.
- Persistent `RunRecord` with atomic updates, current task, and latest context checkpoint.
- Append-only event and evidence logs.
- Runtime services for durable run and artifact storage, separate from chat session storage.
- Run APIs for create, inspect, list, pause, resume, cancel, and approve.
- SSE events for phase transitions, approvals, evidence, checkpoints, and completion.
- TUI `RunList` and `RunDetail` screens with overview, event timeline, artifact index, latest checkpoint, and pending-approval summary.
- MCP operations for run create, inspect, list, pause, resume, and cancel.
- Heartbeat and stale-run detection.

### Acceptance criteria (M1)

- A process can stop during each non-terminal phase and resume without manual state edits.
- Completed events and approvals survive an immediate process kill.
- A run can be inspected without loading its model transcript.
- An operator can create and inspect the same run through REST, TUI, and MCP.
- The TUI reconstructs run status and its timeline from durable state after a server restart rather than from in-memory chat state.
- Terminal states are explicit: completed, failed, cancelled, or blocked.
- Existing chat and MCP behavior remains compatible or the protocol change is documented.

## M2: isolated acquisition and reproduction

**Goal:** create immutable baselines and writable candidates under enforced policy.

**PRD coverage:** FR-2, FR-11; NFR-2, NFR-3, NFR-8.

### Deliverables (M2)

- Source locator and full-revision pinning.
- Per-run baseline and candidate workspace manager.
- Per-run `RunContext` containing baseline, candidate, artifact, source-lock, and policy references; tools do not derive scope from the server current working directory.
- Git clone, worktree, and archive acquisition paths.
- Runtime-owned Git repository service for revision inspection, worktree creation, checkpoint commits, rollback, and dirty-tree detection.
- Read-only `repo_inspect` worker tool with structured status, head, log, show, and diff operations against named run workspaces.
- Environment and toolchain inventory.
- Build, test, run, and health-check command discovery.
- Policy-bound process manager and `run_command` worker tool with fixed working roots, environment allowlist, timeout, output artifacts, cancellation, and resource capture.
- Initial OCI sandbox with filesystem and network policy.
- Source-lock and environment artifacts.

### Acceptance criteria (M2)

- The same pinned source reproduces twice from clean storage.
- Baseline files remain unchanged throughout a run.
- Candidate tools cannot escape configured roots.
- Repository checkout, worktree, checkpoint, and rollback operations are owned by the runtime rather than composed from unrestricted model shell commands.
- `repo_inspect` returns structured data for both workspaces without mutating either tree.
- Network access outside the allowlist is denied or surfaced as a policy violation.
- Missing secrets, services, data, or hardware produce a blocker rather than synthetic success.
- A failed setup includes the exact command, exit state, captured output, and recovery options.

## M3: behavioral DNA extraction

**Goal:** turn source and observations into an evidence-backed draft contract.

**PRD coverage:** FR-4, FR-6; NFR-4, NFR-5.

### Deliverables (M3)

- Repository cartography for components, entrypoints, dependencies, boundaries, tests, schemas, and configuration.
- Common interaction-driver lifecycle: prepare, observe, act, capture, reset, close.
- CLI, HTTP, and deterministic fixture drivers.
- Baseline scenario recorder.
- Worker tools for `behavior_capture`, `evidence_record`, and `artifact_validate`; schema failures return structured paths and remediation rather than requiring ad-hoc Python or shell parsing.
- Evidence references for repository locations, command output, traces, responses, and fixture results.
- Draft contract format with invariants, target behavior, tolerances, unknowns, confidence, and coverage.
- TUI Evidence and Contract tabs plus a read-only viewer for structured run artifacts.
- Performance baseline support for latency, throughput, startup, memory, and CPU when the target exposes those resources to the active driver.

### Acceptance criteria (M3)

- Every important draft invariant is observed, user-provided, inferred with evidence, or explicitly unknown.
- The system reports unknown and unobserved behavior instead of filling gaps.
- Recorded scenarios can be replayed against the same baseline.
- Every generated contract and evidence artifact passes its versioned schema or blocks the phase with structured validation errors.
- The Evidence and Contract views reconstruct their content from artifact references, not agent transcript prose.
- Benchmark artifacts contain method, environment, samples, and raw results.
- A performance-migration prompt can validly conclude that the proposed rewrite is unsupported by measurements.

## M4: contract review and evolution planning

**Goal:** convert observed behavior and user intent into an approved, versioned target.

**PRD coverage:** FR-7, FR-8.

### Deliverables (M4)

- Contract diff and review flow in API, TUI, and MCP.
- Approval queue and evidence-linked decision panel with accept, reject, defer, comment, and amendment actions.
- A `decision_request` worker tool that can ask for a semantic decision but cannot approve, reject, or amend on behalf of an approver.
- Approval records tied to exact contract versions.
- Explicit categories for preserve, change, remove, unknown, and accepted deviation.
- Target architecture and dependency decision records.
- Official-SDK and license provenance checks.
- Migration slices, each tied to contracts, validation, risk, and rollback.
- Dependency-aware task graph whose nodes are small enough to execute within an explicit context, cost, and deadline budget.
- Contract amendment flow that retains prior versions and decisions.

### Acceptance criteria (M4)

- Implementation cannot start without an approved contract and plan.
- Approval events record actor identity, timestamp, decision, artifact hash, affected contract items, and optional rationale outside the agent transcript.
- An assumption cannot silently become an approved invariant.
- A dependency decision records selected and rejected options with provenance.
- Community bindings or custom protocols require a documented reason when no approved first-party path is available.
- Each slice can be implemented, reviewed, and rolled back independently.

## M5: task orchestration and context lifecycle

**Goal:** turn an approved plan into bounded, resumable assignments so long runs do not depend on one growing model conversation.

**PRD coverage:** FR-15, FR-16; NFR-1, NFR-6, NFR-8, NFR-10.

### Deliverables (M5)

- Versioned `TaskSpec` schema with ID, goal, dependencies, contract coverage, artifact inputs, workspace roots, completion criteria, validation commands, outputs, budgets, and stop conditions.
- Persistent task DAG with ready, running, blocked, passed, failed, and cancelled states.
- Minimal context-pack builder that selects task-relevant artifacts by identity instead of replaying the full transcript.
- Versioned `ContextCheckpoint` schema for decisions, discoveries, changed files, command and test outcomes, failure classification, unresolved questions, artifact references, and next task.
- Configurable context-headroom threshold and worker-rotation policy.
- Builder interface supporting `manual`, `supervised`, and `delegated` modes on the same task and verification contracts.
- Worker tools for `task_status`, `task_report`, and `context_checkpoint`; the task engine validates transitions and completion evidence.
- TUI Tasks tab showing dependency, status, budget, retries, blockers, active builder mode, and latest context checkpoint for each task.
- Token and context accounting that includes retries, retrieval, compaction, and provider calls per task and phase.
- Shape-specific densification interface for eligible structured outputs, with original artifacts retained by hash and equivalence tests for each transform.

### Acceptance criteria (M5)

- A fresh worker completes the next task from its task packet and context checkpoint without access to the previous worker transcript.
- Task completion is impossible without its declared outputs, validation results, and artifact references.
- The task view and worker tools report the same durable state after a restart.
- Context rotation preserves decision IDs, hashes, failures, unknowns, blockers, and unresolved questions; primary evidence is never replaced by a summary.
- A task that cannot fit its budget is split or blocked before execution rather than silently truncating context or exceeding cost policy.
- Manual and delegated builders can implement the same fixture and receive the same parity verdict from the same contract.
- Supervised mode is the default; a delegated builder cannot approve its own contract amendment, deviation, destructive action, or final handoff.
- A context optimization ships only when representative evaluations show equal task outcomes and lower total token cost, including retrieval and retries.

## M6: checkpointed implementation loop

**Goal:** evolve the candidate in small slices while protecting the approved contract.

**PRD coverage:** FR-9, FR-11, FR-15, FR-16; NFR-1, NFR-8, NFR-10.

### Deliverables (M6)

- Git-native checkpoint and rollback support.
- Slice executor with focused budgets and completion criteria.
- Builder adapters for human-authored candidate changes, supervised patch proposals, and delegated workers under the same task contract.
- TUI candidate-diff and checkpoint views with manual verification and rollback actions.
- Test- or fixture-first implementation flow.
- Focused build, test, lint, and static-analysis gates.
- Failure taxonomy: baseline defect, candidate defect, environment failure, nondeterminism, policy violation, intentional change, or inconclusive.
- Repeated-action and no-progress detection.
- Decision request when a failure needs semantic input.
- Worker rotation at task or headroom boundaries using the latest durable context checkpoint.

### Acceptance criteria (M6)

- Each passing slice ends at a reproducible checkpoint.
- Manual and agent-authored candidate changes pass through identical validation, evidence, and parity gates.
- A failed slice can roll back without affecting earlier passing slices.
- Mew cannot weaken a contract, test, or tolerance solely to obtain a pass.
- Loop exhaustion pauses with evidence and a recovery proposal.
- Restarting the runtime resumes from the last durable checkpoint.

## M7: differential verification and handoff

**Goal:** prove what the evolved implementation preserves, changes, and leaves unknown.

**PRD coverage:** FR-10; NFR-2, NFR-4.

### Deliverables (M7)

- Common scenario runner for baseline and candidate.
- Comparators for exact data, normalized text, structured HTTP behavior, numerical tolerance, errors, timing, resource use, and side effects.
- Nondeterminism detection and repeated-trial policy.
- Runtime `ParityEngine` with `parity_run` and coverage-report worker surfaces;
  canonicalization, tolerances, and failure classification are runtime strategies rather than independent shell scripts.
- Parity and benchmark reports.
- TUI parity and artifact views with filters for pass, fail, accepted deviation, and inconclusive contract items.
- Artifact bundle exporter.
- Pull-request exporter with reproduction commands, contract coverage, deviations, risks, and rollback.
- GitHub status suitable for team review.

### Acceptance criteria (M7)

- A reviewer can reproduce each final verdict from structured artifacts.
- The parity view links every verdict to its contract item, comparison rule, baseline and candidate outputs, and supporting evidence.
- Every contract item is pass, fail, accepted deviation, or inconclusive.
- Missing observations are never represented as passes.
- Existing and generated tests pass from a clean candidate checkout.
- The pull request contains only the approved migration scope.
- Mew does not merge or deploy without a separate explicit approval.

## M8: browser-observed application reconstruction

**Goal:** apply the same evidence and contract model to authorized web applications.

**PRD coverage:** FR-4, FR-5, FR-6, FR-10, FR-13; NFR-3, NFR-5, NFR-6.

### Deliverables (M8)

- Playwright driver running as an external managed process or service.
- Named-state screenshot capture with viewport and region metadata.
- Provider-neutral `VisionAnalyzer` bridge to a configured multimodal API or external vision service, with ordered provider fallback.
- Structured visual observations linked to screenshot, crop, rubric, provider, model, and confidence.
- Analysis caching keyed by image hash, region, rubric, and model configuration.
- Route and crawl-boundary discovery.
- Optional structured-content extraction for page inventory; browser replay remains the source of behavioral evidence.
- Real browser input for links, controls, forms, keyboard navigation, and responsive states.
- DOM and accessibility snapshots.
- Screenshot and approved-region visual comparison.
- Console, request, response, redirect, and failure capture.
- Content and asset inventory with provenance.
- Visual-direction contract that separates preserved behavior from intended redesign.

### Reference journey (M8)

```text
Input:
  URL: https://nguyenducbinh.vercel.app/
  Intent: preserve the portfolio's content and important interactions,
          then rebuild it with a stronger manga visual direction.

Expected run:
  map public routes -> exercise interactions -> capture responsive states
  -> draft behavior and content contract -> draft manga design direction
  -> user approval -> implement candidate -> replay journeys
  -> compare behavior, accessibility, console, network, and approved visuals
```

### Acceptance criteria (M8)

- Browser actions use real input events rather than synthetic DOM shortcuts.
- A finite journey suite replays from a clean browser context.
- Console and network errors are included in verification.
- The contract identifies copied, transformed, replaced, and excluded content or assets.
- Publication blocks when authorization or provenance is unresolved.
- A candidate passes when its journey checks and user-approved visual contract pass; pixel similarity is required only for regions explicitly marked for preservation.
- Every visual claim references its image hash, inspected region, rubric, provider, model, and confidence.
- Vision output cannot overrule a deterministic failure or produce an unapproved visual parity verdict.
- Without a configured vision provider, deterministic checks continue and semantic visual items block or request human review rather than silently pass.

## M9: computer-use and non-web drivers

**Goal:** observe and reconstruct authorized applications beyond the browser.

**PRD coverage:** FR-4, FR-6, FR-10, FR-11; NFR-3, NFR-5.

### Deliverables (M9)

- Computer-use driver for screen, keyboard, pointer, window, and clipboard state.
- Native accessibility and UI metadata as the first choice, with screenshot-based vision fallback when structured state is unavailable or insufficient.
- Driver adapters for local desktop applications and remote test machines.
- Recorded input and reset strategy.
- Optional device, simulator, or hardware driver contract.
- Privacy zones and capture redaction.
- Human takeover and emergency stop.

### Acceptance criteria (M9)

- Every action is tied to an approved application and scope.
- Sensitive screen regions and clipboard values can be excluded from evidence.
- Runs can reset to a known state before replay.
- Unsupported or nondeterministic interactions are marked inconclusive.
- Authentication bypass, purchases, publishing, and production changes remain approval-gated.

## M10: overnight reliability and team operation

**Goal:** make long-running migration work safe to leave unattended and easy to review as a team.

**PRD coverage:** FR-1, FR-3, FR-12, FR-16; NFR-1, NFR-4, NFR-7, NFR-8, NFR-10.

### Deliverables (M10)

- Supervisor integration and automatic restart.
- Resource, disk, cost, rate-limit, and deadline budgets.
- Backoff and credential-refresh pause states.
- Artifact retention and garbage collection.
- Multi-run dashboard with phase, status, stale/stuck alerts, pending approvals, and budget state.
- Team approval history that remains reviewable without access to the originating agent session.
- Role and policy separation for operator, reviewer, and approver.
- Evaluation corpus and regression runner across supported migration lanes.
- Stable external API and versioned MCP surface.
- Context-cost dashboard by run, phase, task, model, and artifact class.

### Acceptance criteria (M10)

- A multi-hour run survives runtime restart, transient provider failure, and client disconnect.
- Stuck work is detected and paused without corrupting baseline or candidate state.
- Team members can audit and approve without sharing the original agent session.
- Runtime releases pass the golden migration and browser-reconstruction corpus.
- Cost and resource ceilings stop work predictably and preserve a resumable state.
- Multi-hour runs rotate workers without unbounded transcript growth or loss of task, decision, and evidence references.

## M11: installable product and operator CLI

**Goal:** make Mew a one-command install with a stable operator surface, not a build-from-source research project.

**PRD coverage:** FR-14; NFR-9.

### Deliverables (M11)

- One-command installer for supported platforms (Linux, macOS; Windows if viable).
- Stable `mew` executable that starts or connects to the local runtime and opens the primary interface without exposing internal process names.
- Subcommands: `mew setup`, `mew config`, `mew status`, `mew doctor`, `mew update`, `mew uninstall`, `mew version`.
- Automatic or documented lifecycle for the server and MCP bridge as internal processes managed by `mew`, not by the user.
- Release artifacts with checksums, signatures, SBOM, and build provenance.
- Configuration migration for breaking CLI or config changes between versions.
- Rollback path that preserves user data and run artifacts.

### Acceptance criteria (M11)

- A new machine can install Mew with one command and reach a working TUI without manually starting a server.
- `mew doctor` detects and reports common misconfigurations: missing API key, unreachable provider, stale server, broken MCP bridge, insufficient toolchain.
- `mew update` replaces the installation after verifying checksums and signatures.
- Internal process names (`mewcode-server`, `mew-mcp`) do not appear in normal user-facing documentation or workflows.
- Breaking CLI or config changes between releases produce a migration prompt or automatic migration, not a silent failure.
- Uninstall removes binaries and managed processes but preserves user data and run artifacts by default.

## Capability ownership

Mew does not turn every missing capability into a model-callable tool. Ownership
is selected by whether a capability requires judgment, bounded worker access,
durable enforcement, or operator review.

| Capability                      | Skill responsibility                         | Worker tool                                        | Runtime owner                              | Operator surface                          |
| ------------------------------- | -------------------------------------------- | -------------------------------------------------- | ------------------------------------------ | ----------------------------------------- |
| Repository understanding        | Map architecture, boundaries, and risks      | `repo_inspect`                                     | `GitRepository`, `WorkspaceManager`        | Source/workspace overview                 |
| Source mutation and checkpoints | Recommend the smallest safe change           | None for clone, worktree, commit, or rollback      | `WorkspaceManager`, checkpoint service     | Candidate diff and checkpoint actions     |
| Command execution               | Select relevant build and test commands      | `run_command`                                      | Policy-bound process manager               | Command evidence and policy failures      |
| Behavior discovery              | Design scenarios, edge cases, and properties | `behavior_capture`                                 | Interaction drivers, artifact store        | Evidence and draft-contract views         |
| Artifact conformance            | Interpret validation failures                | `artifact_validate`                                | Schema registry and validator              | Artifact viewer                           |
| Evidence capture                | Decide which observations support claims     | `evidence_record`                                  | Append-only evidence store                 | Evidence browser                          |
| Semantic decisions              | Explain ambiguity and request input          | `decision_request`                                 | Approval service                           | Approval queue and decision panel         |
| Task execution                  | Reason within the assigned task              | `task_status`, `task_report`, `context_checkpoint` | Task engine and context store              | Tasks and context views                   |
| Candidate implementation        | Apply skill-level migration procedure        | Builder adapter selected by mode                   | Workspace, policy, and checkpoint services | Candidate diff, verify, rollback          |
| Differential verification       | Classify mismatches and propose remediation  | `parity_run`                                       | `ParityEngine` and report store            | Parity and coverage views                 |
| Secrets and licenses            | Identify contextual risk                     | No bypass tool                                     | Redaction, provenance, and policy services | Blockers and provenance views             |
| Resource control                | Work within the declared budget              | Read-only run/task status                          | Resource accountant and supervisor         | Cost, token, deadline, and disk dashboard |

Runtime services own invariants and may act during phase transitions without a
model tool call. Worker tools expose the smallest structured interface needed for
one bounded task. TUI, API, and MCP surfaces read the same durable state rather
than reconstructing product state from chat messages.

## Promotion rule: skill to runtime

Hermes remains the laboratory for workflows that still depend on exploration and judgment. A mechanic moves into Mew when all of the following are true:

1. it recurs across successful runs;
2. it has structured input, output, and failure states;
3. it needs crash safety, policy enforcement, performance, or stable integration;
4. a golden evaluation can prove the native implementation matches the laboratory workflow.

Mew owns mechanics and proof. Skills retain evolving procedures and judgment until they are stable enough to promote.

## Product-level definition of done

The roadmap's first complete loop is M0 through M7. Its release gate is the
detailed checklist in [PRD section 19](docs/PRD.md#19-release-criteria-for-the-first-complete-loop).

M8 extends the same guarantees to websites. M9 extends them to computer-use targets. M11 makes the whole package installable and operable as a product. Neither lane is considered complete if it bypasses the contract, evidence, approval, durability, or verification model established in M0 through M7.
