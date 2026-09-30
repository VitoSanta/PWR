# Roadmap

**As of 2026-09-30.** The order of work, in the review's phases. The detail
of every item — problem, evidence, change, acceptance, tests — is in the
[implementation plan](plan/implementation-plan.md); this page is the summary a
reader needs first. The roadmap before this date, with its reconciliations and
release plans, is in the [archive](archive/roadmap.md).

## Where things stand

- **Released:** v0.2.0-alpha (2026-09-28), macOS on Apple silicon, as a
  prerelease. Fixes since then are on `develop` and recorded in
  [release/v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md).
- **Verified today:** the Rust suite fails one test, for an environment reason
  (Docker Desktop off, stale socket); see the
  [verification](reviews/2026-09-30-verification.md#test-suite-reproduced).
- **Unproven:** that the harness makes a small model resolve more tasks than a
  simple loop. No campaign has measured the product path.

## Milestones

<!-- generated:milestones -->
<!-- Generated from docs/milestones.json by scripts/milestones.py. Edit the manifest, not this table. -->

| Milestone | Status | Evidence recorded | What remains before advancement |
|---|---|---|---|
| G0 Documents match the code | **passed 2026-09-30** | Documentation rewritten from the code at 0776ff4f (plan W0.1); every claim of the 2026-09-30 review verified (docs/reviews/2026-09-30-verification.md); older documents archived whole. | Keep it true: a change that alters described behaviour updates the document in the same commit. |
| G1 Effects are safe and honestly reported | **not started** | The defects are confirmed in the code: execution-time overwrite hash, non-atomic writes, protections missing from the command sandbox, a goal budget on one branch, acceptance frozen to checks.json only, 800/200-character objective in compaction, an unconditional check mark. | W0.2, W1.1-W1.6, W1.9, W2.2, W3.1, W4.1 done, each with a regression test; Rust, desktop and sidecar suites green on CI with skips reported. |
| G2 One execution path | **not started** | Three semantics today: the chat turn with post-turn checks (pwr-cli main.rs), Goal mode (serve.rs), the scripted loop (orchestrator lib.rs). The evaluator measures the scripted loop. | W2.1, W2.3, W2.4 done; eval run measures the executor the app uses; the parity test lists no undeclared difference. |
| G3 The decision on the adaptive layer | **not started** | None on the product path. | W8.3 simple loop defined; W8.4 preregistered and run on a 9B and a 14B deployment with new tasks; the decision rule applied and recorded in docs/decisions.md. |

Campaign evidence: no campaign has measured the current product path. The last paired comparison (R2 rerun, 2026-09-15, scripted loop) found no uplift on one deployment (11/30 vs 11/30) and an unconfirmed one on another (18 vs 13, p = 0.227); the small-model campaigns of 2026-09-29/30 are development runs, one trial per task (docs/release/v0.2.x-mac-verification.md). All of it stays on the maintainer's machine under experiments/.
<!-- /generated:milestones -->

## NOW — safe effects and honest results

| Item | What |
|---|---|
| W0.2 | **Done.** Tests say what they exercised; the Docker test tells a stopped daemon from a failure |
| W1.1 | An overwrite is bound to the version the model read, never the current one |
| W1.2 | Atomic, checked writes for every file-writing tool |
| W1.3 | Protected paths and dependency trees enforced for commands and checks, not only file tools |
| W1.4 | A goal budget (actions, refused completions, time) that bounds every branch |
| W1.5 | Goal progress judged by failure fingerprints, not check names |
| W1.6–W1.8 | Bounded PDF inflation, bounded repository walk, a timeout on the embedding sidecar |
| W1.9 | Each grant says what it actually opens |
| W2.1, W2.2 | One structured outcome; the check mark says what the checks said |
| W3.1, W3.4 | Acceptance artifacts frozen; unverifiable completion said the same way everywhere |
| W4.1, W4.5 | The objective never compressed; framework guidance from the real catalogue |
| W5.2 | Background summaries off by default and pre-emptible |
| W7.2, W7.4, W7.5 | 3D graph out of the default product; confinement on every turn; graphical defects |
| W8.1, W8.2 | Strict pairing by default; unattended success reported apart from nudged success |
| W9.1 | Releases built from a commit that passed the CI gates |

Gate **G1** closes this phase.

## NEXT — one path, then the decisive benchmark

| Item | What |
|---|---|
| W2.3, W2.4 | One session executor for the app, the CLI and the evaluator; the scripted loop converges onto it |
| W2.5–W2.7 | Window arithmetic out of the orchestrator; one recovery budget; typed roles and sampling |
| W3.2, W3.3, W3.5 | CI-derived checks only proposed; flakiness by failure fingerprint; zero-test detection for every toolchain |
| W4.2–W4.4, W4.6 | Real prompt counting; refusal of over-budget prompts; retrievable large output; visible retrieval failures |
| W5.1, W5.3, W5.5 | Stop during prefill; fit checked before loading; engine occupancy shown |
| W6.1–W6.4 | A transactional journal; the state ownership map enforced; bounded snapshots; resume with reconciled effects |
| W7.1, W7.3, W7.7 | Outcome-first turn view; Advanced behind Advanced; gated models and disk space in the Model Manager |
| W8.3, W8.4, W8.5, W8.7 | The simple loop; the confirmatory campaign; published evidence; stress tests and the agent-mode model matrix |
| W9.2, W9.3 | Locked engine environment; licence inventory |
| W10.1, W10.2, W10.4 | Selection/certification audit; a simpler wiki; god functions split along their decisions |

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
