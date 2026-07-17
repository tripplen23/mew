# Mew product requirements document

**Status:** Draft 3

**Product:** Mew

**Product statement:** Mew is an AI agent harness that extracts the behavioral DNA of existing software, then evolves a new implementation under user-defined constraints.

## 1. Summary

Software prototypes are easier to create than ever, but many are built without a path to production. A prototype may prove the product idea while carrying the wrong runtime, framework, architecture, user experience, or operational model for its next stage.

Rewriting it is not primarily a code-generation problem. The hard part is discovering what the existing system actually does, separating intended behavior from incidental implementation details, deciding what must survive, and proving that the replacement still satisfies those decisions.

Mew treats this as a controlled evolution process. The canonical workflow and
its stage semantics are defined in [Product workflow](#9-product-workflow):

1. acquire and reproduce the source system;
2. observe its structure and behavior;
3. extract a behavioral contract backed by evidence;
4. let the user correct and approve that contract;
5. build an evolved implementation in reviewable slices;
6. compare the source and candidate until the approved contract passes;
7. hand off code, evidence, known deviations, and rollback information.

Mew is not limited to source-to-source migration. With browser or computer-use drivers, it can also study an accessible application from the outside and create a new product that preserves selected interactions while changing its design or implementation. For example:

> Study and reconstruct `https://nguyenducbinh.vercel.app/`, preserve its
> information and important interactions, then rebuild it with a stronger manga
> visual direction.

In that case Mew would inspect the public pages, exercise navigation and interactive states, collect the approved content and behavior, propose a new visual contract, then implement and verify the result. It should not copy protected assets, hidden data, credentials, or brand identity without authorization.

## 2. Product thesis

Mew does not clone code. It extracts behavioral DNA.

**Behavioral DNA** is the product concept: the smallest reviewable description
of what users and dependent systems can observe and what the owner intends to
preserve or deliberately change. A **behavioral contract** is the concrete,
versioned, evidence-linked artifact that records that DNA for one run. Section 9.4
defines its contents.

The contract is not inferred once and treated as truth. Mew presents it to the
user with evidence, uncertainty, and unanswered questions. The user remains the
authority on intent.

## 3. Problem

Mew addresses four connected problems: preserving behavior during migration,
learning behavior when only an observable product is available, sustaining the
resulting work over long-running agent sessions, and letting different builders
participate without surrendering control or exhausting one model context.

### 3.1 Prototype-to-production migration

A working prototype may depend on a language, framework, or deployment model that is unsuitable for production. Conventional rewrite projects fail because knowledge is scattered across source code, runtime behavior, tests, documentation, and the original author's assumptions.

A coding agent can generate replacement code, but compilation and unit tests do not prove parity. Without a behavioral baseline, the agent can silently remove edge cases, alter error behavior, or optimize the wrong bottleneck.

### 3.2 Observable product cloning and transformation

Sometimes the available input is an application rather than its repository. A user may own a site but no longer have a usable codebase, or may want to reproduce a public interaction pattern with a new visual identity.

Screenshots alone are insufficient. They omit routes, responsive states, keyboard behavior, loading and error states, forms, animations, and the relationships between actions. The agent must operate the product, record observations, distinguish facts from guesses, and obtain approval before implementation.

### 3.3 Long-running agent work

Migration and reconstruction runs can take hours or days. They cross network, filesystem, browser, process, and repository boundaries. A chat loop without durable state cannot safely resume after a crash, explain why a decision was made, or prove which source revision and environment produced an observation.

### 3.4 Builder control and context rot

Some operators want an agent to implement approved slices autonomously. Others
want Mew to observe and verify while they write every line. A single immortal
coding conversation serves neither group well: it couples proof to one builder,
accumulates stale assumptions, repeatedly injects irrelevant history, and makes
token cost grow with elapsed time rather than the current task.

Mew therefore separates a durable orchestrator from short-lived builders. The
orchestrator owns state, evidence, contracts, tasks, policy, and proof. A human or
agent builder receives a bounded task packet and can be replaced at a checkpoint
without reconstructing the run from transcript history.

## 4. Goals

Mew must:

1. reproduce an existing system in an isolated workspace when source is available;
2. observe static structure and runtime behavior through interchangeable drivers
   and analyzers;
3. produce a behavioral contract whose important claims point to evidence;
4. expose uncertainty and request user decisions at semantic boundaries;
5. create a migration or reconstruction plan in independently reviewable slices;
6. implement the approved plan without modifying the baseline;
7. compare baseline and candidate behavior with explicit tolerances;
8. survive interruption and resume from durable state;
9. emit an auditable artifact bundle and a reviewable code change;
10. support external agents through stable integration surfaces such as MCP;
11. install, launch, diagnose, update, and remove Mew through one stable public
    command;
12. support manual, supervised, and delegated implementation without changing
    the evidence or verification model;
13. keep long-running work within explicit context and cost budgets by dividing
    plans into bounded tasks and resuming fresh workers from durable context
    checkpoints.

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

## 6. Target users and roles

Target users are:

- product engineers changing the runtime, framework, architecture, or deployment
  model of a successful prototype without losing behavior;
- teams that need an evidence-backed map before modernizing an unfamiliar system;
- designers or developers reconstructing a product they are authorized to use,
  with selected behavior preserved and a deliberate new design direction;
- agent operators who need long-running work to be inspectable, resumable,
  policy-bound, and safe to leave unattended;
- hands-on engineers who want Mew to verify human-authored changes without
  granting an agent write access;
- builders who prefer supervised or delegated implementation inside explicit
  scope, budget, and approval boundaries.

Mew distinguishes four run roles:

| Role | Responsibility |
| --- | --- |
| Requester | Defines the desired evolution and product intent. |
| Operator | Confirms authorization, configures policy, and manages execution. |
| Reviewer | Audits evidence, contracts, plans, and verification results. |
| Approver | Accepts gated decisions, deviations, and final handoff. |

One person may hold all four roles in the initial release. The run manifest
records which identity acted in each role so team separation can be added later.

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

**Expected behavior:** Mew crawls pages within configured boundaries, uses a real
browser to exercise links and controls, records responsive and interactive states,
inventories reusable content, and proposes a visual and behavioral contract. After
approval it creates a new implementation and runs browser-level comparisons
against the contract.

**Expected evolution:** the candidate should not be a pixel-for-pixel copy unless
the user owns the source product or has equivalent reproduction rights and
explicitly requests that output. The approved art direction is part of the target
contract.

### 7.4 Black-box service reconstruction

**Input:** an authorized CLI, API, local application, or test environment without source code.

**Expected behavior:** Mew records observable inputs, outputs, errors, timing, and state transitions. It labels unobserved behavior as unknown and avoids claims about hidden implementation.

## 8. Concepts

### 8.1 Source system

The repository, URL, binary, API, or application being studied. A run must
identify the exact source state. For versioned source this is an immutable
revision. For a live external system it is the strongest available identity,
such as a release version, deployment identifier, response fingerprint, and
capture timestamp; any remaining ambiguity is recorded.

### 8.2 Candidate

The evolved implementation written by a human, an external coding agent, or a
Mew-managed builder. The candidate always lives in a separate writable workspace
from the baseline. Verification guarantees do not depend on who authored it.

### 8.3 Behavioral contract

A versioned, user-approved set of invariants, tolerances, journeys, performance targets, and deliberate changes.

### 8.4 Evidence

A stable reference to a repository location, command output, runtime trace,
browser observation, screenshot, network response, fixture result, benchmark,
or official document. Evidence records its source identity and capture time.
File-backed and captured artifacts include a content hash; ephemeral observations
include the driver, environment, and replay information needed to reproduce them.

### 8.5 Interaction driver

An adapter that lets Mew observe or exercise a system. Initial drivers are CLI, HTTP, fixtures, and browser. Computer-use and hardware drivers follow later.

### 8.6 Migration run

The durable unit of work. It owns policy, workspaces, state, artifacts, decisions, checkpoints, and results independently from a chat session.

### 8.7 Important claim

A claim is important when it defines a contract invariant or tolerance, affects
user-visible or dependent-system behavior, is referenced by an implementation
slice, changes a safety or rights decision, or has uncertain provenance. Important
claims require evidence or an explicit `unknown` classification.

### 8.8 Confidence

Confidence is a qualitative label: high, medium, or low. It reflects evidence
strength and agreement, not model certainty. Repeated direct observation is
stronger than a single observation; user-provided intent and static inference are
recorded as different evidence classes rather than collapsed into one score.

### 8.9 Authorization declaration

A recorded statement in the run manifest identifying the operator's relationship
to the source, such as owner, licensee, or authorized agent, and the basis for the
requested access and reproduction. It is an audit record, not a substitute for
legal rights or applicable platform rules.

### 8.10 Visual analyzer

A provider-neutral capability that interprets screenshot, crop, or frame artifacts
captured by an interaction driver. Capture and interpretation are separate: the
browser or computer-use driver records the state, while a configured multimodal
provider or external vision service produces structured visual observations. A
visual observation records the image hash, inspected region, analysis rubric,
provider and model identity, confidence, and resulting claims.

### 8.11 Builder and autonomy mode

A builder is the replaceable human or agent that edits the candidate for one
approved task. Mew supports three implementation modes on the same run model:

- `manual`: Mew does not ask an agent to edit the candidate; it observes and
  verifies human-authored changes;
- `supervised`: a builder proposes a bounded patch, Mew verifies it, and the
  operator reviews the semantic checkpoint before the next task;
- `delegated`: a builder may iterate within one approved task until its completion,
  stop, or budget conditions fire.

`supervised` is the default. Autonomy never permits a builder to approve its own
contract amendment, accepted deviation, destructive action, or final handoff.

### 8.12 Task packet and context checkpoint

A task packet is the immutable, machine-readable assignment for one bounded unit
of work. It includes the task ID, goal, dependencies, relevant contract items,
input artifact hashes, allowed workspace roots, completion criteria, validation
commands, budgets, stop conditions, and expected outputs.

A context checkpoint is a compact, structured handoff between workers. It records
decisions, discoveries, changed files, command and test outcomes, unresolved
questions, current failure classification, artifact references, and the next task.
It is derived from durable run data and never replaces primary evidence, source
artifacts, or the approved contract. The full transcript may be retained for
debugging, but a fresh worker must be able to continue from the task packet and
context checkpoint without loading that transcript.

## 9. Product workflow

### 9.1 Intake

Mew accepts:

- source locator and revision when available;
- the user's desired evolution;
- scope and exclusions;
- target constraints;
- the authorization declaration and provenance information;
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

Every important claim, as defined in section 8.7, is classified as observed,
inferred, user-provided, or unknown.

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
- evidence references and confidence using the labels defined in section 8.8.

### 9.5 User review

The user can edit, accept, reject, or defer each important contract item. A
deferred item remains unknown and blocks any implementation slice that depends on
it. Independent slices may continue. Mew must never convert an assumption into an
approved invariant without showing the transition.

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

Before implementation, each approved slice is expanded into a dependency-aware
task graph. Every runnable task has one independently testable outcome and a task
packet as defined in section 8.12. Tasks that cannot fit their context, cost, or
deadline budget must be split before a builder starts.

### 9.7 Implementation loop

The operator selects `manual`, `supervised`, or `delegated` mode for each slice.
Mode changes are recorded as run decisions; they do not change the approved
contract or verification criteria. For each runnable task Mew:

1. materializes the task packet and a minimal context pack from durable artifacts;
2. creates a candidate checkpoint;
3. writes tests or fixtures for the relevant contract;
4. lets the selected human or agent builder make the smallest implementation
   change permitted by the mode;
5. builds and runs focused validation;
6. compares baseline and candidate;
7. classifies failures;
8. repeats within budget or pauses for a semantic decision;
9. persists a context checkpoint before rotating workers or compacting context;
10. commits a reviewable checkpoint when the task passes.

No worker is assumed to retain the entire run. A worker normally ends at a task
boundary. When context usage reaches the configured headroom threshold, Mew
persists the checkpoint and starts a fresh worker rather than allowing emergency
truncation to silently discard state.

Mew may use shape-specific, demonstrably lossless representations for repetitive
tool output when the original artifact is retained by hash. It must not represent
generic compression or cache markers as equivalent context unless retrieval is
enforced and the representation preserves distinctions relevant to the task.

Mew must not weaken tests, widen tolerances, or rewrite the contract solely to obtain a passing result.

### 9.8 Differential verification

Mew runs baseline and candidate against the same approved scenarios. Depending on the system, comparison may include:

- exact structured output;
- normalized text or ordering;
- numerical output within tolerance;
- HTTP status, body, headers, and error semantics;
- DOM, accessibility tree, navigation, and browser state;
- visual comparison within approved regions and thresholds;
- semantic visual observations against an approved rubric;
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
A semantic boundary is a decision whose correct answer depends on product intent,
not mechanical fact. Examples include whether changed error behavior is acceptable,
whether a dependency substitution preserves intent, and whether an unobserved area
blocks a slice.

Required approval points:

1. execution policy when commands, network, secrets, or external systems enter scope;
2. behavioral contract before implementation;
3. target architecture and non-obvious dependency substitutions;
4. any contract amendment or accepted deviation;
5. destructive or externally visible action, including merge, deploy, publish,
   purchase, production cutover, irreversible filesystem mutation, or an external
   API write outside the approved candidate workflow;
6. final handoff, merge, publish, or deployment.

Routine reads, approved commands, tests, and writes inside the candidate workspace do not require repeated approval.

## 11. Functional requirements

### FR-1: durable runs

A migration run must have an ID, phase, status, owner, source lock, candidate
workspace, policy, heartbeat, current checkpoint, and timestamps. It must resume
after process restart without reconstructing state from chat history.

### FR-2: isolated workspaces

The baseline is immutable during observation and verification. Candidate writes
occur in a separate worktree or workspace. Tools cannot escape the configured
roots.

### FR-3: append-only event and evidence log

State-changing decisions and important observations are persisted before they
are surfaced as complete. Logs remain readable after failure.

### FR-4: driver interface

The runtime exposes a common lifecycle for interaction drivers: prepare,
observe, act, capture, reset, and close. Drivers return structured evidence
rather than free-form prose alone.

### FR-5: browser and crawl observation

The browser driver uses real input events, captures console and network failures,
records viewport and route state, and attaches screenshots or DOM/accessibility
snapshots to evidence. A crawler or structured-content extractor may accelerate
route and content discovery, but browser observations remain the authority for
interactive behavior. Automated discovery must remain inside the approved
allowlist and honor applicable robots directives and platform restrictions.

### FR-6: behavioral contract versioning

Contracts are machine-readable, diffable, and tied to the source identity.
Approval records the exact version.

### FR-7: contract review and approval

Mew must provide a structured review flow where authorized roles can inspect
evidence, edit, accept, reject, or defer contract items, and record approval
against an immutable contract version. Dependent slices cannot start while a
required item is rejected, deferred, or unknown.

### FR-8: evolution plan generation

Mew must produce a machine-readable plan divided into independently reviewable
slices. Each slice identifies its contract items, scope, dependency decisions,
validation, risks, deliberate deviations, checkpoint, and rollback action.

### FR-9: checkpoints and rollback

Mew can checkpoint the candidate before each implementation slice and return to
a known state without modifying the baseline.

### FR-10: conformance runner and parity report

The same scenario can execute against baseline and candidate. Comparison rules
are explicit and stored with the contract. Every run emits a structured parity
report that maps each contract item to pass, fail, accepted deviation, or
inconclusive, with supporting evidence.

### FR-11: policy enforcement

Filesystem, network, command, secret, resource, and approval policy is enforced
by the runtime where possible, not left only in the model prompt.

### FR-12: external integration

MCP and future APIs expose run creation, status, approvals, evidence, artifacts,
pause, resume, and cancellation without embedding the Mew engine into every
client.

### FR-13: multimodal visual analysis

Mew can submit approved image artifacts to a configured multimodal provider or
external vision service and use an ordered fallback when the primary provider is
unavailable. Requests and results are provider-neutral structured records. Each
result includes the image and region hashes, analysis rubric, provider, model,
request version, confidence, cost metadata when available, and resulting claims.

Vision output is classified as model inference. It cannot overrule a deterministic
failure or become the sole basis for a parity pass unless the user approved a
visual rubric for that contract item. If no vision provider is configured, Mew
continues deterministic browser checks and blocks or requests human review for
items that require semantic visual judgment.

### FR-14: installable operator surface

Each supported release provides a documented one-command installation path and a
stable `mew` executable. Running `mew` without arguments opens the primary local
interface and starts or connects to the required local runtime without asking the
user to manage internal server binaries.

The public CLI includes setup, configuration, version, status, diagnostics,
update, and uninstall flows. Internal processes such as the server and MCP bridge
may remain separate components, but their names, locations, and lifecycle are not
part of the normal user workflow. CLI and configuration changes that affect
automation are versioned or migrated explicitly.

### FR-15: builder modes and interface

Each implementation slice selects `manual`, `supervised`, or `delegated` mode.
The runtime uses the same task, policy, evidence, checkpoint, and verification
contracts in every mode. A builder interface can prepare a task, propose or report
changes, request clarification, report validation, and yield a checkpoint without
granting the builder authority to approve semantic decisions or final handoff.

Manual mode must support externally authored candidate commits without agent write
access. Supervised mode stops at configured semantic checkpoints. Delegated mode
may iterate only inside the current task's roots, budgets, and stop conditions.

### FR-16: bounded tasks and context lifecycle

An approved plan is executable only after it is represented as bounded tasks with
explicit dependencies, inputs, outputs, contract coverage, completion criteria,
validation, budgets, and stop conditions. The runtime creates a minimal context
pack for each task and persists a schema-valid context checkpoint before worker
rotation, context compaction, pause, or interruption.

A fresh worker must be able to continue from durable artifacts without loading the
prior transcript. Summaries and dense representations cannot overwrite primary
evidence or erase identifiers, hashes, decisions, failures, unknowns, or unresolved
questions. Any lossy reduction is labeled and cannot be the sole source for a
contract claim or parity verdict.

### Goal-to-requirement traceability

| Product goal | Primary requirements |
| --- | --- |
| Reproduce an existing system safely | FR-2, FR-11, NFR-2, NFR-3 |
| Observe static and runtime behavior | FR-4, FR-5, FR-13 |
| Produce an evidence-backed contract | FR-3, FR-6, FR-7, NFR-4 |
| Expose uncertainty and request decisions | FR-7 |
| Create an independently reviewable plan | FR-8 |
| Implement without modifying the baseline | FR-2, FR-9, FR-11 |
| Compare baseline and candidate | FR-10 |
| Survive interruption and resume | FR-1, NFR-1, NFR-8 |
| Emit auditable artifacts and code changes | FR-3, FR-10, NFR-4 |
| Support external agents | FR-12, NFR-5, NFR-6 |
| Install and operate Mew as a product | FR-14, NFR-9 |
| Let humans and agents build under one proof model | FR-15, NFR-5, NFR-6 |
| Bound context growth and resume fresh workers | FR-1, FR-16, NFR-1, NFR-10 |

## 12. Non-functional requirements

### NFR-1: durability

A completed event, approval, artifact, or checkpoint survives process failure.
Interrupted runs resume or fail with a clear recovery action.

### NFR-2: reproducibility

Every run records source identity, environment, tool versions, commands, policy,
fixtures, and hashes needed to repeat the result. Golden evaluations must pass
from a clean environment.

### NFR-3: safety and privacy

Untrusted repositories and observed applications are treated as hostile inputs.
Mew defaults to least privilege, redacts secrets and unrelated personal data from
portable artifacts, and isolates execution from the host and unrelated workspaces.

### NFR-4: auditability

A reviewer can trace a contract claim and final verdict back to evidence without
reading the full agent transcript.

### NFR-5: extensibility

New drivers, comparison strategies, providers, and skill packs can be added
without changing the core run model.

### NFR-6: model independence

Durable state, policy, artifacts, and verification belong to Mew rather than a
particular text or multimodal model provider. Provider fallback must not change
the artifact schema or silently change an approved visual rubric.

### NFR-7: efficiency

The system avoids repeating reproduction and observation work when source,
environment, policy, and artifact hashes are unchanged. Correctness and evidence
quality take priority over minimizing tool calls.

### NFR-8: resource and cost control

Every run can set disk, process, network, token or provider-cost, rate-limit, and
deadline budgets. Reaching a budget stops or pauses work predictably, preserves a
resumable state, and records the limiting resource.

### NFR-9: release integrity

Release artifacts include checksums, signatures, a software bill of materials,
and build provenance. Install and update flows verify artifacts before replacing
an existing installation, preserve user data by default, and provide a documented
rollback path. Secrets never appear in command-line arguments, installer logs, or
release artifacts.

### NFR-10: context efficiency and information integrity

Token and context use are measured per task, phase, model, and artifact class.
Mew reserves configurable context headroom and rotates workers before provider
limits force uncontrolled truncation. Context reuse is selective: unchanged
artifacts are referenced by identity, and only task-relevant excerpts or verified
shape-specific dense forms are injected.

Optimization claims require a baseline, representative corpus, answer-equivalence
or task-outcome checks, and total-token accounting that includes retrieval. A
smaller prompt is not a success if it increases retries, omits relevant state, or
changes the answer. Correctness, evidence integrity, and reproducibility take
priority over compression ratio.

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
├── tasks/
├── context-checkpoints/
├── checkpoints/
├── parity-report.json
└── migration-report.md
```

Transcripts are useful for debugging but are not the primary interface between phases. Structured artifacts are the contract between the agent, runtime, user, and future implementations.

## 14. Safety, rights, and provenance

Mew must require the authorization declaration defined in section 8.9 before it
inspects or reproduces a target. The declaration does not automatically grant
rights to every asset or dependency contained in that target.

For external websites and applications:

- respect configured crawl boundaries and platform restrictions;
- do not attempt authentication bypass or hidden endpoint discovery outside the approved scope;
- do not collect unrelated personal data;
- apply configured redaction before screenshots or crops leave the local trust
  boundary for a remote vision provider;
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

These are tracked outcome metrics, not first-release gates. M0 establishes a
baseline from at least three golden tasks; each subsequent release sets its target
against that baseline. Lower time and fewer post-handoff semantic corrections are
better, but not at the expense of evidence quality or honest blockers.

### Evidence quality

- all important claims, as defined in section 8.7, have evidence or are explicitly
  classified as unknown;
- zero claims presented as fact without evidence or user attribution;
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

The first browser slice supports public or locally hosted websites with finite routes and explicit interaction scope. It uses Playwright as an external driver and focuses on navigation, controls, forms, responsive states, console errors, network behavior, DOM/accessibility structure, and approved visual comparison. When a multimodal provider is configured, a separate visual analyzer can assess hierarchy, composition, art direction, and visual defects from captured artifacts.

General desktop control, mobile applications, authenticated third-party systems, hardware, production cutover, and open-ended black-box discovery follow only after the underlying run, evidence, contract, and verification model is reliable.

The implementation sequence and milestone ownership for these slices are defined
in [`PHASES.md`](../PHASES.md). M0 through M7 deliver the first complete
source-available loop; M8 applies the same guarantees to browser-observed targets.

## 17. Risks

### The agent produces a convincing but incomplete contract

Mitigation: require evidence, confidence, unknowns, contract review, and coverage reporting. Hidden golden scenarios test whether the process finds behavior beyond the happy path.

### The rewrite optimizes the wrong problem

Mitigation: treat performance claims as hypotheses until profiling produces a baseline. Allow the correct recommendation to be "do not rewrite this component."

### Browser cloning becomes visual plagiarism

Mitigation: require authorization and provenance, separate interaction contracts from brand assets, and make target art direction an explicit contract rather than a vague prompt.

### Long runs drift or loop

Mitigation: durable phase and task budgets, heartbeats, checkpoints,
repeated-action detection, cost limits, and pause states with a clear reason.
Workers receive bounded task packets instead of the full run transcript. Before
context rotation or compaction, Mew persists a context checkpoint and verifies its
artifact references; a fresh worker resumes from that checkpoint.

### Context compression hides required information or increases total cost

Mitigation: primary evidence and approved artifacts are never replaced by a
summary. Prefer task-scoped retrieval and shape-specific lossless densification.
Treat generic cache markers and lossy summaries as optimization hypotheses, count
retrieval and retry tokens, and verify task outcomes on a representative corpus
before enabling an optimization by default.

### Autonomy removes useful operator control

Mitigation: manual, supervised, and delegated builders share one run and proof
model. Supervised is the default, autonomy is selected per slice, and semantic
approvals remain external to the builder.

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
- bounded task graphs, task packets, context packs, and context checkpoints;
- a replaceable builder interface with manual, supervised, and delegated modes;
- sandbox and process lifecycle management;
- driver bridges for CLI, HTTP, fixtures, browser, and later computer use;
- a provider-neutral visual-analyzer bridge with screenshot preprocessing,
  redaction, artifact hashing, caching, and ordered multimodal fallback;
- a stable `mew` command and release manager that hides internal process layout;
- conformance and benchmark runners;
- report and pull-request exporters.

The model reasons about ambiguity. Mew owns mechanics, policy, durable state, and proof.

## 19. Release criteria for the first complete loop

The first complete Mew release, corresponding to M0 through M7 in
[`PHASES.md`](../PHASES.md), is ready when an operator can:

1. create a run from a source repository and evolution goal;
2. reproduce the baseline in an isolated workspace;
3. obtain an evidence-backed contract and approve it;
4. approve a sliced migration plan;
5. choose manual, supervised, or delegated implementation per slice;
6. inspect the bounded task graph and its completion criteria;
7. rotate to a fresh worker and continue from a context checkpoint without the
   prior transcript;
8. interrupt and resume implementation;
9. compare baseline and candidate on deterministic fixtures;
10. receive a reviewable pull request and artifact bundle;
11. reproduce the parity verdict from a clean environment;
12. complete the process without editing runtime state by hand.

Browser and computer-use capability is considered mature only when the same contract, evidence, approval, durability, and verification guarantees apply to externally observed applications.
