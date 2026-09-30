# Candidate architecture: an evidence-driven local session

**PLANNED target, 2026-09-12.** Component names below describe internal contracts, not newly implemented Rust types. The [audit](adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md) identifies actual reuse. [MASTER_SPEC](../MASTER_SPEC.md) defines scope; [research](local-agent-research.md) defines experiments; the [glossary](glossary.md) fixes the sense of the terms used below, including the deliberate split between behavior policy and access policy. This replaces the older target architecture without authorizing a big-bang rewrite.

## Shape and boundaries

Use a modular Rust application in the existing Cargo workspace. One session runtime serves the terminal UI, noninteractive CLI and evaluator. Inference remains in external local backend processes; tools run within policy-managed process boundaries. A future local API is another client, not an independent orchestration service. Do not create a crate per box until dependency or testing needs require it.

```text
TUI / CLI / evaluator / later local API
                  |
             SessionRuntime <---- user steering and authorization
                  |
     TaskState + evidence/artifact references
                  |
   ContextEngine --> BehaviorPolicy --> ModelAdapter --> InferenceBackend
         ^                                  |                 |
         |                           canonical proposal <-----+
         |                                  |
         |                   validate --> PolicyEngine
         |                                  |
         +---- observation <-- ToolExecutor / ProcessSupervisor
         |                                  |
         +---- check evidence <------- Verifier / RecoveryPolicy

All transitions --> EventStore --> UI projection / diagnostics / evaluation
RepositoryIndex --> ContextEngine; CapabilityEvidence --> BehaviorPolicy
```

> **Note, 2026-09-24.** This table records the 2026-09-12 target. Since then the client is the desktop app (Tauri 2 + Angular), a client of `pwr serve --stdio` (ACP). The ratatui console remains as the development tool; see the [audit](adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md) and [PWR_PRODUCT_SOURCE_OF_TRUTH.md](PWR_PRODUCT_SOURCE_OF_TRUTH.md) for what is built.

No adapter executes tools. No model result authorizes effects. Evaluator-hidden checks never reach the context engine, task state, file search or ordinary tool process.

| Component | Responsibility and state owned | Dependencies and boundary reason | Current reuse / decision |
|---|---|---|---|
| Session runtime | Session/turn lifecycle, current objective revision, cancellation, pending actions, budgets, terminal result | Canonical contracts below; no terminal rendering or backend wire formats. Ensures the measured path is the product path. | EVOLVE orchestrator; REWRITE chat loop onto it. |
| Model runtime/backend | Model discovery, load/unload, granted context, streaming/cancellation and observed timings | Provider transport and hardware facts; no task policy. Distinguishes backend defects from model outputs. | KEEP `ModelProvider`, `InferenceBackend`, `RuntimeFactory`; the MLX engine and llama.cpp (Ollama and LM Studio removed 2026-09-19). |
| Capability evidence | Versioned probe observations, sample counts/uncertainty, provenance and invalidations | Backend, calibration fixtures, artifact store; no tool permission. Separates observed behavior from static names. | EVOLVE domain observations and existing probes/calibration. |
| Behavior policy/adaptation | Frozen policy version and finite choices for schemas, edit format, ordering, output reserve, planning/retries | Capability evidence, task/runtime observations; emits representation choices, not new authority. Independently ablatable. | EVOLVE `pwr-compat`, profiles and task strategy; adaptive selection RESEARCHING. |
| Context engine | Prompt manifest, section budgets, selected evidence IDs, compaction watermark, recent window | Task state, index, observations, adapter cost estimate; never owns canonical facts. Reduces lost/stale evidence and context cost. | EVOLVE typed `context::compile`; retire duplicate chat compactor. |
| Working state | Objective/constraints, plan dependencies, explicit claims/assumptions, evidence links, checks, files touched, pending questions | Event reducer; model may propose state updates, tool/check results establish facts. Keeps bookkeeping outside prose. | EVOLVE `RunState`, plan graph, task ledger; exact resume missing. |
| Memory | Validated repository facts and optional reusable procedures, each scoped and invalidatable | Store and verification; separate from active state. Cross-task semantic/procedural promotion must earn evidence. | KEEP persisted index; semantic/procedural memory RESEARCHING. |
| Repository intelligence | File inventory, content hashes, shallow symbols/imports, optional semantic edges, freshness | Filesystem/Git and language adapters; never asserts inferred edges as facts. Avoid repeated scans and improve localization. | EVOLVE `pwr-repo`; AST/LSP/embeddings are alternative interventions. |
| Tool registry | Canonical name, input/output schema, effect class, required scope, concurrency class, result adapter | Domain contracts; one schema source for offering/decoding. Prevent advertised/accepted tool mismatch. | EVOLVE `ToolCatalog`/`ActionProposal`; keep offered schemas and accepted actions in parity. |
| Policy engine | Workspace trust, grants, protected paths, one-action/session scope, policy epoch, denial evidence | User intent and OS adapters; independent of model parsing. Supports legitimate wider access without invisible widening. | EVOLVE `ToolPolicy`/approvals/seatbelt. |
| Executor and process supervisor | Action receipts, process IDs, output artifacts, timeouts, service lifecycle, patch transactions | Policy engine, filesystem/process adapters. Central ownership avoids orphan processes and uncertain writes. | KEEP executor, hash guards and `ServiceSupervisor`; background/PTY sessions PLANNED. |
| Result processing | Bounded observation, diagnostic extraction, truncation/redaction metadata, raw artifact locator | Tool outcome/artifact store; no authority to turn partial logs into success. Saves inference without discarding evidence. | EVOLVE `bounded_result`, head/tail output and located diagnostics. |
| Verification | Initial acceptance contract, baseline, check records, postconditions and coverage limits | Same executor/policy as tools; no model self-certification. Separates execution success from task acceptance. | KEEP `pwr-verify`; EVOLVE mode-specific completion and shared integration. |
| Recovery | Failure class, attempts, observed-effect history, strategy transitions, checkpoint eligibility | Runtime, checks and evidence; cannot grant access or change acceptance. Prevent repeated deterministic failure. | EVOLVE loop/progress detectors and recovery budgets. |
| Git/workspace | Root/HEAD/dirty diff identity, change ownership, checkpoint manifests | Repository/executor/store. Preserves user changes and revalidates state after branch changes. | KEEP status/diff; transaction-aware restore PLANNED. |
| Multimodal gateway | Typed attachments and transformed-view provenance, model-specific limits | Capability evidence, artifacts and backend encoders; no text-only path pretends to see an image. | Text/PDF import exists; image request/action support PLANNED. |
| Persistence/events | Durable typed events, artifacts, checkpoints, schema migrations and retention rules | Local SQLite/filesystem; append before advancing externally observable state. Makes faults inspectable and resumption possible. | EVOLVE store/observe, existing hashes and replay. |
| Evaluator | Trial manifest, treatment identity, schedule, all-attempt outcomes and paired estimates | Same session runtime plus isolated hidden checks; owns experimental assignment only. Prevents benchmark-only behavior. | EVOLVE `pwr-eval`; current eval remains verifier-supplied. |
| Client/API/UI | Conversation and derived session view; interrupt, steer, approve, inspect artifacts | Commands/events from runtime; no provider prompting or verification decisions. Avoid a third loop. | KEEP ratatui frontend; extract business logic. No API server currently. |

## Model and harness interaction

A deployment fingerprint must cover actual artifact identity, quantization, backend/version, model template/tokenizer and runtime options, adapter version, loaded window, and relevant hardware. If the backend exposes only a weak identity, record that uncertainty; a locally derived variant ID is not proof of the weight bytes. Existing profile precedence can remain as configuration compatibility, but a model-name substring cannot establish capability.

Probe in tiers. Discovery cheaply gathers declared modalities/window and backend features. Protocol probes test complete streams, tool names/arguments, abstention and cancellation. Task-like probes vary tool-set size, dependency depth, ambiguous arguments, stale paths, patch formats and evidence positions. Performance probes separate loading, cold prefill, cached prefill, decode, memory/pressure and context occupancy. Cache evidence by fingerprint; run a small refresh on changes. Price probe cost and compare predictive value against simply observing the first ordinary turns. Do not force an afternoon of probes before every chat.

Separate `supported`, `observed reliable in these trials`, `unsupported` and `unknown`. Measure rates by condition with trial counts; parallel-call validity, planning depth and patch accuracy are distributions, not enduring booleans. A short JSON probe may authorize experimentation but cannot predict repository repair without validation. Declared vision includes limits on image dimensions/count and encoding; test actual screenshot interpretation separately.

The canonical action stays the same while its presentation varies: native tool schema or constrained JSON, full or grouped catalog, `replace_text` versus multi-hunk edit, compact error fields versus bounded logs. State-dependent tool subsets need an explicit discovery path for omitted tools. Grammar constraints are optional backend features; schema-valid wrong arguments still fail semantic/precondition checks. Initially choose among a few preregistered policies; do not build a learned meta-agent before fixed alternatives are measured.

Deterministic repair is limited to unambiguous, versioned transformations, such as recognized tool aliases or decoding a JSON argument string. Record original and normalized forms. Do not infer a missing file, select between ambiguous matches, convert a read into a write, or retry a potentially completed command blindly. Return a precise refusal with relevant known facts and ask for a corrected proposal within budget.

## Context and external state

Maintain three distinct stores: an authoritative event/working-state record; retrievable repository/tool artifacts; and a disposable model-visible context. The entire conversation is not the database. Working-state entries carry `kind`, scope, source event/artifact, file hash/range where applicable, status, and invalidation conditions. Hypotheses, model claims, human decisions and observed check outcomes remain distinguishable.

A turn assembles the latest objective and constraints, compact state and completion criteria, relevant evidence, the last action/result exchange and a bounded recent conversation. Preserve call/result pairing. Model-generated summaries are optional derived artifacts that cannot replace source facts. Edits invalidate affected excerpts and dependent claims; branch switches and external changes require revalidation. Resume must use current bytes, not an old hash described as current.

Start with the existing lexical and shallow-symbol index. Incremental content validation, recently touched files, diagnostic locations, Git changes and ready subgoals provide cheap signals. Compare language-parser/LSP edges against this baseline before requiring an AST service or embedding model. Label heuristic imports/test ownership and retain provenance, not a fictitious resolved call graph. Retrieve a targeted window and rehydrate its source when detail matters.

Budget system instructions, tools, task/state, retrieved text, observations, modality tokens and output reserve separately. Use backend/tokenizer accounting where available and record estimates otherwise. Essential state that cannot fit causes a named reduction or failure; it must not silently overflow. A fixed quarter-window reserve and maximum calibrated tier are legacy heuristics to test, not permanent constants. Stable instruction prefixes may improve cache reuse; report cache state because reordered prompts can trade retrieval benefit for prefill cost.

Persist large logs before reducing them, with artifact ID, byte ranges and retention/access policy. Retain exit status, relevant diagnostic locations, head/tail or structured failures, explicit missing/truncated fields and an accessible raw view. Current command capture can hash discarded bytes without retaining them: it cannot honestly offer rehydration of those bytes. Storage must be bounded separately from prompt size and secret-bearing artifacts protected before indexing.

## Runtime, verification and recovery

The session progression is `admit → observe/plan → propose → validate → authorize → execute → observe effects → verify as needed → continue or conclude`. A new user objective creates a revision and causes relevant plan/acceptance assumptions to be reconciled; it does not erase the session. A plan is optional. Dependencies help expose ready work; neither a plan nor `record_progress` grants permission. Validate missing edges/cycles and preserve the graph on checkpoint. Ordinary statements are conversation; a terminal task result is a separate evidence-bearing event.

Before a mutation record the baseline, intended effect, affected-path hashes and policy decision. Afterward check the cheap postcondition: created path/hash, applied patch, command exit or live process. Run narrow syntax/tests when relevant and affordable, broad acceptance at completion. No redundant full test suite after every read. Separate check-discovery failure, environment failure, flake, code failure and unknown. For prose and diagnosis, use explicit delivery/evidence contracts and report limits; do not require green code tests as a substitute for answer correctness.

Recovery is a bounded state transition with a reason: malformed call → exact schema feedback; absent path → fresh search; stale edit → re-read and re-propose; failed patch → retain original bytes; compiler failure → located evidence; transient provider error → compatible retry; context overflow → rebuild at an authorized setting; repeated unchanged effects → change strategy or stop. Check state and artifact novelty determine progress; a new timestamp, repeated read or edit/revert cycle does not. Compaction count alone is not proof of a loop. Semantic planning contradictions may need model or user resolution; do not pretend deterministic code can settle them.

Crash recovery distinguishes action intent from observed receipt. An interrupted read can be repeated; an ambiguous write/process/network operation must be reconciled against effects before retry. No general exactly-once guarantee across arbitrary external tools. A checkpoint binds objective, graph, pending action state, checks, budgets, deployment/policy and workspace hashes. Restoring files must preserve pre-existing dirty work and never silently reset the user's branch. Sessions persist across process restarts only after these invariants are tested.

## Full local access and extensions

The default posture is capability-complete under authorization: the question a request meets is what the user has granted, never whether the agent is the kind of thing that does that. The target supports authorized filesystem roots beyond one repository, shell/processes, Git, toolchains, network and web access, browser and local applications. Policy is capability/effect based: path roots and read/write rights; process/executable or explicit shell interpreter; local-service and external-network scope; application/account access; destructive/publication/credential decisions. User-selected autonomy profiles summarize concrete grants, not vague trust in the model. A trusted workspace can configure checks but cannot grant host authority through an injected file.

Allow reversible authorized work without repeated prompts. For a new destructive or external effect, surface the concrete command/diff/target and whether approval is single-use or session-wide. Record denial, expiration, revocation and policy epoch. Child workers/plugins inherit at most the parent's scope. MCP is a transport/registration adapter whose output is untrusted data, not an alternative permission system. Keep explicit argv execution; an authorized shell is a distinct, audited capability.

Keep macOS confinement while designing other OS adapters. `LocalService` on seatbelt is host-local and can include LAN interfaces; it is not a loopback-only guarantee. User-authorized unconfined execution must be unmistakable and cannot be labeled sandboxed. Rollback covers tracked local artifacts where practical; it cannot undo a package lifecycle script's remote side effects or publication. Logs and model-server endpoints have separate disclosure/retention policies.

## Capability-driven multimodality

PLANNED canonical content parts include text and image artifact references, MIME type, dimensions and transformation provenance; future audio/video can extend this rather than being embedded as fake text. The adapter encodes only supported parts. For an image-capable deployment, expose screenshot inspection with bounded resolution, optional crops and observed image-token/latency cost. A text-only deployment receives an explicit unsupported result and may use a permitted OCR/DOM/accessibility adapter, clearly labeled as a different observation.

First vision experiment: inspect a locally generated UI, identify a defect and verify its repair through DOM/browser behavior plus screenshot review. A passing HTML asset-reference check is not visual regression testing. Computer actions use observe → act → verify, coordinate frames and fresh screenshots; stale images cannot authorize blind clicks. Do not add a vision model merely because a field says `vision=true`; loading cost and capability benefit enter evaluation.

## Agentic chat and events

The conversation is the user-facing engineering session. Its primary view shows objective, meaningful status, current plan, active deployment/evidence level, recent actions and the latest result. Expandable panels expose commands/output, files read/modified, diff, checks, checkpoints, approvals and later workers. Context occupancy and model latency distinguish waiting on inference from a stuck process. Tokens and memory are measurements or labeled estimates.

Interrupt is a runtime command. Stop generation promptly, supervise process cancellation, then report reconciled effects. Steering enters at a safe action boundary and increments the objective revision. Workspace/model switches preserve history but invalidate incompatible checks/context/capability evidence; they never carry grants silently to a new root. Model swapping mid-session is an experiment until native call history can be re-encoded correctly.

Use a versioned event envelope with session/task/turn/action IDs, sequence, timestamp, objective revision, policy/deployment reference, causal parent, artifact references and audience class. This is a proposed schema, not the current wire type.

- **User milestones:** objective/plan changed, action started/completed, files changed, checks finished, blocked/approval required, checkpoint/resume, terminal result. Coalesce routine reads; show their detail on demand.
- **Audit evidence:** original proposal, normalization, policy verdict, executed arguments, output hashes, observed effects and verification coverage. Retain inspectable records without flooding chat.
- **Telemetry:** token/cost estimates, cache/pressure samples, raw deltas and diagnostic counters. Aggregate by default; missing values stay unknown.

Plans, assumptions and evidence-backed decisions are intentionally externalized artifacts. Do not expose hidden chain-of-thought as an observability feature. A later local API should authenticate clients, bind each command to a session/workspace, stream these events and apply the same policy; no browser client directly invokes host tools.
