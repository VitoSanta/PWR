# Evaluation

**Checked against `develop` at `0776ff4f`, 2026-09-30.** How PWR is measured,
what has been measured, and what has not. The methodology before this date is
in the [archive](archive/evaluation.md); the decisive campaign still to run is
specified in the [implementation plan, W8](plan/implementation-plan.md#w8--the-decisive-benchmark).

## The question

> At equal model, machine and budget, does PWR resolve more real tasks than a
> simple loop, or the same tasks with less time and fewer human interventions?

**Not answered.** The one paired comparison on record shows no uplift on one
deployment and an unconfirmed one on another, on the scripted loop, not the
app's path.

## The machinery

`crates/pwr-eval` and `pwr eval`:

- **Corpora** (`corpus/*.json`): frozen tasks with a statement, a workspace, a
  visible and a hidden verifier, and for many a reference solution and a wrong
  implementation that must fail. `pwr check-corpus` checks a corpus is fair
  before it is used. Present: `external-v1/v2`, `generation-v1`,
  `longhaul-v1`, `longhorizon-v1/v2`, `m5-frozen-v1`, `m6-hard-v1`,
  `realistic-v1` (it includes a deliberate prompt-injection file), `vague-v1`,
  `small-apps-v1`.
- **Arms** (`--arm`): `b1` is PWR's scripted loop; `b0` a conventional loop;
  `b2` a fixed localize–repair–validate workflow
  (`crates/pwr-orchestrator/src/baseline.rs`). All share tools, policy, reply
  handling and the event log.
- **Modes** (`--mode`): `verifier-supplied` (the default, and every existing
  report) hands the agent the corpus's visible check; `product-path` lets
  check discovery run as for a user. Scoring always uses the hidden verifier.
- **Other conditions**: `--context-policy current|recency-fill|evidence-state`,
  `--oracle-context` (a localization diagnostic), `--reasoning-effort`
  (Medium by default, `off` for the pre-2026-09-30 behaviour),
  repeated `--seed`s under one lease, `--resume` from an immutable manifest.
- **Comparison**: `pwr eval compare` pairs by deployment, task and seed;
  **permissively by default**, which lets campaigns that also changed corpus,
  sampling or hardware pair silently. `--strict` (implied by `--declare`)
  requires every differing field to be declared as the treatment, rejects
  double-recorded trials and counts every assigned trial in the denominator
  (`compare_strict`, `crates/pwr-eval/src/lib.rs:2883`). Plan W8.1 makes strict
  the default.
- **Thresholds**: `docs/thresholds.json` is what the code reads (`pwr-eval`
  includes it at build time); its reasoning and amendments are in
  [thresholds.md](thresholds.md). Bars on record: resolved task rate ≥ 0.4,
  hidden verification among declared = 1.0, scope respected = 1.0, safety
  violations = 0, harness failure rate ≤ 0.1, declined legitimate task = 0.
  They were set for the old research milestones; W8.4 adds its own by a dated
  amendment.
- **Regression suites** (`suites/a1`–`a4`: tool calls, navigation, editing,
  verification) run by `pwr eval suite`; they catch regressions, they do not
  measure uplift.

**The measured loop is not the shipped one.** `eval run` runs the scripted
loop; the app runs the conversation turn and Goal mode. Plan W2.4 and gate G2
fix this before the decisive campaign.

## The stack matrix

`evidence/stack-matrix/`: PWR on tasks across languages, frameworks, databases
and tools (31 task folders in the repository, further held-out tasks kept
outside it), **run the way the app runs** — `pwr serve --stdio`, one goal-mode
conversation per task, permission questions answered by a stand-in that grants
what the task names. The verdict comes from a clean copy with the owner's tests
restored and hidden tests laid over, run in the task's container. Tasks have
`dev` and `heldout` splits; a held-out task whose failure was read to change
PWR moves to `dev`, and every such move is listed in its README.

After a failed verification the runner sends the model a generic nudge ("the
work does not satisfy everything I asked") and tries again. That is an
intervention by an external oracle: a result after a nudge is not an
unattended success. Each turn is recorded, so first-cycle results are
recoverable; plan W8.2 reports them separately.

## What has been measured

All of it on the maintainer's M2 Max (64 GB); raw artifacts are git-ignored
under `experiments/` and `~/Desktop/pwr-evidence/`, so an outside reader
cannot re-check them yet (plan W8.5).

| Campaign | Loop | Result | What it shows |
|---|---|---|---|
| R2 rerun, 2026-09-15 (`experiments/r2-rerun-20260915-8648aed`) | scripted, B0 vs B1, 30 tasks | deployment `4420340fd319`: 11/30 vs 11/30, and B1 used more generated tokens (152,226 vs 106,034); deployment `5df73e30dde5`: 13 vs 18, sign test p = 0.227 | No uplift on one; a positive, unconfirmed signal on the other |
| R2 harness revision, 2026-09-16 | scripted | fewer tokens and minutes, but hidden checks passed 14 with the repairs vs 17 without | Fixing malformed output and reasoning can lower cost without raising capability |
| R3 development run, 2026-09-17 | scripted, old regime | 2/35 resolved, heavy re-read churn; stopped | The failure mode it targeted existed; not a measure of today's product |
| Stack matrix c4 (pinned `pwr-d313acd9`), recorded 2026-09-28 | app path, goal mode | 7 of 9 tasks passed; 5 of 6 held-out | One run per task; the verdict is the runner's final one, which may follow nudges; not a corpus score |
| Small apps, 2026-09-29/30 (`corpus/small-apps-v1.json`, 4 tasks) | scripted | gpt-oss-20b 3/4 → 4/4; Qwen3-14B 2/4 → 3/4; Ornith-1.5-9B 1/4 → 3/4; Qwen2.5-Coder-14B 0/4 → 1/4 (first vs last campaign) | Development runs, one trial each, harness changing between them; Qwen3-14B answered the same prompt four ways across four runs |

Campaigns recorded before 2026-09-03 are void (their tool-failure rate was
never measured and a workspace with no checks scored as resolved; see
[thresholds.md](thresholds.md) and the archived experiment log).

## Rules for a claim

- Per deployment, never pooled across deployments.
- Paired by task and seed; absolute difference in percentage points with an
  interval over tasks.
- Every assigned trial in the denominator, including backend failures.
- Interventions counted and reported; a nudged success is not unattended.
- False acceptance reported: the agent or PWR said done or verified, and the
  hidden verifier failed.
- Conditions pinned: binary, sidecar, model artifact, corpus revision,
  sampling, hardware.
- Removing a defect of the environment (a template bug, a malformed call) is
  reported as that, not as raising the model's capability.
- Negative and inconclusive results are recorded in the
  [experiment log](experiment-log.md).
