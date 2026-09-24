# Migration from the current platform

> **Status note, 2026-09-23.** This plan of 2026-09-12 is kept as the record of
> the migration it set out; parts of it were overtaken. In particular its
> backend row ("Preserve Ollama; LM Studio is already implemented") no longer
> holds: both were removed on 2026-09-19 in favour of PWR's own MLX engine,
> with llama.cpp as the GGUF and Windows path. The current order of work is in
> [`roadmap.md`](../roadmap.md), "Update — 2026-09-23".

**Authoritative migration plan, 2026-09-12. PLANNED engineering work.** The new thesis is in [MASTER_SPEC](../../MASTER_SPEC.md), mechanisms in [architecture](../architecture.md), experimental decisions in [research](../local-agent-research.md), and exits in [roadmap](../roadmap.md). The [audit](CURRENT_ARCHITECTURE_AUDIT.md) gives per-subsystem dispositions and implementation evidence.

## What changes the direction

Keep the repository as an experimental platform. It already has backend lifecycle separation, compatibility adapters, calibrated operating points, hash-guarded tools, checked execution, context/state machinery and unusually useful failed-run records. Starting over would discard evidence and verified behavior without a demonstrated capability benefit.

The blocking gap is trustworthy comparison followed by shared execution: current evaluation excludes provider failures from several metrics and accepts weak pair identities, while interactive chat has a separate loop and completion semantics. Building an adaptive layer on that foundation could optimize a benchmark that the product does not actually run.

## Ordered engineering migration

| Step / current → target | Concrete change and dependencies | Acceptance / rollback |
|---|---|---|
| **1. Weak pair/accounting → strict trial contract (R0)** | Add typed immutable trial identity, declared treatment differences, duplicate rejection and all-attempt totals in `pwr-eval`. Initially wrap current verifier-supplied execution, explicitly labeled. Preserve old report readers and conditional metrics with their definitions. | Corrupt identity/duplicate/missing/provider-failure fixtures; totals reconcile. New schema/command is opt-in; old artifacts never reinterpreted. |
| **2. Two loops → shared session semantics (R1)** | Freeze path-specific fixtures. Extract orchestration behind a session interface and route `converse::take_turn`, scripted run and a new product-path eval adapter to it. Keep UI, tools, backend adapters and policy behavior. Make declared acceptance differences explicit. | Same effects/policy/evidence through each client; no new lost-goal, denied-action or false-acceptance regressions. Switch clients individually; revert a client binding without reverting working tools. |
| **3. Maximum context/static heuristics → experimental policy seam (R2)** | Expose versioned choices for existing context/schema/output/planning/retry policies. Collect actual behavior and resources; retain frozen legacy settings for comparison. Move backend-name parsing out of core decisions gradually. | Delivered requests/events name the condition; all budgets/provenance match. Keep legacy policy selectable; no new default before a result. |
| **4. Chosen weakness → one intervention (R3)** | Implement the smallest H1–H5 experiment, selected by R2 failure evidence. Durable artifact references/state and targeted result parsing are candidates, not mandatory work packages. | Preregistered causal report, including negative/inconclusive result and keep/remove decision. Feature flag or policy ID restores control. |
| **5. Fixed policies → measured selector only if useful (R4)** | Test H6 using development-only fitting and repository-disjoint confirmation, charging probes and cache/loading overhead. Do not train a router on holdout outcomes. | Confirmed per-cohort benefit over best fixed-per-deployment control. If rejected, stop at fixed policies; this is a valid project outcome. |
| **6. Replay projection → safe continuation (R5)** | Persist full graph/check/budget/policy/objective and pending-action state; reconcile workspace and process effects after interruption. Implement user steering on shared runtime and preserve dirty-tree ownership. | Kill/restart at action boundaries, external edit, branch/model switch and service cleanup fixtures; actual product tasks meet R5 gates. No automatic re-execution of ambiguous side effects. |
| **7. Workspace-only surface → explicit extensions (R6)** | Add one authorized host resource or multimodal/tool adapter based on observed coding need. Same policy, artifacts, metrics and client contracts. | Separate capability experiment and permission tests; remove the adapter without changing the core loop. |

This is a sequence of seams and experiments, not an instruction to move all code between crates first. Steps 1–2 repair the ability to make valid claims. Steps 3–5 determine what deserves permanent architecture. Long-horizon durability work can expose defects earlier, but its full product promise waits for R5 validation.

## Decisions that survive, change, or cease to bind

| Historical decision | New disposition |
|---|---|
| ADR-001 Rust runtime | Retain Rust core. The prohibition on external Python tools is superseded: isolated existing tools/parsers are allowed when useful and policy-bounded; no new required runtime dependency is introduced now. |
| ADR-002 Ollama first | Preserve Ollama; LM Studio is already implemented. Backend protocols are compatibility concerns, not product identity. |
| ADR-003 Qwen primary; ADR-004 Ornith challenger | Preserve their experiment records. Model roles/tags are dated cohort facts, not permanent privileged models or a current inventory. |
| ADR-005 hardware-aware planning | Retain measured resources/admission; replace largest-context objective and add task-useful context evidence. |
| ADR-006 provider separation | Retain, extending canonical typed modality/tool contracts as needed. |
| ADR-007 empirical evaluation | Retain and strengthen: all-attempt outcomes, strong fixed baselines, independent holdouts, paired uncertainty and explicit treatment differences. |
| ADR-008 deterministic verification | Retain objective checks for the claims they support. Supersede universal green-check completion for prose/diagnosis with explicit evidence-specific result semantics. |
| ADR-009 persistent repository intelligence | Retain provenance/invalidation; do not assume vector memory or resolved semantic graphs are required. |
| ADR-010 adaptive reasoning | Keep an experimental policy parameter; no evidence-free default depth or new model inference for every tool decision. |
| ADR-011 deferred/declined routing | Keep the failed-proxy observations. Supersede the categorical prohibition and 262K premise; current explicit/auto selection exists, quality remains unmeasured, multi-model scheduling stays experimental. |
| ADR-012 bespoke PDF extraction | Preserve existing behavior and narrow limitations. Reopen parser selection with an external corpus; dependency minimalism alone does not justify maintaining a general PDF parser. |
| Adaptive-runtime AR-001–014 | Retain completion records as the history of adapter/discovery migration. Their old audit is replaced; the new work does not reimplement those completed seams. |
| Historical M0–M6 and thresholds | Keep generated table, manifest and experiments unchanged as revision-scoped evidence. R0–R6 are the new sequence; historical bars do not certify new architecture. |

## Technical debt that matters now

Pair validation and provider-failure accounting block research claims. Duplicate chat/scripted compaction and acceptance block product transfer. Approximate context accounting, ignored or weak deployment/hardware identity fields, and discarded output without artifact retention can turn resource/evidence failures into apparent model failures. Partial state replay and unsupervised in-flight effects block honest resume. CLI orchestration volume makes all of these harder to keep aligned; extract behavior where these contracts need a seam, not to optimize line counts.

Existing `strategies/models.json` still contains historical full-context minima and `strategies/default.json` task/model-specific prompt experiments. **They remain runtime configuration in this documentation-only change, not new normative requirements.** A later policy migration must preserve them as a frozen baseline and explicitly version changes. Likewise this task does not change chat's broad grants, sandbox availability, completion behavior, probe requirements or quality selection.

No automatic Git reset/clean, rewriting user work or mass deletion of experiments is part of migration. New checkpoints must distinguish user input changes from agent changes. Restore is conditional on hashes/effect ownership and cannot reverse arbitrary host/network operations.

## Documentation authority and preservation

Eight existing files formed the canonical set in the first pass: project entry/contract, research, architecture, evaluation, inspected audit, migration and sequential roadmap. The follow-up pass recorded at the end of this document added one, [docs/glossary.md](../glossary.md), because three central words — policy, checkpoint, context — carried more than one sense across those eight. All replaced versions are recoverable at commit `f7ad8ca6135bc88517f0a43bc333898c935534bc` (for example `git show f7ad8ca:MASTER_SPEC.md`). No pre-existing documentation edits were present at task start. Historical roadmap text, experiment log, protocols, reports and ADR bodies are retained; status notices make their scope explicit.

The source audit corrects contradictions: old claims of Ollama-only backends, macOS-only hardware probing, tag-only profiles, absent multi-hunk patches and universally declined auto selection are obsolete. The older benchmark text cannot establish current chat quality. Prior causal interpretations of bundled prompt/output experiments are qualified in the new research synthesis without altering the recorded observations.

The notices on detailed documents preserve useful implementation explanations while directing readers to the new canonical files. They are not a fresh line-by-line conformance certification of every historical paragraph. `SECURITY.md` remains operational guidance and receives a current-console caveat; it does not promise the planned full-host policy.

## Validation of this documentation migration

Validate relative links in the canonical set and status notices, `git diff --check`, historical-body preservation, the generated M-series block using `scripts/milestones.py`, and hashes of every source/config/test file already modified before this task. No Rust source, dependencies, runtime strategy values, corpus, experiment report or benchmark threshold is changed by this task. Offline Rust tests and live-model runs are not substitutes for documentation validation and are not claimed as newly executed.

## Exact file ledger

The generated list below records every documentation file touched in this task. “Deprecated” means deprecated as an authoritative design, with its body retained; no file is deleted. “Reference notice” means revision-scoped background, not deletion or withdrawal of useful implementation evidence.

First pass — Created: **none**. Deleted: **none**.

| Exact file | Change |
|---|---|
| [MASTER_SPEC.md](../../MASTER_SPEC.md) | Canonical rewrite |
| [README.md](../../README.md) | Canonical rewrite |
| [SECURITY.md](../../SECURITY.md) | Current-console operational caveat |
| [docs/CLI-spec.md](../CLI-spec.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/ARCHITECTURAL_DECISIONS.md](ARCHITECTURAL_DECISIONS.md) | Deprecated authority; original body retained |
| [docs/adaptive-runtime/BENCHMARK_AND_CERTIFICATION.md](BENCHMARK_AND_CERTIFICATION.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/CONTEXT_ARCHITECTURE.md](CONTEXT_ARCHITECTURE.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md](CURRENT_ARCHITECTURE_AUDIT.md) | Canonical rewrite |
| [docs/adaptive-runtime/HARDWARE_AND_TIER_SYSTEM.md](HARDWARE_AND_TIER_SYSTEM.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/MIGRATION_BACKLOG.md](MIGRATION_BACKLOG.md) | Deprecated authority; original body retained |
| [docs/adaptive-runtime/MIGRATION_PLAN.md](MIGRATION_PLAN.md) | Canonical rewrite |
| [docs/adaptive-runtime/MODEL_COMPATIBILITY_LAYER.md](MODEL_COMPATIBILITY_LAYER.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/MODEL_PROFILE_SPEC.md](MODEL_PROFILE_SPEC.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/MODEL_SELECTION_AND_ROUTING.md](MODEL_SELECTION_AND_ROUTING.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/PWR_PRODUCT_EVOLUTION.md](PWR_PRODUCT_EVOLUTION.md) | Deprecated authority; original body retained |
| [docs/adaptive-runtime/RISKS_AND_TECHNICAL_DEBT.md](RISKS_AND_TECHNICAL_DEBT.md) | Reference notice; original body retained |
| [docs/adaptive-runtime/TARGET_ARCHITECTURE.md](TARGET_ARCHITECTURE.md) | Deprecated authority; original body retained |
| [docs/adaptive-runtime/TEST_STRATEGY.md](TEST_STRATEGY.md) | Reference notice; original body retained |
| [docs/adr/ADR-001-rust-runtime.md](../adr/ADR-001-rust-runtime.md) | Reference notice; original body retained |
| [docs/adr/ADR-002-ollama-first.md](../adr/ADR-002-ollama-first.md) | Reference notice; original body retained |
| [docs/adr/ADR-003-qwen-primary.md](../adr/ADR-003-qwen-primary.md) | Reference notice; original body retained |
| [docs/adr/ADR-004-ornith-challenger.md](../adr/ADR-004-ornith-challenger.md) | Reference notice; original body retained |
| [docs/adr/ADR-005-hardware-aware-planning.md](../adr/ADR-005-hardware-aware-planning.md) | Reference notice; original body retained |
| [docs/adr/ADR-006-provider-separation.md](../adr/ADR-006-provider-separation.md) | Reference notice; original body retained |
| [docs/adr/ADR-007-empirical-evaluation.md](../adr/ADR-007-empirical-evaluation.md) | Reference notice; original body retained |
| [docs/adr/ADR-008-deterministic-verification.md](../adr/ADR-008-deterministic-verification.md) | Reference notice; original body retained |
| [docs/adr/ADR-009-persistent-repository-intelligence.md](../adr/ADR-009-persistent-repository-intelligence.md) | Reference notice; original body retained |
| [docs/adr/ADR-010-adaptive-reasoning.md](../adr/ADR-010-adaptive-reasoning.md) | Reference notice; original body retained |
| [docs/adr/ADR-011-deferred-model-routing.md](../adr/ADR-011-deferred-model-routing.md) | Reference notice; original body retained |
| [docs/adr/ADR-012-document-import.md](../adr/ADR-012-document-import.md) | Reference notice; original body retained |
| [docs/agent-loop.md](../agent-loop.md) | Reference notice; original body retained |
| [docs/architecture.md](../architecture.md) | Canonical rewrite |
| [docs/benchmark-design.md](../benchmark-design.md) | Reference notice; original body retained |
| [docs/benchmark-plan.md](../benchmark-plan.md) | Reference notice; original body retained |
| [docs/calibration.md](../calibration.md) | Reference notice; original body retained |
| [docs/context-management.md](../context-management.md) | Reference notice; original body retained |
| [docs/data-model.md](../data-model.md) | Reference notice; original body retained |
| [docs/direction.md](../direction.md) | Deprecated authority; original body retained |
| [docs/evaluation.md](../evaluation.md) | Canonical rewrite |
| [docs/experiment-log.md](../experiment-log.md) | Reference notice; original body retained |
| [docs/hardware-profiling.md](../hardware-profiling.md) | Reference notice; original body retained |
| [docs/local-agent-research.md](../local-agent-research.md) | Canonical rewrite |
| [docs/manual-testing.md](../manual-testing.md) | Reference notice; original body retained |
| [docs/memory-state.md](../memory-state.md) | Reference notice; original body retained |
| [docs/model-profiles.md](../model-profiles.md) | Reference notice; original body retained |
| [docs/observability.md](../observability.md) | Reference notice; original body retained |
| [docs/repository-intelligence.md](../repository-intelligence.md) | Reference notice; original body retained |
| [docs/roadmap.md](../roadmap.md) | New research sequence; historical body and generated block retained |
| [docs/security-sandboxing.md](../security-sandboxing.md) | Reference notice; original body retained |
| [docs/testing-strategy.md](../testing-strategy.md) | Reference notice; original body retained |
| [docs/thresholds.md](../thresholds.md) | Reference notice; original body retained |
| [docs/tool-runtime.md](../tool-runtime.md) | Reference notice; original body retained |
| [docs/verification-recovery.md](../verification-recovery.md) | Reference notice; original body retained |
| [docs/vision-and-scope.md](../vision-and-scope.md) | Deprecated authority; original body retained |

## Follow-up pass, 2026-09-12

The first pass left three gaps: no shared definition of the terms the canonical set relies on, no statement of how a durable decision is recorded now that the ADR series is closed, and contribution guidance still carrying two rules the new contract supersedes. Relative links across all 57 Markdown files resolve, `scripts/milestones.py` reports the roadmap already matches its manifest, and the audit's verifiable facts were re-checked against the tree: commit `f7ad8ca6135bc88517f0a43bc333898c935534bc`, 14 crates, Rust edition 2024 with MSRV 1.88. No Rust source, dependency, runtime strategy value, corpus or threshold is changed by this pass either.

Created: [docs/glossary.md](../glossary.md). Deleted: **none**.

| Exact file | Change |
|---|---|
| [docs/glossary.md](../glossary.md) | Created: canonical vocabulary, disambiguating behavior policy from access policy, workspace checkpoint from context checkpoint, and declared/allocated/delivered/useful context |
| [MASTER_SPEC.md](../../MASTER_SPEC.md) | Glossary added to the authority order; new section closing the ADR series and stating how durable and experimental decisions are recorded |
| [README.md](../../README.md) | Glossary row in the design table; ADR closure noted |
| [CONTRIBUTING.md](../../CONTRIBUTING.md) | New section on status words, decision recording, preservation of superseded text and the lifted Python prohibition; one added principle about measured and product paths |
| [docs/architecture.md](../architecture.md) | Header points at the glossary for the policy split |
| [docs/contribution-guidelines.md](../contribution-guidelines.md) | Superseded-process notice; original body retained |
| [docs/adaptive-runtime/MIGRATION_PLAN.md](MIGRATION_PLAN.md) | This section and ledger; one typo corrected |

## Second follow-up, 2026-09-12: the destination said plainly

The first two passes described the destination accurately and buried it. Full machine and network reach appeared under boundaries as a "long-term capability, not prohibited by design", browser automation sat in a sentence that also refused a plugin market, and product UX was listed as not a research contribution without saying it was still a release condition. A reader could finish the set believing the restricted surface was the design rather than the current rung.

So the contract now states it where the definition is: an unrestricted coding agent driven from a conversation, reaching anything on its machine and anything on the authorized network, with ease of use as an acceptance requirement rather than a finishing touch. The non-goals split into two lists, because a sequencing decision and a scope boundary were being read as one thing — what PWR will never be, and what is deliberately not yet. R0-R6 are unchanged: the order in which capability is admitted is still an evidence gate, and the gate is now labelled as one.

| Exact file | Change |
|---|---|
| [MASTER_SPEC.md](../../MASTER_SPEC.md) | Destination and usability requirement stated in the definition; non-goals split into permanent and sequenced; full machine access reframed from concession to destination |
| [README.md](../../README.md) | The destination on the first screen, with the current distance from it named |
| [docs/architecture.md](../architecture.md) | Default posture named: capability-complete under authorization |
| [docs/roadmap.md](../roadmap.md) | R6 says its one-at-a-time rule is about evidence, not about scope, and that an extension ships drivable from the conversation |
