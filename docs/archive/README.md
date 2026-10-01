# Archive

Documents superseded from 2026-09-30, when the documentation was rewritten from
the code (decision D-2026-09-30-5). They are kept whole, as evidence of how the
design was reasoned about. **None of them describes the current system**:
several contradict it (see the [verification](../reviews/2026-09-30-verification.md),
N7 and 9.4). Their own internal links point to where the files used to be.

| Archived | What it was | Replaced by |
|---|---|---|
| [MASTER_SPEC-2026-09-30.md](MASTER_SPEC-2026-09-30.md) | The bounded local coding contract of 2026-09-30 | [MASTER_SPEC](../../MASTER_SPEC.md), adopted 2026-10-01 (D-2026-10-01-2) |
| [MASTER_SPEC-2026-09-12.md](MASTER_SPEC-2026-09-12.md) | The project and research contract of 2026-09-12 | [MASTER_SPEC](../../MASTER_SPEC.md) |
| [PWR_PRODUCT_SOURCE_OF_TRUTH.md](PWR_PRODUCT_SOURCE_OF_TRUTH.md) | Feature-by-feature account compiled for v0.1.0 (2026-09-24) | [feature-status](../feature-status.md) and the technical documents |
| [roadmap.md](roadmap.md) | Research roadmap, reconciliations and release plans to 2026-09-28 | [roadmap](../roadmap.md), [plan](../plan/implementation-plan.md) |
| [backlog.md](backlog.md) | Every item not built, with the 2026-09-27 reconciliation and closed records | [plan, carry-over tables](../plan/implementation-plan.md#carried-over-from-earlier-plans) |
| [v0.3.0-alpha-plan.md](v0.3.0-alpha-plan.md) | Release plan of 2026-09-28: v0.2.x fixes, Windows engine, consolidation | [plan](../plan/implementation-plan.md); Windows moved to Later ([D-2026-09-30-4](../decisions.md)) |
| [experiment-log.md](experiment-log.md) | Experiment log to 2026-09-17 | [experiment-log](../experiment-log.md) |
| [milestones-2026-09-06.json](milestones-2026-09-06.json) | Research milestones M0–M6 | [milestones.json](../milestones.json) (gates G0–G3) |
| [architecture.md](architecture.md) | Candidate architecture (PLANNED, 2026-09-12) | [architecture](../architecture.md) |
| [agent-loop.md](agent-loop.md) | Agent loop, pre-2026-09-12 | [agent-loop](../agent-loop.md) |
| [context-management.md](context-management.md) | Context management, pre-2026-09-12 | [context](../context.md) |
| [models-and-context.md](models-and-context.md) | Context, hardware detection, Model Manager (2026-09-23) | [context](../context.md), [models](../models.md), [inference](../inference.md) |
| [model-compatibility.md](model-compatibility.md) | Profiles, Quick Calibration, Reasoning Effort (2026-09-24) — said reasoning is never kept in history, which is no longer true | [models](../models.md) |
| [model-profiles.md](model-profiles.md) | Model roles of the Ollama era | [models](../models.md) |
| [memory-and-wiki.md](memory-and-wiki.md) | Profile, memory and wiki (2026-09-25) | [context](../context.md#the-project-wiki-and-memory) |
| [memory-state.md](memory-state.md) | Memory and state, pre-2026-09-12 | [state-and-persistence](../state-and-persistence.md) |
| [repository-intelligence.md](repository-intelligence.md) | Repository index, pre-2026-09-12 | [context](../context.md#repository-knowledge) |
| [tool-runtime.md](tool-runtime.md) | Tool runtime, pre-2026-09-12 | [tools-and-sandbox](../tools-and-sandbox.md) |
| [security-sandboxing.md](security-sandboxing.md) | Security design, pre-2026-09-12 | [SECURITY](../../SECURITY.md), [tools-and-sandbox](../tools-and-sandbox.md) |
| [verification-recovery.md](verification-recovery.md) | Verification and recovery, pre-2026-09-12 — says no-verifier completion is refused, which is not true | [verification](../verification.md) |
| [pwr-serve.md](pwr-serve.md) | Protocol design and history (S1–S3) | [pwr-serve](../pwr-serve.md) |
| [current-cli.md](current-cli.md) | Command-line guide (2026-09-24) | [cli](../cli.md) |
| [CLI-spec.md](CLI-spec.md) | CLI specification of the Ollama era | [cli](../cli.md) |
| [evaluation.md](evaluation.md) | Evaluation methodology (2026-09-12, amended 2026-09-23) | [evaluation](../evaluation.md) |
| [benchmark-design.md](benchmark-design.md), [benchmark-plan.md](benchmark-plan.md) | Capability-suite design and corpus plan | [evaluation](../evaluation.md), plan W8 |
| [calibration.md](calibration.md), [hardware-profiling.md](hardware-profiling.md) | Context-ladder calibration and hardware probing | [inference](../inference.md#the-working-window), [models](../models.md) |
| [r3-h2-evidence-state.md](r3-h2-evidence-state.md) | Design of the evidence-state compaction experiment (R3, H2) | Plan W4.7 |
| [redesign-2026-09-17.md](redesign-2026-09-17.md) | Redesign proposal after R3 was stopped | The plan |
| [local-agent-research.md](local-agent-research.md) | Research thesis, prior art, hypotheses (2026-09-12) | [MASTER_SPEC](../../MASTER_SPEC.md), plan W8 |
| [direction.md](direction.md), [vision-and-scope.md](vision-and-scope.md) | Earlier directions | [MASTER_SPEC](../../MASTER_SPEC.md) |
| [documentation-rag-and-kv-cache.md](documentation-rag-and-kv-cache.md) | Proposal C.22c (2026-09-27) | Later (plan) |
| [frontend-spike.md](frontend-spike.md) | The Tauri/Angular vs Slint decision | [desktop](../desktop.md) |
| [manual-testing.md](manual-testing.md) | Manual testing protocols to 2026-09-21 | [testing](../testing.md#manual-passes) |
| [prompt-cache-check-2026-09-25.md](prompt-cache-check-2026-09-25.md) | A prompt-cache verification procedure | [inference](../inference.md) |
| [observability.md](observability.md), [data-model.md](data-model.md), [testing-strategy.md](testing-strategy.md), [contribution-guidelines.md](contribution-guidelines.md) | Pre-2026-09-12 design notes | [state-and-persistence](../state-and-persistence.md), [testing](../testing.md), [CONTRIBUTING](../../CONTRIBUTING.md) |
| [glossary.md](glossary.md) | Vocabulary of 2026-09-12 | [glossary](../glossary.md) |
| [adaptive-runtime/](adaptive-runtime/) | The adaptive-runtime design set and its 2026-09-12 audit | The plan; the audit's successor is the [verification](../reviews/2026-09-30-verification.md) |
| [adr/](adr/) | ADR-001 to ADR-012 (series closed) | [decisions](../decisions.md) |
