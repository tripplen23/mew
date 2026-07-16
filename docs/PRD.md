# Mew product requirements document

**Status:** Draft 1

**Product:** Mew

**Product statement:** Mew is an AI agent harness that extracts the behavioral DNA of existing software, then evolves a new implementation under user-defined constraints.

## 1. Summary

Software prototypes are easier to create than ever, but many are built without a path to production. A prototype may prove the product idea while carrying the wrong runtime, framework, architecture, user experience, or operational model for its next stage.

Rewriting it is not primarily a code-generation problem. The hard part is discovering what the existing system actually does, separating intended behavior from incidental implementation details, deciding what must survive, and proving that the replacement still satisfies those decisions.

Mew treats this as a controlled evolution process:

1. acquire and reproduce the source system;
2. observe its structure and behavior;
3. extract a behavioral contract backed by evidence;
4. let the user correct and approve that contract;
5. build an evolved implementation in reviewable slices;
6. compare the source and candidate until the approved contract passes;
7. hand off code, evidence, known deviations, and rollback information.

Mew is not limited to source-to-source migration. With browser or computer-use drivers, it can also study an accessible application from the outside and create a new product that preserves selected interactions while changing its design or implementation. For example:

> Clone `https://nguyenducbinh.vercel.app/`, preserve its information and important interactions, then rebuild it with a stronger manga visual direction.

In that case Mew would inspect the public pages, exercise navigation and interactive states, collect the approved content and behavior, propose a new visual contract, then implement and verify the result. It should not copy protected assets, hidden data, credentials, or brand identity without authorization.

## 2. Product thesis

Mew does not clone code. It extracts behavioral DNA.

Behavioral DNA is the smallest reviewable description of what users and dependent systems can observe and what the owner intends to preserve. It may include:

- public APIs, commands, events, file formats, and configuration;
- user journeys, navigation, controls, visual states, and accessibility behavior;
- numerical invariants, tensor shapes, ordering, timing, and error semantics;
- performance and resource budgets;
- content or domain rules that belong to the product rather than its current implementation;
- approved changes that define how the evolved implementation should differ.

The behavioral contract is not inferred once and treated as truth. Mew presents it to the user with evidence, uncertainty, and unanswered questions. The user remains the authority on intent.

## 3. Problem

### 3.1 Prototype-to-production migration

A working prototype may depend on a language, framework, or deployment model that is unsuitable for production. Conventional rewrite projects fail because knowledge is scattered across source code, runtime behavior, tests, documentation, and the original author's assumptions.

A coding agent can generate replacement code, but compilation and unit tests do not prove parity. Without a behavioral baseline, the agent can silently remove edge cases, alter error behavior, or optimize the wrong bottleneck.

### 3.2 Observable product cloning and transformation

Sometimes the available input is an application rather than its repository. A user may own a site but no longer have a usable codebase, or may want to reproduce a public interaction pattern with a new visual identity.

Screenshots alone are insufficient. They omit routes, responsive states, keyboard behavior, loading and error states, forms, animations, and the relationships between actions. The agent must operate the product, record observations, distinguish facts from guesses, and obtain approval before implementation.

### 3.3 Long-running agent work

Migration and reconstruction runs can take hours or days. They cross network, filesystem, browser, process, and repository boundaries. A chat loop without durable state cannot safely resume after a crash, explain why a decision was made, or prove which source revision and environment produced an observation.

## 4. Goals

Mew must:

1. reproduce an existing system in an isolated workspace when source is available;
2. observe static structure and runtime behavior through interchangeable drivers;
3. produce a behavioral contract whose important claims point to evidence;
4. expose uncertainty and request user decisions at semantic boundaries;
5. create a migration or reconstruction plan in independently reviewable slices;
6. implement the approved plan without modifying the baseline;
7. compare baseline and candidate behavior with explicit tolerances;
8. survive interruption and resume from durable state;
9. emit an auditable artifact bundle and a reviewable code change;
10. support external agents through stable integration surfaces such as MCP.

## 5. Non-goals

Mew is not intended to:

- become a general-purpose autonomous software engineer;
- support every language, framework, browser, desktop environment, and hardware target in the first release;
- infer product intent without user review;
- declare success because code compiles or existing tests pass;
- bypass authentication, paywalls, access controls, robots policies, or platform restrictions;
- copy private data, credentials, proprietary assets, or brand identity without permission;
- silently change the behavioral contract to make a failing candidate pass;
- merge, deploy, purchase, publish, or perform another irreversible action without explicit approval;
- optimize a component before measurements show that it is relevant.

## 6. Target users

### 6.1 Product engineer with a successful prototype

They have a working application and need to change its runtime, framework, architecture, or deployment model without losing behavior.

### 6.2 Team inheriting an unfamiliar system

They need an evidence-backed map of the system before they can modernize or replace it.

### 6.3 Designer or developer reconstructing an owned product

They can provide a URL or runnable binary and want a new implementation with selected behavior preserved and a deliberate new design direction.

### 6.4 Agent operator

They need long-running work to be inspectable, resumable, policy-bound, and safe to leave unattended.

## 7. Core use cases

### 7.1 Runtime or language migration

**Input:** source repository and a goal such as "replace this Python inference service with a production-suitable Rust implementation."

**Expected behavior:** Mew pins the source revision, reproduces the service, profiles it, identifies the actual bottlenecks, extracts contracts and fixtures, proposes dependency substitutions, and migrates one boundary at a time. It compares source and target outputs before recommending cutover.

**Important constraint:** the user's diagnosis is a hypothesis. If profiling shows that native dependencies or model execution dominate latency, Mew must report that evidence rather than manufacture a justification for a rewrite.

### 7.2 Framework migration

**Input:** source repository and a goal such as "move this application from framework A to framework B."

**Expected behavior:** Mew preserves approved routes, data contracts, user journeys, configuration semantics, and failure behavior while replacing framework-specific implementation details.

### 7.3 Website reconstruction with a new art direction

**Input:** an authorized public URL and a goal such as "rebuild this portfolio with more manga energy."

**Expected behavior:** Mew crawls accessible pages, uses a real browser to exercise links and controls, records responsive and interactive states, inventories reusable content, and proposes a visual and behavioral contract. After approval it creates a new implementation and runs browser-level comparisons against the contract.

**Expected evolution:** the candidate should not be a pixel-for-pixel copy unless the user explicitly owns and requests that output. The approved art direction is part of the target contract.

### 7.4 Black-box service reconstruction

**Input:** an authorized CLI, API, local application, or test environment without source code.

**Expected behavior:** Mew records observable inputs, outputs, errors, timing, and state transitions. It labels unobserved behavior as unknown and avoids claims about hidden implementation.

## 8. Concepts

### 8.1 Source system

The repository, URL, binary, API, or application being studied. A run must pin or otherwise identify the exact source state whenever possible.

### 8.2 Candidate

The evolved implementation created by Mew. The candidate always lives in a separate writable workspace from the baseline.

### 8.3 Behavioral contract

A versioned, user-approved set of invariants, tolerances, journeys, performance targets, and deliberate changes.

### 8.4 Evidence

A stable reference to a repository location, command output, runtime trace, browser observation, screenshot, network response, fixture result, benchmark, or official document. Evidence records its source revision, capture time, and content hash where practical.

### 8.5 Interaction driver

An adapter that lets Mew observe or exercise a system. Initial drivers are CLI, HTTP, fixtures, and browser. Computer-use and hardware drivers follow later.

### 8.6 Migration run

The durable unit of work. It owns policy, workspaces, state, artifacts, decisions, checkpoints, and results independently from a chat session.

## 9. Product workflow

### 9.1 Intake

Mew accepts:

- source locator and revision when available;
- the user's desired evolution;
- scope and exclusions;
- target constraints;
- authorization and provenance declarations;
- filesystem, network, secret, and resource policy.

Mew converts free-form intent into a proposed run manifest. The user approves trust-boundary changes before execution.

### 9.2 Acquisition and reproduction

For source-based runs Mew:

- clones or mounts the source at a pinned revision;
- creates separate baseline and candidate workspaces;
- discovers documented build, test, and run commands;
- records toolchain and dependency versions;
- executes the baseline in an isolated environment;
- reports missing credentials, data, hardware, or external services as blockers.

For externally observable systems Mew records the target URL or application version, access conditions, crawl boundaries, and allowed interaction scope.

### 9.3 Reconnaissance

Mew combines static and dynamic investigation:

- repository and dependency mapping;
- entrypoint and boundary discovery;
- test and schema inspection;
- process, log, trace, and network observation;
- CLI, API, fixture, browser, or computer-use interaction;
- baseline performance and resource measurement.

Every important claim is classified as observed, inferred, user-provided, or unknown.

### 9.4 Behavioral DNA extraction

Mew produces a draft contract containing:

- behaviors that must remain;
- behavior that may change;
- explicit target behavior;
- fixtures and user journeys;
- error and edge-case semantics;
- numerical and visual tolerances;
- performance and resource budgets;
- unsupported or unobserved areas;
- evidence references and confidence.

### 9.5 User review

The user can edit, accept, reject, or defer each important contract item. Mew must never convert an assumption into an approved invariant without showing the transition.

The approved contract becomes immutable for the implementation loop. Amendments create a new contract version and preserve the previous decision history.

### 9.6 Evolution plan

Mew maps source responsibilities to target components and proposes slices. Each slice includes:

- scope and owning contracts;
- selected dependencies and provenance;
- implementation strategy;
- validation commands;
- expected risks and deliberate deviations;
- rollback point.

The plan must prefer official SDKs and documented standards. Community bindings or custom protocol implementations require an explicit rationale and approval when a first-party path is unavailable.

### 9.7 Implementation loop

For each approved slice Mew:

1. creates a checkpoint;
2. writes tests or fixtures for the relevant contract;
3. makes the smallest implementation change;
4. builds and runs focused validation;
5. compares baseline and candidate;
6. classifies failures;
7. repeats or pauses for a semantic decision;
8. commits a reviewable checkpoint when the slice passes.

Mew must not weaken tests, widen tolerances, or rewrite the contract solely to obtain a passing result.

### 9.8 Differential verification

Mew runs baseline and candidate against the same approved scenarios. Depending on the system, comparison may include:

- exact structured output;
- normalized text or ordering;
- numerical output within tolerance;
- HTTP status, body, headers, and error semantics;
- DOM, accessibility tree, navigation, and browser state;
- visual comparison within approved regions and thresholds;
- latency, throughput, memory, CPU, and startup time;
- side effects and persisted state.

A result is pass, fail, accepted deviation, or inconclusive. Missing data and unavailable environments are never reported as success.

### 9.9 Handoff

A completed run exports:

- candidate source or pull request;
- run manifest and source lock;
- approved contract;
- evidence log;
- fixtures and journeys;
- dependency and license decisions;
- parity and benchmark reports;
- accepted deviations and unresolved risks;
- reproduction and rollback instructions.

Mew does not merge or deploy by default.

## 10. Human approval model

Mew pauses at semantic and trust boundaries rather than every mechanical step.

Required approval points:

1. execution policy when commands, network, secrets, or external systems enter scope;
2. behavioral contract before implementation;
3. target architecture and non-obvious dependency substitutions;
4. any contract amendment or accepted deviation;
5. destructive or externally visible action;
6. final handoff, merge, publish, or deployment.

Routine reads, approved commands, tests, and writes inside the candidate workspace do not require repeated approval.

## 11. Functional requirements

### FR-1: durable runs

A migration run must have an ID, phase, status, owner, source lock, candidate workspace, policy, heartbeat, current checkpoint, and timestamps. It must resume after process restart without reconstructing state from chat history.

### FR-2: isolated workspaces

The baseline is immutable during observation and verification. Candidate writes occur in a separate worktree or workspace. Tools cannot escape the configured roots.

### FR-3: append-only event and evidence log

State-changing decisions and important observations are persisted before they are surfaced as complete. Logs remain readable after failure.

### FR-4: driver interface

The runtime exposes a common lifecycle for interaction drivers: prepare, observe, act, capture, reset, and close. Drivers return structured evidence rather than free-form prose alone.

### FR-5: browser and crawl observation

The browser driver uses real input events for interaction, captures console and network failures, records viewport and route state, and can attach screenshots or DOM/accessibility snapshots to evidence. A crawler or structured-content extractor may accelerate route and content discovery, but browser observations remain the authority for interactive behavior.

### FR-6: behavioral contract versioning

Contracts are machine-readable, diffable, and tied to the source revision. Approval records the exact version.

### FR-7: checkpoints and rollback

Mew can checkpoint the candidate before each implementation slice and return to a known state without modifying the baseline.

### FR-8: conformance runner

The same scenario can execute against baseline and candidate. Comparison rules are explicit and stored with the contract.

### FR-9: policy enforcement

Filesystem, network, command, secret, resource, and approval policy is enforced by the runtime where possible, not left only in the model prompt.

### FR-10: external integration

MCP and future APIs expose run creation, status, approvals, evidence, artifacts, pause, resume, and cancellation without embedding the Mew engine into every client.

## 12. Non-functional requirements

### Durability

A completed event, approval, artifact, or checkpoint survives process failure. Interrupted runs resume or fail with a clear recovery action.

### Reproducibility

Every run records source identity, environment, tool versions, commands, policy, fixtures, and hashes needed to repeat the result. Golden evaluations must pass from a clean environment.

### Safety

Untrusted repositories and observed applications are treated as hostile inputs. Mew defaults to least privilege, redacts secrets from artifacts, and isolates execution from the host and unrelated workspaces.

### Auditability

A reviewer can trace a contract claim and final verdict back to evidence without reading the full agent transcript.

### Extensibility

New drivers, comparison strategies, providers, and skill packs can be added without changing the core run model.

### Model independence

Durable state, policy, artifacts, and verification belong to Mew rather than a particular model provider.

### Efficiency

The system should avoid repeating reproduction and observation work when source, environment, policy, and artifact hashes are unchanged. Correctness and evidence quality take priority over minimizing tool calls.

## 13. Run artifacts

The initial on-disk shape is:

```text
.mew/runs/<run-id>/
├── manifest.json
├── source-lock.json
├── environment.json
├── events.jsonl
├── evidence.jsonl
├── repo-map.json
├── baseline/
├── fixtures/
├── behavioral-contract.yaml
├── migration-plan.yaml
├── decisions.jsonl
├── checkpoints/
├── parity-report.json
└── migration-report.md
```

Transcripts are useful for debugging but are not the primary interface between phases. Structured artifacts are the contract between the agent, runtime, user, and future implementations.

## 14. Safety, rights, and provenance

Mew must ask the operator to confirm that they are authorized to inspect and reproduce the target. Authorization does not automatically grant rights to every asset or dependency contained in it.

For external websites and applications:

- respect configured crawl boundaries and platform restrictions;
- do not attempt authentication bypass or hidden endpoint discovery outside the approved scope;
- do not collect unrelated personal data;
- record the provenance and license of reused content and assets;
- default to recreating behavior and structure rather than copying protected branding or media;
- require explicit approval before publishing a candidate that could impersonate the source.

For source repositories:

- preserve license and attribution requirements;
- record dependency licenses and selected replacements;
- keep secrets and private source out of portable evidence bundles;
- report incompatible licensing as a blocker.

## 15. Success metrics

### Product outcome

- percentage of completed runs whose candidate is accepted or merged;
- percentage of contract items that pass or have an approved deviation;
- time from intake to an approved contract;
- time from approved contract to first passing vertical slice;
- number of semantic corrections requested by the user after handoff.

### Evidence quality

- all critical claims have evidence;
- no fabricated or missing-source claims;
- all accepted deviations identify an approver and contract version;
- reviewers can reproduce the final verdict from artifacts.

### Reproducibility and safety

- source and toolchain are pinned for every source-based run;
- golden tasks pass in at least two clean runs;
- zero writes outside approved workspaces;
- zero secrets in committed artifacts;
- zero unapproved destructive or externally visible actions.

### Migration quality

- no unexplained contract delta;
- no custom protocol implementation when an approved official SDK or standard tool exists;
- every implementation slice can be reviewed and rolled back independently;
- benchmark claims include baseline, method, environment, and raw results.

## 16. Initial product scope

The first product slice supports source-available libraries, CLIs, and HTTP services with deterministic fixtures. It proves the full workflow from acquisition through parity report before adding broad UI or computer-use coverage.

The first browser slice supports public or locally hosted websites with finite routes and explicit interaction scope. It uses Playwright as an external driver and focuses on navigation, controls, forms, responsive states, console errors, network behavior, DOM/accessibility structure, and approved visual comparison.

General desktop control, mobile applications, authenticated third-party systems, hardware, production cutover, and open-ended black-box discovery follow only after the underlying run, evidence, contract, and verification model is reliable.

## 17. Risks

### The agent produces a convincing but incomplete contract

Mitigation: require evidence, confidence, unknowns, contract review, and coverage reporting. Hidden golden scenarios test whether the process finds behavior beyond the happy path.

### The rewrite optimizes the wrong problem

Mitigation: treat performance claims as hypotheses until profiling produces a baseline. Allow the correct recommendation to be "do not rewrite this component."

### Browser cloning becomes visual plagiarism

Mitigation: require authorization and provenance, separate interaction contracts from brand assets, and make target art direction an explicit contract rather than a vague prompt.

### Long runs drift or loop

Mitigation: durable phase budgets, heartbeats, checkpoints, repeated-action detection, cost limits, and pause states with a clear reason.

### Skills become an untestable collection of prompts

Mitigation: versioned structured inputs and outputs, golden tasks, schemas, validators, and promotion of stable mechanics into the runtime.

### Mew duplicates mature agent infrastructure before proving demand

Mitigation: use Hermes as the workflow laboratory. Port only mechanics that recur across successful runs and must survive crashes or enforce policy.

## 18. Architecture implications

The existing Rust engine, protocol, server, TUI, skills runtime, persistence, and Go MCP adapter remain useful foundations. The product shift requires several additions:

- a migration run model separate from chat sessions;
- per-run baseline and candidate workspaces instead of server-wide current-directory context;
- durable state and evidence stores;
- approval and artifact events in the protocol;
- sandbox and process lifecycle management;
- driver bridges for CLI, HTTP, fixtures, browser, and later computer use;
- conformance and benchmark runners;
- report and pull-request exporters.

The model reasons about ambiguity. Mew owns mechanics, policy, durable state, and proof.

## 19. Release criteria for the first complete loop

The first complete Mew release is ready when an operator can:

1. create a run from a source repository and evolution goal;
2. reproduce the baseline in an isolated workspace;
3. obtain an evidence-backed contract and approve it;
4. approve a sliced migration plan;
5. interrupt and resume implementation;
6. compare baseline and candidate on deterministic fixtures;
7. receive a reviewable pull request and artifact bundle;
8. reproduce the parity verdict from a clean environment;
9. complete the process without editing runtime state by hand.

Browser and computer-use capability is considered mature only when the same contract, evidence, approval, durability, and verification guarantees apply to externally observed applications.
