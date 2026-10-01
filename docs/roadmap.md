# Roadmap

**As of 2026-10-01.** The order of work, in the review's phases. The detail
of every item — problem, evidence, change, acceptance, tests — is in the
[implementation plan](plan/implementation-plan.md); this page is the summary a
reader needs first. The roadmap before this date, with its reconciliations and
release plans, is in the [archive](archive/roadmap.md).

## Where things stand

- **Released:** v0.2.0-alpha (2026-09-28), macOS on Apple silicon, as a
  prerelease. Fixes since then are on `develop` and recorded in
  [release/v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md).
- **On `develop`, not released and not pushed:** 49 commits since the
  documentation baseline (`0776ff4f`): most of gate G1, the session executor, and
  the model-compatibility and recovery work of 2026-10-01 (adapters for the
  Mistral, Liquid and Granite families and a Qwen3-Coder format, tool-call ids
  for Gemma, a sampling floor, a clean start after collapsed replies…).
- **Verified today (local only, no CI run since 2026-09-30):** Rust 1,364
  passed / 5 ignored, desktop 107, sidecar 39; all 22 models installed on the
  maintainer's Mac pass the critical Quick Calibration checks.
- **Unproven:** that the harness makes a small model resolve more tasks than a
  simple loop. Development runs on the app's path (2026-10-01) show a signal on
  one deployment (3/8 → 6/8, one trial) and clear failures on hard tasks; no
  `heldout` task has been run on the current harness, and no baseline arm exists
  yet.

## Milestones

<!-- generated:milestones -->
<!-- Generated from docs/milestones.json by scripts/milestones.py. Edit the manifest, not this table. -->

| Milestone | Status | Evidence recorded | What remains before advancement |
|---|---|---|---|
| G0 Documents match the code | **passed 2026-09-30** | Documentation rewritten from the code at 0776ff4f (plan W0.1); every claim of the 2026-09-30 review verified (docs/reviews/2026-09-30-verification.md); older documents archived whole. | Keep it true: a change that alters described behaviour updates the document in the same commit. |
| G1 Effects are safe and honestly reported | **in progress: implemented on develop, CI not run** | Implemented on develop with regression tests (not pushed, CI not run): execution-time overwrite hash fixed, atomic writes, protections in the command sandbox, a goal budget on every branch, acceptance artifacts frozen, the objective kept whole in compaction, marks that follow verdicts, zero-test signatures for six toolchains, failure fingerprints. Local suites on 2026-10-01: Rust 1,364 passed / 5 ignored, desktop 107, sidecar 39. Still open: W3.2 (CI proposals), W3.4 (the `complete` promise), W4.2 (exact preflight), W4.4, W2.1 (scripted runner not migrated). | A push and a green CI run on macOS (the first since 2026-09-30); the native-app walk; the open items above. |
| G2 One execution path | **in progress: the app, the console and Goal mode share one executor; the evaluator does not** | `executor::execute` runs the conversation and the goal for the app and the console, with the checks that close a turn (W2.3). The scripted loop (`pwr run`, `eval run`) keeps its own holds, compaction, recovery and catalogue; two of its completion holds, reasoning effort and the catalogue were aligned with the app (docs/plan/executor-parity.md, D-2026-09-30-6). The evaluator measures the scripted loop; the stack-matrix runner drives the app's own protocol. | W2.4: `EvalHost` so `eval run` calls the executor, the `legacy` arm for older campaigns, `pwr run` as a thin call; W2.1 for the scripted runner's result; the parity test lists no undeclared difference. |
| G3 The decision on the adaptive layer | **not started** | None on the product path. | W8.3 simple loop defined; W8.4 preregistered and run on a 9B and a 14B deployment with new tasks; the decision rule applied and recorded in docs/decisions.md. |

Campaign evidence: no campaign has measured the current product path against a baseline. The last paired comparison (R2 rerun, 2026-09-15, scripted loop) found no uplift on one deployment (11/30 vs 11/30) and an unconfirmed one on another (18 vs 13, p = 0.227). On the app's own path (stack matrix, 2026-10-01, development runs, one trial per task) Qwen3.6-35B-A3B went from 3/8 to 6/8 `dev` tasks after the review round also ran without a declared acceptance check: a signal, not a result; no `heldout` task has been run on the current harness (docs/evaluation.md). All of it stays on the maintainer's machine under experiments/ and ~/Desktop/pwr-evidence/.
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
| W9.1 | Releases built from a commit that passed the CI gates | On `develop`; never run |

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

Gate **G2** comes before the campaign; gate **G3** is its decision.

## LATER — only what the decision supports

Semantic retrieval and evidence-state compaction if they beat the simple
alternatives; a complete llama.cpp path and then a second operating system
(Windows) with its own command isolation; vision for selected UI tasks with an
independent browser check; documentation retrieval and KV-cache quantization
experiments; a desktop end-to-end test; notarization.

## NOT NOW

Multi-agent execution, automatic model routing, a richer 3D graph, universal
memory, critic/consensus calls, a general browser or computer agent, a
marketplace, Windows/Linux parity now, a new PDF/OCR stack, maximum-context
optimisation, a full certification system, enterprise features. Reasons in
[decisions.md](decisions.md#d-2026-09-30-3--what-is-not-built-now).
