# Mew roadmap

This roadmap replaces the previous UI-led phase list. The existing agent, protocol, server, TUI, persistence, tools, skills, tracing, and MCP work form the foundation. New phases follow the product workflow defined in [`docs/PRD.md`](docs/PRD.md): extract behavioral DNA, approve it, evolve an implementation, and prove the result.

A phase is complete only when its acceptance criteria pass in a clean environment. Dates are deliberately omitted; evidence decides when a phase is ready to close.

## Current status

| Milestone | Outcome | Status |
| --- | --- | --- |
| Foundation | General local coding-agent substrate | Complete |
| M0 | Product contract and evaluation laboratory | Active |
| M1 | Durable migration runs | Planned |
| M2 | Isolated acquisition and reproduction | Planned |
| M3 | Behavioral DNA extraction | Planned |
| M4 | Contract review and evolution planning | Planned |
| M5 | Checkpointed implementation loop | Planned |
| M6 | Differential verification and handoff | Planned |
| M7 | Browser-observed application reconstruction | Planned |
| M8 | Computer-use and non-web drivers | Planned |
| M9 | Overnight reliability and team operation | Planned |

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

## M0: product contract and evaluation laboratory

**Goal:** prove the workflow before turning every step into Rust infrastructure.

### Deliverables (M0)

- Product requirements document and shared vocabulary.
- Separate versioned skill pack for repository mapping, behavioral-contract extraction, evolution planning, and differential verification.
- Machine-readable schemas for run manifests, evidence, contracts, plans, and verification reports.
- At least three golden tasks spanning more than one system shape.
- A Hermes laboratory profile with minimal tools and explicit safety policy.
- A record of human approvals, failed assumptions, repeated mechanics, and missing runtime primitives from real runs.

### Golden task categories (M0)

1. deterministic library, CLI, or HTTP migration;
2. framework migration with a public contract;
3. small authorized website reconstruction with a deliberate new art direction.

### Acceptance criteria (M0)

- Three tasks complete from intake to verification without fabricated output.
- Every critical contract claim points to evidence.
- Each task succeeds twice from a clean workspace with pinned source and tools.
- Unexplained contract deviations are zero.
- Repeated workflow mechanics are classified as runtime candidates or skill-level reasoning.
- No task writes outside its approved workspace or leaks a secret into artifacts.

## M1: durable migration runs

**Goal:** make a migration run a first-class durable object independent from chat history.

### Deliverables (M1)

- Protocol types for run ID, phase, status, policy summary, approval request, artifact reference, checkpoint, and failure reason.
- Persistent `RunRecord` with atomic updates.
- Append-only event and evidence logs.
- Run APIs for create, inspect, list, pause, resume, cancel, and approve.
- SSE events for phase transitions, approvals, evidence, checkpoints, and completion.
- TUI and MCP surfaces for run status and pending approvals.
- Heartbeat and stale-run detection.

### Acceptance criteria (M1)

- A process can stop during each non-terminal phase and resume without manual state edits.
- Completed events and approvals survive an immediate process kill.
- A run can be inspected without loading its model transcript.
- Terminal states are explicit: completed, failed, cancelled, or blocked.
- Existing chat and MCP behavior remains compatible or the protocol change is documented.

## M2: isolated acquisition and reproduction

**Goal:** create immutable baselines and writable candidates under enforced policy.

### Deliverables (M2)

- Source locator and full-revision pinning.
- Per-run baseline and candidate workspace manager.
- Git clone, worktree, and archive acquisition paths.
- Environment and toolchain inventory.
- Build, test, run, and health-check command discovery.
- Process lifecycle with timeout, output limits, cancellation, and resource capture.
- Initial OCI sandbox with filesystem and network policy.
- Source-lock and environment artifacts.

### Acceptance criteria (M2)

- The same pinned source reproduces twice from clean storage.
- Baseline files remain unchanged throughout a run.
- Candidate tools cannot escape configured roots.
- Network access outside the allowlist is denied or surfaced as a policy violation.
- Missing secrets, services, data, or hardware produce a blocker rather than synthetic success.
- A failed setup includes the exact command, exit state, captured output, and recovery options.

## M3: behavioral DNA extraction

**Goal:** turn source and observations into an evidence-backed draft contract.

### Deliverables (M3)

- Repository cartography for components, entrypoints, dependencies, boundaries, tests, schemas, and configuration.
- Common interaction-driver lifecycle: prepare, observe, act, capture, reset, close.
- CLI, HTTP, and deterministic fixture drivers.
- Baseline scenario recorder.
- Evidence references for repository locations, command output, traces, responses, and fixture results.
- Draft contract format with invariants, target behavior, tolerances, unknowns, confidence, and coverage.
- Performance baseline support for latency, throughput, startup, memory, and CPU where measurable.

### Acceptance criteria (M3)

- Every critical draft invariant is observed, user-provided, or explicitly marked as an inference.
- The system reports unknown and unobserved behavior instead of filling gaps.
- Recorded scenarios can be replayed against the same baseline.
- Benchmark artifacts contain method, environment, samples, and raw results.
- A performance-migration prompt can validly conclude that the proposed rewrite is unsupported by measurements.

## M4: contract review and evolution planning

**Goal:** convert observed behavior and user intent into an approved, versioned target.

### Deliverables (M4)

- Contract diff and review flow in API, TUI, and MCP.
- Approval records tied to exact contract versions.
- Explicit categories for preserve, change, remove, unknown, and accepted deviation.
- Target architecture and dependency decision records.
- Official-SDK and license provenance checks.
- Migration slices, each tied to contracts, validation, risk, and rollback.
- Contract amendment flow that retains prior versions and decisions.

### Acceptance criteria (M4)

- Implementation cannot start without an approved contract and plan.
- An assumption cannot silently become an approved invariant.
- A dependency decision records selected and rejected options with provenance.
- Community bindings or custom protocols require a documented reason when no approved first-party path is available.
- Each slice can be implemented, reviewed, and rolled back independently.

## M5: checkpointed implementation loop

**Goal:** evolve the candidate in small slices while protecting the approved contract.

### Deliverables (M5)

- Git-native checkpoint and rollback support.
- Slice executor with focused budgets and completion criteria.
- Test- or fixture-first implementation flow.
- Focused build, test, lint, and static-analysis gates.
- Failure taxonomy: baseline defect, candidate defect, environment failure, nondeterminism, policy violation, intentional change, or inconclusive.
- Repeated-action and no-progress detection.
- Decision request when a failure needs semantic input.

### Acceptance criteria (M5)

- Each passing slice ends at a reproducible checkpoint.
- A failed slice can roll back without affecting earlier passing slices.
- Mew cannot weaken a contract, test, or tolerance solely to obtain a pass.
- Loop exhaustion pauses with evidence and a recovery proposal.
- Restarting the runtime resumes from the last durable checkpoint.

## M6: differential verification and handoff

**Goal:** prove what the evolved implementation preserves, changes, and leaves unknown.

### Deliverables (M6)

- Common scenario runner for baseline and candidate.
- Comparators for exact data, normalized text, structured HTTP behavior, numerical tolerance, errors, timing, resource use, and side effects.
- Nondeterminism detection and repeated-trial policy.
- Parity and benchmark reports.
- Artifact bundle exporter.
- Pull-request exporter with reproduction commands, contract coverage, deviations, risks, and rollback.
- GitHub status suitable for team review.

### Acceptance criteria (M6)

- A reviewer can reproduce each final verdict from structured artifacts.
- Every contract item is pass, fail, accepted deviation, or inconclusive.
- Missing observations are never represented as passes.
- Existing and generated tests pass from a clean candidate checkout.
- The pull request contains only the approved migration scope.
- Mew does not merge or deploy without a separate explicit approval.

## M7: browser-observed application reconstruction

**Goal:** apply the same evidence and contract model to authorized web applications.

### Deliverables (M7)

- Playwright driver running as an external managed process or service.
- Route and crawl-boundary discovery.
- Optional structured-content extraction for page inventory; browser replay remains the source of behavioral evidence.
- Real browser input for links, controls, forms, keyboard navigation, and responsive states.
- DOM and accessibility snapshots.
- Screenshot and approved-region visual comparison.
- Console, request, response, redirect, and failure capture.
- Content and asset inventory with provenance.
- Visual-direction contract that separates preserved behavior from intended redesign.

### Reference journey (M7)

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

### Acceptance criteria (M7)

- Browser actions use real input events rather than synthetic DOM shortcuts.
- A finite journey suite replays from a clean browser context.
- Console and network errors are included in verification.
- The contract identifies copied, transformed, replaced, and excluded content or assets.
- Publication blocks when authorization or provenance is unresolved.
- A visually different candidate can pass because behavior and target art direction, not pixel identity, define success.

## M8: computer-use and non-web drivers

**Goal:** observe and reconstruct authorized applications beyond the browser.

### Deliverables (M8)

- Computer-use driver for screen, keyboard, pointer, window, and clipboard state.
- Driver adapters for local desktop applications and remote test machines.
- Recorded input and reset strategy.
- Optional device, simulator, or hardware driver contract.
- Privacy zones and capture redaction.
- Human takeover and emergency stop.

### Acceptance criteria (M8)

- Every action is tied to an approved application and scope.
- Sensitive screen regions and clipboard values can be excluded from evidence.
- Runs can reset to a known state before replay.
- Unsupported or nondeterministic interactions are marked inconclusive.
- Authentication bypass, purchases, publishing, and production changes remain approval-gated.

## M9: overnight reliability and team operation

**Goal:** make long-running migration work safe to leave unattended and easy to review as a team.

### Deliverables (M9)

- Supervisor integration and automatic restart.
- Resource, disk, cost, rate-limit, and deadline budgets.
- Backoff and credential-refresh pause states.
- Artifact retention and garbage collection.
- Multi-run dashboard and team approvals.
- Role and policy separation for operator, reviewer, and approver.
- Evaluation corpus and regression runner across supported migration lanes.
- Stable external API and versioned MCP surface.

### Acceptance criteria (M9)

- A multi-hour run survives runtime restart, transient provider failure, and client disconnect.
- Stuck work is detected and paused without corrupting baseline or candidate state.
- Team members can audit and approve without sharing the original agent session.
- Runtime releases pass the golden migration and browser-reconstruction corpus.
- Cost and resource ceilings stop work predictably and preserve a resumable state.

## Promotion rule: skill to runtime

Hermes remains the laboratory for workflows that still depend on exploration and judgment. A mechanic moves into Mew when all of the following are true:

1. it recurs across successful runs;
2. it has structured input, output, and failure states;
3. it needs crash safety, policy enforcement, performance, or stable integration;
4. a golden evaluation can prove the native implementation matches the laboratory workflow.

Mew owns mechanics and proof. Skills retain evolving procedures and judgment until they are stable enough to promote.

## Product-level definition of done

The roadmap's first complete loop is M0 through M6. At that point a user can provide source and an evolution goal, approve behavioral DNA, interrupt and resume execution, and receive a reviewable candidate with reproducible evidence.

M7 extends the same guarantees to websites. M8 extends them to computer-use targets. Neither lane is considered complete if it bypasses the contract, evidence, approval, durability, or verification model established in M0 through M6.
