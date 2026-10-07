# Roadmap

**Source status reviewed 2026-10-07.** The order of work under the mission adopted on 2026-10-01. The detail
of every item — problem, evidence, change, acceptance, tests — is in the
[implementation plan](plan/implementation-plan.md); this page is the summary a
reader needs first. The roadmap before this date, with its reconciliations and
release plans, is in the [archive](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/roadmap.md).

## Where things stand

- **Released:** v0.2.0-alpha (2026-09-28), macOS on Apple silicon, as a
  prerelease. Fixes since then are on `develop` and recorded in
  [release/v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md).
- **On `develop`, not released:** local commits since the
  documentation baseline (`0776ff4f`): most of gate G1, the session executor, and
  the model-compatibility and recovery work of 2026-10-01 (adapters for the
  Mistral, Liquid and Granite families and a Qwen3-Coder format, tool-call ids
  for Gemma, a sampling floor, a clean start after collapsed replies…).
- **MEASURED after the 2026-10-02 review of the F1 commits, with the
  preexisting safety changes in the tree:** Rust 1,438 reported passed / 5
  ignored (one Docker skip); formatting and Clippy pass; desktop 107 and
  sidecar 41 at the executor cycle. No new CI or live-model check.
- **Unproven:** that the harness makes a small model resolve more tasks than a
  simple loop. Development runs on the app's path (2026-10-01) show a signal on
  one deployment (3/8 → 6/8, one trial) and clear failures on hard tasks; no
  confirmatory held-out comparison is recorded. The minimal control arm now
  exists; experimental proposals measurements have mixed model/task outcomes
  ([plan W2.9](plan/implementation-plan.md)).

## Milestones

<!-- generated:milestones -->
<!-- Generated from docs/milestones.json by scripts/milestones.py. Edit the manifest, not this table. -->

| Milestone | Status | Evidence recorded | What remains before advancement |
|---|---|---|---|
| G0 Documents match the code | **passed 2026-09-30** | Documentation rewritten from the code at 0776ff4f (plan W0.1); every claim of the 2026-09-30 review verified (docs/reviews/2026-09-30-verification.md); older documents archived whole. | Keep it true: a change that alters described behaviour updates the document in the same commit. |
| G1 Effects are safe and honestly reported | **in progress: candidate validation pending** | IMPLEMENTED on develop: version-bound/atomic edits, command protections, Goal budgets, frozen acceptance, context/sampling correctness repairs, managed cancellation, zero-test reporting and structured conversation outcomes. Dated local suites are recorded in docs/reviews/ and docs/experiment-log.md; their counts do not certify a later candidate. Additional page/repair/runtime work landed through 2026-10-07. No candidate gate is advanced by the documentation review. | Freeze a candidate and obtain its complete local/hosted macOS checks, explicit skip coverage and native-app walk. Remaining work includes W3.2 (CI proposals), W1.10 (ordinary aggregate budget), W4.2 (exact preflight), W2.1 (scripted result migration) and documented lifecycle/evidence gaps. Release packaging/licence work is in the next-release checklist. |
| G2 One execution path | **in progress: shared product executor; scripted research path separate** | `executor::execute` runs the conversation and the goal for the app and the console, with the checks that close a turn (W2.3). The scripted loop (`pwr run`, `eval run`) keeps its own holds, compaction, recovery and catalogue; two of its completion holds, reasoning effort and the catalogue were aligned with the app (docs/plan/executor-parity.md, D-2026-09-30-6). The evaluator measures the scripted loop; the stack-matrix runner drives the app's own protocol. | IMPLEMENTED product-path runner tooling: engine lease/provenance, frozen splits, minimal control and paired/power analysis. Still needed: validated competitor endpoints/arms, controlled campaigns and the declared scripted-result migration (W2.1). The runner is the designated product-path evaluator (D-2026-10-02-1); no EvalHost migration is implied. |
| G3 All-tier mission exit before Windows | **not started** | PLANNED under the owner-approved contract of 2026-10-01 (D-2026-10-01-2). No product-path confirmatory comparison or all-tier baseline exists. | W8.3 same-engine simple loop; W2.4/G2 parity; initial S/M/L/XL baseline and prospective power; owner-approved numerical thresholds; frozen W8.4/F6 against the simple loop and at least two eligible competitors per tier, with all capacity, daily-use, lightness, reliability and audit criteria satisfied; W8.5 publish evidence. |

Campaign evidence: no confirmatory current-product comparison against a simple loop or competitors is recorded. Historical paired R2 (2026-09-15, scripted loop) found 11/30 vs 11/30 on one deployment and 13 vs 18 on another (p = 0.227). Product-path development runs on 2026-10-01 showed 3/8 to 6/8 final outcomes on one deployment, one trial per task; limited proposals measurements on 2026-10-06 had mixed model/task results (plan W2.9). These do not establish general uplift. Raw artifacts remain unpublished; candidate evidence is tracked separately in docs/release/next-release-readiness.md.
<!-- /generated:milestones -->

## NOW — safe effects and honest results

| Item | What | State |
|---|---|---|
| W0.2 | Tests say what they exercised; the Docker test tells a stopped daemon from a failure | **Done** |
| W1.1, W1.2 | An overwrite is bound to the version the model read; writes are atomic and re-checked | **Done** |
| W1.3 | Protected paths and dependency trees enforced for commands and checks, not only file tools | On `develop`; CI/macOS pending |
| W1.4 | Goal-wide actions, refusals, verification/review caps and a deadline | On `develop`; CI pending |
| W1.5 | Goal progress judged by failure fingerprints, not check names | On `develop` (six toolchains) |
| W1.6–W1.8 | Bounded PDF inflation, bounded repository walk, a timeout on the embedding sidecar | On `develop` |
| W1.9 | Each grant says what it actually opens | On `develop`; native walk pending |
| W2.1, W2.2 | One structured outcome; the check mark says what the checks said | W2.2 on `develop`; W2.1 partial (the scripted runner is not migrated) |
| W3.1, W3.4 | Acceptance artifacts frozen; unverifiable completion said the same way everywhere | W3.1 on `develop`; **W3.4 open** (`complete`'s description still promises verification) |
| W4.1, W4.5 | The objective never compressed; framework guidance from the real catalogue | W4.1 on `develop`; W4.5 open |
| W5.2 | Background summaries off by default and pre-emptible | On `develop` |
| W7.2, W7.4, W7.5 | 3D graph out of the default product; confinement on every turn; graphical defects | W7.2, W7.4 on `develop`; W7.5 ongoing |
| W8.1, W8.2 | Strict pairing by default; unattended success reported apart from nudged success | W8.1 on `develop`; **W8.2 open** |
| W9.1 | Releases built from a commit that passed the CI gates | Implemented; validate for the selected candidate |

Gate **G1** closes this phase: it needs a push and a green CI run.

## NEXT — one path, then the decisive benchmark

| Item | What | State |
|---|---|---|
| W2.3, W2.4 | One session executor for the app, the CLI and the evaluator; the scripted loop converges onto it | W2.3 on `develop`; **W2.4 in progress** |
| W2.5–W2.7 | Window arithmetic out of the orchestrator; one recovery budget; typed roles and sampling | | W2.5 on `develop`; W2.6, W2.7 open |
| W3.2, W3.3, W3.5 | CI-derived checks only proposed; flakiness by failure fingerprint; zero-test detection for every toolchain | | W3.3 on `develop`; W3.2, W3.5 partial |
| W4.2–W4.4, W4.6 | Real prompt counting; refusal of over-budget prompts; retrievable large output; visible retrieval failures | | W4.3, W4.6 partial; W4.2, W4.4 open |
| W5.1, W5.3, W5.5 | Stop during prefill; fit checked before loading; engine occupancy shown | | W5.1 on `develop`; W5.3, W5.5 open (prefill progress is shown since 2026-10-01) |
| W6.1–W6.4 | A transactional journal; the state ownership map enforced; bounded snapshots; resume with reconciled effects | | W6.1 on `develop`; W6.2–W6.4 open |
| W7.1, W7.3, W7.7 | Outcome-first turn view; Advanced behind Advanced; gated models and disk space in the Model Manager | | W7.1 partial; W7.3, W7.7 open |
| W8.3, W8.4, W8.5, W8.7 | The simple loop; the confirmatory campaign; published evidence; stress tests and the agent-mode model matrix | | open — the heart of G3; the stack-matrix runner and a 22-model compatibility sweep are the groundwork |
| W9.2, W9.3 | Locked engine environment; licence inventory | | open; newer MLX releases exist, see [distribution.md](distribution.md) |
| W10.1, W10.2, W10.4 | Selection/certification audit; a simpler wiki; god functions split along their decisions | | open |

Gate **G2** comes before confirmation; **G3** is the owner-approved all-tier
F6 exit criterion, not the former two-deployment decision. F1/F2 audit/research
and F3 infrastructure proceed together; correctness fixes do not wait for G3.

## LATER — measured extensions and Windows after the exit criterion

**PLANNED:** stable extension/permission interface, then web, documents, data,
browser, vision, memory, exploration subagents and background/computer work
only with objective task sets, threat models and measured value. Core token and
memory costs stay bounded. Runtime/context optimization hypotheses remain
subject to controlled comparisons, not blanket prohibition.

**PLANNED Windows:** persistent llama.cpp, cancellation/cache/template parity,
CUDA/Vulkan, command isolation, CI and real GPU testing only after G3/F6.
Core engine/sandbox interfaces remain portable now.

## NOT NOW

Automatic routing, richer 3D graphs, a marketplace, new PDF/OCR stack, enterprise
features and broad certification without evidence. No cloud-inference dependency,
no Windows implementation before the exit gate, no maximum-context target without
quality/latency measures. D-2026-10-01-2 supersedes the previous blanket exclusions
only within the new mission's measurement and permission requirements.
