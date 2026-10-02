# Mission status

Updated 2026-10-02, review of the F1 commits (second reviewer). Resume from the owner's
mission mandate, this file, MASTER_SPEC, decisions and implementation plan.
Evidence vocabulary is defined in MASTER_SPEC; no gate/model-capability claim
follows from this status.

## Current phase / cycle

- **F0 adopted**, owner approved ("ok procedi"), commit `b9db2eab`.
  MASTER_SPEC and D-2026-10-01-2 are operative; prior contract archived whole.
- **IMPLEMENTED F1 context correctness cycle:** A01–A04 and adjacent count,
  no-op compaction and audit fixes, with failing-before-fix regressions.
  W4.8/W4.2 remain PARTIAL until eval parity/exact preflight.
- **IMPLEMENTED F1 A05 correction, W5.6 PARTIAL:** scoped/mode-safe parser,
  schema 4 pinned cache and 19 new regressions. UI/chat/eval deliberately use
  unknown mode; per-generation selection after reasoning planning is PLANNED.
- **IMPLEMENTED F1 A06/A08 repair:** configured response deadline and managed
  cancellation, Stop during permissions/checks/Goal review, EOF teardown,
  honest empty/zero-test evidence and policy-specific completion catalogues.
  W3.4 implemented; aggregate ordinary bounds W1.10 remain PARTIAL/PLANNED.
- **Review of F1 commits, 2026-10-02:** checks reproduced (Rust 1,424 / 5
  ignored / Docker skip, fmt, Clippy). Two behaviour changes corrected with
  failing-before regressions: the response timeout bounds silence, not length
  (absolute deadline retried a healthy long reply into *backend failing*; a
  measured reply took 799 s); an unreachable compaction trigger stops counting
  (a five-read turn stopped after three on its compaction budget). See the
  audit's review section and the experiment log.
- **D-2026-10-02-1 (owner's choice):** the stack-matrix runner is the
  product-path evaluator; the planned `EvalHost` is not built. Runner engine
  lease, provenance, frozen split manifest and paired analysis/power
  IMPLEMENTED with tests; minimal-control arm and competitors next. The
  2026-10-01 "3/8 -> 6/8" reads as p = 0.375 (first cycle 3/8 -> 4/8).
- **IMPLEMENTED F2 research artifact, partial:** competitors.md now includes
  opened Lost in the Middle/RULER, pinned Qwen3.6 mode and mlx-lm renderer
  sources. PWR effects remain unknown; full survey/audit remain PLANNED.
- Cycle parent: `c24028f5` on `develop` (executor cycle); this review
  commit is resolved by `git log -1 -- docs/plan/mission-status.md`. Context
  `84b4654c` and sampling `312070c6` follow F0 `b9db2eab`. No fetch or push.
- Initial code baseline: `ae1e36c1e5dbe80f7fa3ee781072948b106a0208`.
  G1/G2/G3 are not newly satisfied; no release or model download.

## Reproduced checks / limits

- **MEASURED** final Rust suite: 1,424 reported passed, 0 failed,
  5 ignored; one host-dependent Docker test skipped (among reported passes).
- **MEASURED** Clippy with denied warnings, formatting, desktop 107 tests /
  13 files, production build, sidecar 41 tests, milestone-table agreement and
  diff whitespace checks pass. Hosted CI / clean npm installation unknown.
- **MEASURED** 19 new executor-cycle tests (including concurrent stress),
  plus expanded fixtures, cover cancellation, deadlines and honest verification.
  The prior context fixtures cover preparation failure,
  real Settings dispatch mutation/read-only recovery, objective over trigger,
  true overflow, physical-fit recovery, large call budgeting, fixed overhead,
  stale history count, productive compaction budget and audit event counts.
- Existing floor/profile values, 32,768 ceiling / 100 actions and recovery
  are unchanged. Rejecting unsafe cached card recipes can change sampling;
  no task-capability effect has been measured.
  Two automatic compactions remain a harness budget, not a loop diagnosis;
  Goal continuation policy is unchanged. Token estimates remain heuristics.
- Hardware from F0: **MEASURED** M2 Max, 64 GiB, about 239 GiB disk free.
  Docker socket absent. No inference/timing run or current heldout inspection;
  product-path capability/performance by tier and effective windows unknown.
- Evidence: `~/Desktop/pwr-evidence/logs/mission-20261001-f0/` and
  `~/Desktop/pwr-evidence/logs/mission-20261001-f1-context/` and
  `~/Desktop/pwr-evidence/logs/mission-20261001-f1-sampling/` and
  `~/Desktop/pwr-evidence/logs/mission-20261001-f1-executor/`.

## Preserve other sessions' work

Starting modified files remain uncommitted: `crates/pwr-tools/src/lib.rs`,
`crates/pwr-tools/tests/{adversarial,sandbox_and_approvals}.rs`, `docs/README.md`,
`docs/evaluation.md`, and the starting Semantic Decision Layer entry in
`docs/experiment-log.md`; starting untracked research:
`docs/research/semantic-decision-layer.md`. Tool diffs checked identical to the
F0 saved patch. Suites include these hunks; commit only this cycle's work.

## Open findings / hypotheses

| Item | State / next evidence |
|---|---|
| A01 context grant after error | IMPLEMENTED correction; MEASURED fake-provider and real Settings regressions |
| A02 trigger vs capacity | IMPLEMENTED correction + review fix (unreachable trigger stops counting); MEASURED regressions; effective model capacity unknown |
| A03 productive compaction labelled Looping | IMPLEMENTED budget classification; MEASURED distinct-read regression; cap remains 2 |
| A04 ignored tool arguments | IMPLEMENTED estimate correction; MEASURED 40 KiB write; exact preflight PLANNED W4.2 |
| A05 sampling boundaries/mode | IMPLEMENTED correction + MEASURED regressions; dynamic per-generation selection PLANNED W5.6 |
| A06 conversation-wide bound | response silence bound/managed cancel and Stop repairs IMPLEMENTED; ordinary total bound PLANNED W1.10; worker latency unknown |
| A07 eval parity | PLANNED W2.4/W2.1; scripted eval still separate |
| A08 completion promise | IMPLEMENTED correction W3.4; empty/zero-test evidence cannot claim verified; catalogue/evidence regressions |
| Ceiling/actions/sampling floor | HYPOTHESIS of benefit; no product-path baseline/default tuning |
| Same-model review / collapsed-reply recovery | HYPOTHESIS; no new causal evidence |
| Stable prefix / ranked map / tier edit format | HYPOTHESIS; controls/dev rejection rules W5.7/W4.9/W2.8 |
| Semantic Decision Layer | HYPOTHESIS from preexisting proposal; unimplemented/unmeasured; not selected |

## Queue / next step

No campaign queued or launched. External queue.sh serializes its own model
list, but is not verified to exclude another queue/engine. F3 needs a lease
and recorded provenance before timing; machine is not certified idle.

1. Continue the remaining F1 audit, tests first for demonstrated defects;
   no default tuning. (Goal-deadline transcript: IMPLEMENTED 2026-10-02.)
2. Runner (W2.4 by D-2026-10-02-1): W8.3 minimal control as an arm, competitors (owner approves
   installs); then W5.6 per-generation selection; continue the F2 survey.
3. Arrange Docker and a serial idle/night slot for S/M/L/XL baselines.
4. Propose exit thresholds from baseline; owner approval before F6.

## Pending owner stops

F0 approval received; no further contract approval pending. F6 numerical
thresholds later; future visible-feature removal or out-of-scope irreversible/
external actions; Windows hardware only after the approved exit criterion.
