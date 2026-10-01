# Mission status

Updated 2026-10-01, initial F0 cycle. Resume from the owner's mission mandate,
this file, MASTER_SPEC, decisions and implementation plan. Evidence vocabulary
is defined in MASTER_SPEC; no gate or model-capability claim follows from this
status.

## Current phase / cycle

- **F0 adopted 2026-10-01**, owner approved ("ok procedi"). MASTER_SPEC and
  D-2026-10-01-2 are operative; prior contract archived whole. Current cycle:
  F1 correctness regressions and F2 source research, with F3 parity next.
- **IMPLEMENTED initial F1/F2 work artifacts:**
  `docs/reviews/2026-10-01-audit.md`, `docs/research/competitors.md` (partial).
  Full audit and exhaustive research remain PLANNED.
- Code baseline commit: `ae1e36c1e5dbe80f7fa3ee781072948b106a0208`, on `develop`,
  56 commits ahead of locally stored `origin/develop`; no fetch this cycle.
- Last completed cycle: this F0 docs commit (resolve with `git log -1 --
  docs/plan/mission-status.md`), after the recorded local checks. No push, gate
  completion, release or model download. G1/G2/G3 are not newly satisfied.

## Reproduced state

- **MEASURED** Rust suite exit 0: 1,372 reported passed, 5 ignored; one
  host-dependent Docker test skipped (included among reported successes).
- **MEASURED** formatting, Clippy with denied warnings, desktop 107 tests /
  13 files, production build, sidecar 41 tests, milestone-table agreement and
  diff whitespace checks pass. Hosted CI and clean npm installation unknown.
- **MEASURED** M2 Max, 64 GiB, about 239 GiB disk free. Docker absent; no active
  campaign in inspected processes; Ollama server has no loaded models.
- Initial product-path capability/performance by tier: **unknown**. No inference
  or timing run; machine not certified idle. No current heldout inspected.
- Logs/probes/source snapshots:
  `/Users/vitosantanelli/Desktop/pwr-evidence/logs/mission-20261001-f0/`.

## Preserve other sessions' work

Starting dirty files: `crates/pwr-tools/src/lib.rs`,
`crates/pwr-tools/tests/{adversarial,sandbox_and_approvals}.rs`, `docs/README.md`,
`docs/evaluation.md`, `docs/experiment-log.md`; starting untracked file:
`docs/research/semantic-decision-layer.md`. Reviewed and preserved. A starting
patch/status and research copy are in the evidence directory. Commit only this
cycle's reviewed hunks; do not accidentally absorb the process-safety work or
Semantic Decision Layer proposal. No source-code file changed in this cycle.

## Open findings / hypotheses

| Item | State / next evidence |
|---|---|
| A01 context grant after error | IMPLEMENTED defect, source verified; failing fake-provider test next (W4.8) |
| A02/A03 trigger vs capacity / Looping | IMPLEMENTED branch, source verified; behavioral reproduction next (W4.8) |
| A04 ignored tool arguments | IMPLEMENTED estimate gap, source verified; W4.2 failing 40 KB argument test |
| A05 sampling parser boundaries/mode | MEASURED pure probe accepts excluded recipes; failing regression then fix (W5.6) |
| A06 conversation-wide bound | unknown total bound in ordinary path; Goal bound IMPLEMENTED; W1.10 |
| A07 eval parity | PLANNED W2.4/W2.1; scripted eval still separate |
| A08 completion promise | IMPLEMENTED misleading description; W3.4 open |
| 32,768 ceiling / 100 actions / 0.6–0.95–20 sampling floor | HYPOTHESIS of benefit; values inspected, no baseline or default change |
| Same-model review / collapsed-reply recovery | HYPOTHESIS of benefit; no new causal evidence |
| Stable KV prefix / ranked map / tier edit format | HYPOTHESIS; controls and provisional dev rejection rules in plan W5.7/W4.9/W2.8 |
| Semantic Decision Layer | HYPOTHESIS from preexisting proposal; unimplemented/unmeasured; not selected for next cycle |

## Queue / next step

No campaign queued or launched. Existing external `queue.sh` serializes its
own model list but is not verified to prevent another independent queue or
engine. F3 needs an exclusive lease and recorded provenance before timing.

1. Commit the adopted F0 documents, preserving other sessions' hunks.
2. Record the adoption commit in the next cycle status update.
3. Correct A01–A05 with failing behavioral tests and minimal fixes; no behavioral
   default tuning. Continue F1/F2 source work and native UX checks as feasible.
4. Complete W2.4 and F3 baseline controls/provenance, power analysis, pinned
   dev split; arrange Docker and a serial idle/night slot for S/M/L/XL baseline.
5. Propose numeric exit thresholds from that baseline; owner approval before F6.

## Pending owner stops

- F0 approval received on 2026-10-01; no further approval needed to adopt it.
- F6 numerical thresholds later, before campaign; none proposed as adopted.
- Future visible feature removal or out-of-scope irreversible/external actions.
- Windows hardware question only when the approved exit criterion is reached.
