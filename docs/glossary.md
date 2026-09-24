# Shared vocabulary

**Canonical terms, 2026-09-12.** The other canonical documents use these words in exactly these senses. Where a term names existing code, the source is cited as evidence; where it names a PLANNED contract, it is labelled. Status words (IMPLEMENTED, PROTOTYPED, PLANNED, RESEARCHING, HYPOTHESIS) are defined in [MASTER_SPEC](../MASTER_SPEC.md) and are not repeated here.

Three words carried more than one meaning in the older documents and are disambiguated below on purpose: **policy**, **checkpoint** and **context**. Using them loosely is how a representation choice gets mistaken for an authorization, and how a discarded prompt gets mistaken for a restorable workspace.

## Deployment and model

| Term | Meaning |
|---|---|
| **Deployment** | One servable configuration, not a model name: weights artifact/digest, quantization and KV-cache format, backend build, tokenizer/template, adapter version, loaded context window and runtime options, on identified hardware. Two quantizations of one weight family are two deployments. |
| **Deployment fingerprint** | The recorded identity of a deployment, including fields observed as unknown. An unknown field constrains the claim; it is never treated as equal to another deployment's value. A locally derived variant ID is not proof of the weight bytes. |
| **Backend** | The local inference engine a deployment runs on, behind the `InferenceBackend` contract. IMPLEMENTED: PWR's own MLX engine ([pwr-mlx](../crates/pwr-mlx/src/lib.rs), a Python sidecar over mlx-lm) and llama.cpp for GGUF ([pwr-llama](../crates/pwr-llama/src/lib.rs), a managed `llama-server`, limited). Ollama and LM Studio were backends until 2026-09-19; the last revision with them is tagged `last-with-http-backends`. |
| **Adapter** | The code translating canonical requests into a deployment's wire and prompt conventions, IMPLEMENTED in [pwr-compat](../crates/pwr-compat/src/lib.rs). An adapter cannot supply a tool skill the model was never trained for. |
| **Probe** | A measurement run against a deployment. Discovery probes read declared facts; protocol probes test streams, tool names/arguments, abstention and cancellation; task-like probes vary catalog size, dependency depth, stale paths and evidence position; performance probes separate load, cold prefill, cached prefill, decode and pressure. |
| **Capability evidence** | Versioned probe observations with sample counts, uncertainty and provenance. Distinguishes `supported`, `observed reliable in these trials`, `unsupported` and `unknown`. A short successful probe is not certification. |
| **Calibration** | The measurement of an operating point (granted context, occupancy, complete-stream and pressure behaviour) for a deployment, IMPLEMENTED as the v5 protocol in the orchestrator. It is not evidence of useful coding context. |

## Context

| Term | Meaning |
|---|---|
| **Declared context** | The window a backend or model card advertises. |
| **Allocated context** | The window actually granted when the model was loaded. |
| **Delivered context** | The tokens a given turn actually placed in the prompt. |
| **Useful context** | The occupancy at which task performance still holds, established only by task-like tests. Needle recall authorizes its own tested condition and nothing more. |
| **Context engine** | The component that assembles a turn from working state, evidence and the recent window under separate section budgets. It owns no canonical facts. EVOLVE target of [context.rs](../crates/pwr-orchestrator/src/context.rs). |
| **Compaction** | Reducing the model-visible context. It discards a disposable view, never an authoritative record. A compaction count is not by itself evidence of a loop. |
| **Rehydration** | Fetching the retained bytes behind a selected view. A hash whose bytes were discarded cannot be rehydrated, and must not be described as retrievable memory. |

## Action and policy

| Term | Meaning |
|---|---|
| **Canonical action** | The tool-independent intent and effect: which tool, which arguments, which effect class. It stays fixed while its presentation varies. |
| **Action representation** | How a canonical action is offered and accepted — native tool schema or constrained JSON, full or grouped catalog, `replace_text` or multi-hunk patch, compact error fields or bounded logs. This is the object of H1 and H6. |
| **Behavior policy** | The frozen, versioned set of representation and budget choices in force for a session: schema form, catalog shape, ordering, output reserve, planning and retry settings. **It grants no authority.** Selecting one adaptively is RESEARCHING. |
| **Access policy** | Authorization: workspace trust, grants, protected paths, scope of one action or a session, policy epoch and denial evidence. IMPLEMENTED as `ToolPolicy`, approvals and the macOS seatbelt profile. Tool output is data and never widens it. |
| **Policy epoch** | The version counter of the access policy in force, recorded with each decision so a grant, expiry or revocation is attributable. |
| **Deterministic normalization** | An unambiguous, versioned repair of a proposal — a recognized tool alias, a JSON argument delivered as a string. Both original and normalized forms are recorded. It may never invent a missing argument, choose between ambiguous matches, or turn a read into a write. |
| **Autonomy profile** (PLANNED) | A named summary of concrete grants — path roots, executables, network scope, destructive and publication decisions — chosen by the user. It is a set of scopes, not a level of trust in the model. |

## State, evidence and memory

| Term | Meaning |
|---|---|
| **Session** | One continuous user-facing engineering conversation with its objective revisions, budgets and authorizations. **The conversation is the loop**: it is not a view onto a separate batch run. |
| **Turn** | One model exchange within a session: assembled prompt, model output, and any proposed actions. |
| **Task** | A unit of work with an objective, acceptance criteria and a terminal result. A session may carry several. |
| **Working state** | The authoritative, externally represented record: objective and constraints, plan dependencies, claims and assumptions, evidence links, checks, files touched, pending questions. The model proposes updates; tool and check results establish facts. |
| **Evidence** | An observation with provenance — a source event or artifact, a file hash and range where applicable, and its invalidation conditions. A model claim is not evidence. |
| **Artifact** | Retained bytes with an ID, byte ranges and a retention policy: source text, command output, reports, traces. Stored before it is reduced. |
| **Stale evidence** | An excerpt or claim whose source has since changed. Edits, branch switches and external modifications invalidate it; resuming on an old hash described as current is the failure this term names. |
| **Memory** | Validated repository facts and optional reusable procedures held outside the active task, each scoped and invalidatable. Distinct from working state. Cross-task promotion must earn evidence. |
| **Repository index** | File inventory, content hashes, shallow declarations and imports, and heuristic neighbours, IMPLEMENTED in [pwr-repo](../crates/pwr-repo/src/lib.rs). It is not a resolved call graph, and does not claim to be one. |

## Verification and completion

| Term | Meaning |
|---|---|
| **Check** | A discovered, independently executed repository command with a recorded outcome. Discovery, environment failure, flake, code failure and unknown are separate classes. |
| **Baseline contract** | The set of checks already failing before the agent acted, agreed at admission. It bounds what a later comparison can honestly claim. |
| **Acceptance** | Task-appropriate independent evidence that the objective was met. A model-authored passing test is not independent evidence. |
| **Terminal state** (PLANNED) | How a task ended: `completed`, `blocked`, `interrupted`, `budget_exhausted`, `failed`. |
| **Evidence outcome** (PLANNED) | What was actually established: `checks_passed`, `baseline_preserved`, `answer_delivered`, `not_checked`. Reported alongside the terminal state, never merged into it. |
| **False acceptance** | The agent or runtime reported acceptance while hidden checks or the task rubric fail. Counted separately from failure. |
| **No new failures** | The condition the current chat wrapper actually tests. It is weaker than *all checks passed*, and R1 exists to stop reporting it as the latter. |

## Recovery and continuation

| Term | Meaning |
|---|---|
| **Failure class** | The observed reason an action did not achieve its effect: malformed call, absent path, stale hash, failed patch, compiler failure, transient provider error, context overflow, timeout, repeated unchanged effect. Recovery is a bounded transition keyed to the class, not another round of prompting. |
| **Progress** | A change in check state or artifact novelty. A new timestamp, a repeated read or an edit/revert cycle is not progress. |
| **Workspace checkpoint** | A durable binding of objective, plan graph, pending action state, checks, budgets, deployment and access policy, and workspace hashes. Restoring one preserves pre-existing dirty work and never silently resets a branch. |
| **Context checkpoint** | A saved prompt-assembly state. Distinct from a workspace checkpoint, and never a substitute for one. |
| **Replay** | Rebuilding a projection from persisted events, IMPLEMENTED for reporting and diagnosis. **Replay is not resumed execution**; resumption additionally requires effect reconciliation. |
| **Effect reconciliation** | Determining, after an interruption, whether an ambiguous write, process or network operation actually happened, before anything is retried. There is no general exactly-once guarantee across arbitrary external tools. |

## Experiments

| Term | Meaning |
|---|---|
| **Trial** | One immutable task, workspace, environment, deployment, harness condition, budget and repetition. Repeated seeds are repetitions, not new repositories. |
| **Condition / treatment** | The preregistered harness factor under test. Any undeclared difference between arms rejects the causal pairing. |
| **B0 / B1 / B2** | The three baselines in [evaluation](evaluation.md): B0 a conventional loop, B1 frozen current PWR, B2 the strong fixed policy with a staged localization → repair → validation workflow. H6 must beat B2, not merely B0. |
| **`verifier_supplied`** | The current evaluation mode in which the corpus provides the visible verifier. Named explicitly because it does not exercise product check discovery, and is therefore not product equivalence. |
| **All-attempt denominator** | The primary estimand counts every assigned trial, including provider failures, timeouts and interrupted work. Conditional model-served views are secondary and labelled. |
| **Development data / holdout** | Tasks used to choose and fit a policy, versus repository-disjoint tasks reserved for confirmation. Fitting a selector on holdout outcomes voids the result. |
| **Promotion gate** | The frozen practical threshold, its uncertainty bound, zero observed new scope/policy/false-acceptance violations, and cost within budget. Inconclusive never promotes a default. |
| **Negative result** | A deliverable. A rejected hypothesis retires the intervention and keeps the simpler harness; it does not become a smaller claim about the same feature. |

## Names

**PWR** is the project; `pwr` is the binary and the crate prefix. The name states the thesis: the interesting question is how much engineering capability a harness can extract from a model that a frontier lab would call inadequate, on hardware a developer already owns. It is not a claim that poor models equal rich ones.
