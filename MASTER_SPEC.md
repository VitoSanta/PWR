# PWR: project and research contract

**Authoritative direction, 2026-09-12.** This specification replaces the previous product thesis. It describes the destination and constraints on future work; it does not assert that the destination is implemented. Current behavior is recorded in the [audit](docs/adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md). Historical text is available at Git commit `f7ad8ca6135bc88517f0a43bc333898c935534bc`; dated experiment artifacts remain in place.

## Definition and initial user

PWR is a local agent harness research platform whose first product is an autonomous software-engineering session. It combines model inference with tools, repository evidence, external task state, verification and recovery, and tests which representations and policies let resource-constrained local models complete more real work. A developer supplies a repository, objective and authorized access; the agent investigates, acts, preserves relevant evidence, and returns changes and an account of what was actually checked. Vision and other machine interaction enter through capability contracts when supported.

**The destination is an unrestricted coding agent, driven from a conversation.** It should be able to reach anything on the machine it runs on and anything on the network its user authorizes — files anywhere the user allows, processes, shells, toolchains, Git, local applications, services, a browser, the open internet — and it should be usable by saying what is wanted, in the way Claude Code and Codex are usable. Nothing in this contract restricts the capability itself. What it restricts is what may be *claimed* as measured, and the order in which capability is admitted. A capability that is held back here is held back for want of evidence or of a policy that can express its scope, never because reaching the machine or the internet is thought to be out of bounds. The authorization model exists to make that reach explicit and observable, not to keep the agent small.

**Ease of use is an acceptance requirement, not a finishing touch.** The conversation is the product surface: a capability that can only be driven by remembering a flag, editing a configuration file or reading a raw event log is not finished, whatever its internals can do. Simple to ask for and fully able to act are the same requirement stated twice.

The initial user is a developer or agent researcher on a single-user local workstation, initially the existing Apple-silicon/macOS laboratory, running one model on PWR's own engine (MLX; llama.cpp for GGUF and Windows in progress -- Ollama and LM Studio were used until 2026-09-19). The research runs on that laboratory; the product the research serves is a desktop app for macOS and Windows. The initial workload is unfamiliar-repository diagnosis, bug repair, features, refactors and migrations spanning multiple files. Dependency changes, frontend/backend development, local services, Git and implementation research belong to this vertical. Parameter count is an experimental dimension; fitting a particular model or loading an entire repository into context is not the goal.

## Problem and thesis

A valid OpenAI-compatible response does not establish reliable tool selection, edit semantics, working-state retention or autonomous completion. Transport, model, context construction, environment and completion-contract failures can produce the same visible symptom. PWR must isolate these causes before compensating for them.

**Primary HYPOTHESIS:** within a fixed local deployment and resource budget, a small policy selected from measured behavior can improve verified engineering completion or reduce its total cost over the best fixed policy selected on separate development tasks. The policy adapts action representation and evidence delivery; deterministic state and verification remove avoidable bookkeeping and repeated mistakes. Promotion requires held-out benefit after paying for probes, retries, cache misses and additional inference. If the policy cannot outperform a fixed baseline, PWR should keep the simpler baseline.

Repo maps, adaptive edit formats, memory, compaction, structured tools, checkpoints and modular loops are prior art. The possible contribution is a reproducible account of **which intervention helps which local deployment and task regime**, and a compact implementation that transfers across unseen repositories. There is no novelty, state-of-the-art, or cloud-parity claim yet. The [research synthesis](docs/local-agent-research.md) records sources and competing explanations.

## Minimum research core

| Class | Systems | Why they belong |
|---|---|---|
| **CORE RESEARCH** | Behavior-conditioned action/context policy; evidence-linked task state; effect-based recovery | Test reduced invalid actions, lost/stale evidence and repeat failures with same-model ablations. |
| **NECESSARY INFRASTRUCTURE** | Shared session runtime, provider adapters, policy executor, hash-guarded edits, process ownership, repository checks, local artifact/event store, paired evaluator | Make experiments fair and engineering actions usable, bounded and observable. These are not novelty claims. |
| **PRODUCT UX** | The PWR desktop app for macOS and Windows -- conversation, steering/stop, plan/status, diffs/checks, permission prompts, model/resource indicators, session/workspace navigation -- as a client of `pwr serve` | Let a person authorize and inspect sustained work without reading raw telemetry. Not a research contribution, and still a release condition: the app is PWR's only front end -- a standalone application, not an editor or IDE plugin -- and the bar is that it alone drives every capability the agent has. The terminal console is the development and research tool, not a product surface. |
| **EXPERIMENTAL** | AST/LSP retrieval, predictive probes, model-generated summaries, adaptive planning, procedural memory, serial worker roles | Keep replaceable; require benefit over simple lexical retrieval and single-agent policies. |
| **LATER** | Browser/desktop adapters, vision workflows, MCP/plugin ecosystem, concurrency and heterogeneous model scheduling | Preserve interfaces; implement only after coding experiments justify the next capability. No marketplace, and no integration into third-party editors. Execution isolation on Windows is not here: it is a prerequisite of the Windows app, because the harness does not ship a platform where it cannot confine what the agent runs. |

## Design principles

1. **Model proposes semantics; harness preserves facts.** File hashes, tool outcomes, plan dependencies and check state are externally represented. A model claim is never automatically a verified fact.
2. **Measure behavior at the deployment boundary.** Identity includes weights/artifact, quantization, backend/version, tokenizer/template, adapter and settings. Unknown is distinct from unsupported and from observed success. A short successful probe is not certification.
3. **Separate declared, allocated, delivered and useful context.** Needle recall and occupancy authorize only their tested operating condition. Useful coding context needs task-like tests; largest context is a candidate policy, not a requirement.
4. **Expose one execution semantics to every client.** Chat, batch and evaluation should share runtime, policy and result contracts; deliberate mode differences are named and tested.
5. **Preserve evidence before compressing it.** Source text and logs are artifacts with provenance and bounded retention. The prompt receives selected views with a way to rehydrate them. A hash without retained bytes is not retrievable memory.
6. **Verify effects cheaply and honestly.** Exit status, patch application and checks support specific claims. Passing a suite is not proof that every natural-language requirement was met. No available verifier does not mean a prose answer must be impossible.
7. **Authorize effects, not model brands.** Workspace scope and user-configurable host access are policy. Tool data never grants permission. Deterministic normalization cannot widen scope or invent a missing semantic argument.
8. **Bound resources and recoveries.** Count all model calls and underlying actions, including calibration, failures, workers and retries. Serial inference is the initial default; concurrency must earn its cost.
9. **Migrate behind tested interfaces.** Retain Rust and working adapters/tools; use modules within the current workspace, not microservices or a new framework. Mature external parsers/tools are allowed behind policy; the historical ban on any Python tool is not a research principle.
10. **Negative results are deliverables.** Freeze thresholds and holdouts, retain failed campaigns, remove an intervention when evidence does not justify its complexity.

## Completion is an evidence contract

PLANNED common result semantics separate terminal state (`completed`, `blocked`, `interrupted`, `budget_exhausted`, `failed`) from evidence (`checks_passed`, `baseline_preserved`, `answer_delivered`, `not_checked`). A coding task can claim checked acceptance only when its non-exempt acceptance checks pass and the baseline contract holds. A diagnosis must cite repository evidence and preserve requested read-only scope; a documentation task can report structural/link checks and deliver prose without pretending semantic correctness was mechanically proven. Missing checks remain explicit. A model cannot waive checks or change acceptance criteria to rescue its score.

Current scripted execution is stricter for no-check tasks; current chat uses a different post-check contract. Neither automatically implements these planned semantics.

## Boundaries and non-goals

Two different lists follow, and running them together is how a sequencing decision hardens into a scope boundary nobody meant.

**Not what PWR is:** a chat skin over someone else's model runtime, autocomplete, a model trainer, a claim that small weights equal frontier models, a benchmark leaderboard optimized for one toy corpus, or a general SaaS platform. These do not become goals later.

**Not yet, and deliberately:** a mandatory vector database, a multi-agent swarm, model downloading, a cloud dependency, a plugin market, a distributed scheduler. Browser and computer control, network research tools and MCP adapters are on the intended path and are sequenced behind the first research milestone, because a capability admitted before the coding loop can be measured cannot be told from one that helped. The order is an evidence gate; it is not a statement about what the agent is allowed to touch. Local data and traces stay local by default; any external inference or network tool is an explicit disclosure and access decision, which is a matter of the user knowing and consenting rather than of the capability being withheld.

Full legitimate machine access is the destination stated above, not a concession. It requires explicit scopes for paths, commands, network, applications and credentials, with observable policy decisions and practical recovery. Existing macOS confinement remains until replacement is validated. No promise of universal rollback: network effects, package scripts and external application actions may be irreversible.

## Evidence vocabulary and authority

- **IMPLEMENTED:** a reachable path exists, named with source evidence; does not imply production maturity or measured model quality.
- **PROTOTYPED:** partial or experimental implementation/artifacts; required integration or validation is missing.
- **PLANNED:** selected engineering direction, not available behavior.
- **RESEARCHING:** unresolved design choice with a defined experiment.
- **HYPOTHESIS:** falsifiable claim with control, metric, threshold and rejection decision.

Authority order: this contract for scope and principles; [glossary](docs/glossary.md) for the meaning of the terms all of them use; [architecture](docs/architecture.md) for the candidate design; [research](docs/local-agent-research.md) and [evaluation](docs/evaluation.md) for experimental claims; [roadmap](docs/roadmap.md) for new sequencing; [audit](docs/adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md) for inspected implementation. [SECURITY.md](SECURITY.md) remains current operational guidance. All older ADRs, strategy prose, milestone tables and detailed implementation documents are subordinate revision-specific evidence, including where their original text says “accepted,” “binding,” or “current.” Runtime configuration has not been changed to match this new direction yet.

## Recording new decisions

The ADR series is closed at ADR-012. Its bodies are retained as evidence about the revisions that produced them; no ADR-013 will be written. A durable decision is now recorded by amending the document that owns it — scope and principles here, mechanism in [architecture](docs/architecture.md), an experimental commitment in [research](docs/local-agent-research.md), a measurement rule in [evaluation](docs/evaluation.md), sequencing in [roadmap](docs/roadmap.md), inspected behavior in the [audit](docs/adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md) — and dating the amendment. Superseded text is never deleted; it keeps its body and gains a notice naming what replaced it.

An experimental decision additionally requires a dated entry in the [experiment log](docs/experiment-log.md) naming the hypothesis ID, the conditions compared, the outcome including inconclusive ones, and the resulting keep, revise or remove statement. Every claim in the canonical set carries a status word from the vocabulary above; changing a status requires named source evidence in the same edit, and a document that cannot cite one says `unknown` rather than rounding up.
