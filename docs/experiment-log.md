# Experiment log

Dated entries, newest first, for every experiment and every change that
alters what a campaign measures (prompt, catalogue, loop, budgets, adapters).
Each entry names: the hypothesis or question, the conditions compared (commit,
deployment, corpus revision, arm, seeds), the outcome with counts —
including an inconclusive or negative one — and what was kept, revised or
removed. Raw artifacts are cited by path even when they are not public.

Entries through 2026-09-17 are in the [archived log](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/experiment-log.md);
the archived [roadmap](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/roadmap.md) and [backlog](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/backlog.md)
hold the campaign notes of 2026-09-18 to 2026-09-28.

---

## 2026-10-02 — Preserve the core contract after the Stage frontend rollback

**IMPLEMENTED frontend compatibility repairs; diagnostic battery in progress.**
Comparing the restored frontend with the pre-rollback `167bf7f7` identified
functional regressions beyond the visual changes. Stage's CSS, layout, assets
and dependencies remain intact. Memory proposals now retain their originating
workspace and scope, show that origin, reject unknown workspace origins and
never populate another workspace's memory view. Two restored regressions fail
before and pass after repair.

The frontend again reads typed verification/confinement evidence, distinguishes
unavailable, zero-test and failed checks, retains terminal retry/continue
controls and marks an undeclared acceptance contract as unverified. Named
acceptance changes offer the core's human authorization dialog; a refusal
keeps them blocked. Repeated review clicks do not queue duplicate dialogs, and
a response from an old session cannot populate a new session. Streamed replies
are replaced by the authoritative final reply with its verification verdict.
Core prefill progress appears within Stage's existing working indicator.

The current Stage terminal stays a single shell. Its bounded plain-text buffer
is again available through the native client's read-only terminal capability,
only after the core's explicit TerminalRead approval (including in Full).
Requests from other sessions and buffers from other workspaces return empty.
Closing while shell startup is pending invalidates that startup and closes a
late-created shell. These behaviors are covered by protocol/buffer and
controlled asynchronous regressions; they do not establish native timing
coverage. The first imported memory regression referenced a type absent on
Stage; its test-only cast allowed the actual behavioral red run, and the type
fixture failure is retained separately.

Frontend validation: **103 tests in 15 files pass**, with the behavioral red
logs retained. Production build and `git diff --check` pass. There is no core,
sidecar or sampling change in this repair, so the real-model battery remains
pinned to `7d024539`. Frontend builds/tests overlap the diagnostic trials;
latency comparisons are not claimed. A native walk of `7d024539` already
confirmed Goal and Full → Auto → Ask / Full → Ask against the saved core
configuration, using empty model roots and a disposable workspace. The app
quit cleanly and personal last-workspace/trust files were restored. The new
frontend bundle still needs its native walk.

First corrected observations: all three fresh Quick Calibrations pass their
agent-critical checks. Qwen3.5-9B's owner-acceptance ledger trial passes the
independent verifier in 19.4 minutes, 40 actions and eight failed/refused
attempts, but **Goal is unverified**: two added test files changed the frozen
acceptance contract and require human review. Gemma 4 12B fails the same trial
in 2.8 minutes with two actions and protocol/repetition termination. The 30B
trial is still running. These are single unseeded development observations,
not model rankings or an isolated before/after improvement claim; the owner
acceptance declaration differs from the earlier ledger fixture.

Raw logs, native walk, source/model manifests and trial records:
`~/Desktop/pwr-evidence/batteries/20261002-stage-frontend/` and
`~/Desktop/pwr-evidence/runs/battery-20261002-fixed-*/`.

## 2026-10-02 — Ask before a dependency installer writes its protected tree

**IMPLEMENTED; real-model follow-up pending.** During the pre-fix 9B React
trial on `8b3b33dd`, `npm install` first timed out offline, then failed with
EPERM on node_modules after network permission was granted. No dependency
question was offered. The model tried shell commands and another package
manager, then hit repeat guards. A real offline local npm install reproduced
the missing gate; the command-classification and outside-sandbox gate
regressions also failed before repair.

Explicit npm/pnpm/Yarn/Bun/Composer install/add/update/remove operations now
require DependencyChange before execution. An outside-sandbox installer asks
for both rights. Builds and tests are unchanged; no sandbox boundary is widened
and unrecognised forms still face installed-tree protection. The actual local
install is denied without the grant and passes inside Seatbelt with it. Four
focused tools tests and all twelve orchestrator approval tests pass. This
advances W1.3. Broad Cargo validation reports 1492 passes, 0 failures and five
ignored. One Docker probe skipped when its daemon did not answer; the isolated
rerun exercised the actual permission boundary and passed without a skip.
Clippy with warnings denied passes. Corrected model trials are pending.

**IMPLEMENTED Stage/frontend compatibility repair (W7.4).** The restored
frontend mapped Full to Ask and offered no Full switch, although its composer
did report an unconfined sandbox. Preserve Stage's layout, CSS, assets and
dependencies, but retain Full in the store, restore its control using the
existing option style and distinguish it from a platform without a sandbox in
the composer. Two regressions failed on Stage: reported Full became Ask, and
the DOM had no Full option. All 82 frontend tests in 13 files now pass, including
Full → Auto → Ask and Full → Ask. The first DOM test wait did not track the
demo responder's delayed timer; polling the state resolved this test-fixture
issue, retained in `frontend-full-first-green-failed.log`. The baseline 30B Goal reply also exposed a completed-but-unverified summary
rendered as Finished; unverified goals now show a paused state with their
verification evidence, and changed acceptance contracts show a failure. Two
additional regressions failed before these repairs. The production frontend
build passes. Native walk remains pending. The initial rollback `8b3b33dd` remains an exact
frontend copy; the repaired version has these small functional differences.

Separately, 28 checks through the corrected `7943ce79` ACP binary passed:
file preview bounds and escape refusals, profile persistence, memory CRUD and
workspace isolation, wiki/project management and reported Ask/Auto/Full policy.
These use disposable homes/workspaces and no inference.

Campaign amendment: keep the three pre-fix ledger trials and the 9B React trial
already in flight; subsequent model trials use the corrected binary, fresh
calibration and unchanged frozen fixtures. A first continuation correctly
refused an occupied engine lease and did no inference. All intermediate logs
are retained under `~/Desktop/pwr-evidence/batteries/20261002-stage-frontend/`.

## 2026-10-02 — Repairs found during the desktop rollback battery

**IMPLEMENTED correctness repairs; model effectiveness still UNKNOWN.**
The live baseline remains pinned to `8b3b33dd`. The first 9B billing task
ended at the 60-minute Goal guard, unverified: 83 actions, 21 failed/refused,
and a failing independent verifier. This is a diagnostic observation, not a
model ranking. It exposed two core defects reproduced before repair:

- A failed command's live card showed stderr alone; saved cards also ignored
  stdout and its verbatim block after the JSON envelope. Live and replay now
  retain both streams. Regression cases use the actual result serializer.
- Repetition and failed-run guards refused new checks even after real input
  repairs. A successful file change now renews the command attempts; denied
  edits and existing-directory no-ops do not. The existing action, time and
  no-progress bounds remain. Real-tool regressions exercise identical failures,
  five distinct failures, edits, deletes, moves, directories and restoration.

The workspace baseline reported 1483 passes, 2 failures and 5 ignored. One
failure was a stale assertion on the Unparseable stop message; its check now
matches the supported tool-call formats. The other was the page-capture path
with desktop Chrome 154 on macOS 27.0.1. Controlled probes found full Chrome
works in Full access, while the installed Chromium headless shell captures the
same HTTP 500, DOM and console under all three tested execution environments.
macOS now prefers an already installed Playwright headless shell; an explicit
`PWR_BROWSER` still wins, and full-browser fallback remains where no helper is
installed. No sandbox permission is widened and no browser is downloaded.
Fresh workspace validation reports 1489 passes, 0 failures and 5 ignored;
Clippy with warnings denied, formatting, 43 MLX-sidecar tests and the three
runner policy regressions pass. Host-dependent skips were not collected for
the full Cargo run, so its reported pass count is not an exhaustive coverage
claim. Corrected-model follow-up is still pending. One earlier parallel
browser trial captured the page but lost its separate HTTP-status probe;
the isolated default-browser rerun and fresh full suite pass. This transient
is retained in `tools-final-green.log` despite that log's historical name.

Raw red/green regressions, compatibility probes and baseline task results:
`~/Desktop/pwr-evidence/batteries/20261002-stage-frontend/` and
`~/Desktop/pwr-evidence/runs/battery-20261002-stage-q35-9b/ts-ledger/`.

## 2026-10-02 — Desktop rollback battery, sandbox and Full access

**IMPLEMENTED measurement procedure; live battery in progress, effectiveness
UNKNOWN.** At the owner's request, the Angular frontend was restored exactly
from `stage` (`c44e1dce`) on `develop` (`8b3b33dd`), retaining the Rust core.
The product-path evaluator now accepts `--permission-mode ask|auto|full` and
records the core's actual policy and confinement in `provenance.permissions`.
It refuses policy mismatches and reuse of a completed trial under another
policy. Ask remains the default. Regression checks exercise all three policies
against a stand-in core, refuse wrong reported confinement before prompting
and refuse relabelling a legacy Ask result as Full.

The diagnostic battery pins the binary and sidecar at `8b3b33dd`, uses existing
local MLX artifacts Qwen3.5-9B 4-bit, Gemma 4 12B 4-bit and Qwen3-Coder-30B-A3B
4-bit, and MLX 0.32.3 / mlx-lm 0.31.3 on the maintainer's M2 Max, 64 GB.
All three fresh Quick Calibrations pass the agent-critical checks. Four frozen
development tasks cover debugging, React, Python and Rust; two external
development tasks start without application code and have owner acceptance
tests and separate hidden verifiers (Python todo CLI, Node HTTP notes API).
All six seed/reference pairs were validated: seed fails, reference passes.
Goal mode runs one unattended cycle per task, without oracle nudges or fixed
sampling seeds. The expanded matrix pairs debugging under Ask/Full and a new HTTP API under
Auto/Full, and probes session lifecycle under Ask/Full. Auto grants the
loopback network permission needed by the HTTP acceptance tests while keeping
the sandbox active; Full automatically grants permission and disables it. No heldout task
is read for tuning and no statistical model-ranking claim is planned.

The first trial predates the runner's additional policy metadata; its explicit
Ask request is preserved in its transcript. Later runner digests name the
metadata extension. Native bundle and runner preparation plus workspace tests
overlap the first diagnostic trial; latency comparisons are not claimed.
Raw artifacts and the battery manifest:
`/Users/vitosantanelli/Desktop/pwr-evidence/batteries/20261002-stage-frontend/`;
task results: `~/Desktop/pwr-evidence/runs/battery-20261002-stage-*/`.
New native bundle:
`~/Desktop/pwr-evidence/builds/stage-frontend-20261002/PWR.app`.
The restored Stage frontend exposes Ask/Auto; Full is tested through ACP.

## 2026-10-02 — Core effects, parsing, memory and real MLX context bounds

**IMPLEMENTED correctness repair; capability and performance effects UNKNOWN.**
Owner authorized applying the core audit before their manual test and a later
battery. Parent `95e7bed020a0c519338d03787254987d620f4fed`; no model inference or
campaign. C01–C14 cover durable intent identities, uncertain command effects,
no automatic permission replay, complete tool transcripts, conservative Qwen
and Mistral parsing, bounded/origin-safe memory, current source hashes and an
actual-token preflight before MLX prefill. An additional reproduced clock
rollback defect (C15) is fixed without changing UUID format or dependency pins.
The numbered-command reader also refuses unknown/duplicate numeric members and
preserves quoted argv and policy fields. Parser revisions: qwen-v4/mistral-v2.

**MEASURED:** fresh full workspace: 1452 reported passed, 0 failed, 5
ignored; one reported pass is a host-dependent Docker skip (no engine socket),
confirmed separately with nocapture. Clippy with warnings denied, formatting,
desktop 109 tests / 14 files and production build, pinned sidecar 43 tests,
milestone agreement and whitespace checks pass. Red parser/effect/sidecar,
numbered-field and clock-sequence regressions precede their fixes; the earlier
full-suite ID failure and its IDs/seed are retained. Hosted CI and a clean
installation remain unknown.

Audit and manual-test cases: [core audit](reviews/2026-10-02-core-audit.md).
Raw evidence: `/Users/vitosantanelli/Desktop/pwr-evidence/logs/core-fixes-20261002/`.
Prepared local application: `/Users/vitosantanelli/Desktop/pwr-evidence/builds/core-fixes-20261002/PWR.app`;
its core/sidecar/bundle hashes and exact source patch are saved in that folder.
The build includes the pre-existing uncommitted tool sandbox fixes; those
changes and the unrelated research/docs edits are preserved, not absorbed into
this correction commit. No push, public release, new tuning default or model
effectiveness claim. Owner manual feedback comes before the battery; stale
parser/sidecar calibration scope needs Quick Calibration of the chosen model.

## 2026-10-02 — The W8.3 minimal control, as a stack-matrix arm

**IMPLEMENTED experimental control; deterministic tests, no model run.** What
"PWR beats a simple loop" is measured against. The owner approved the
classification in W8.3: the control shares the deployment, sampling, reasoning
budget, tools, sandbox, approvals and goal budgets with PWR, and leaves out
PWR's instructions and retrieval, completion holds, repetition and stall guards,
reply recoveries, summarising compaction (it drops the oldest exchanges
instead), checks after a turn and Goal verification/review. It runs through the
same `take_turn` with `Continuity::harness = Minimal`, so the plumbing cannot
drift between arms; `pwr serve` takes `harness: "minimal"` and tags the reply,
and `run.py --arm minimal` sends it and stops on a mismatch. Default behaviour
is unchanged: every gate is open for `Harness::Full`. Regressions: four loop
cases (holds, reply recovery, compaction, repeated refusal) that pass under the
control and fail under the full harness; the executor's one budgeted, unverified
turn; the server's parameter and tag; the neutral prompt; whole-exchange
dropping; the runner's arm parameters. A first paired dev comparison needs the
Mac free and Docker running.

## 2026-10-02 — Paired analysis of stack-matrix runs; the 3/8 to 6/8 signal re-read

**IMPLEMENTED analysis; MEASURED re-reading of existing runs, no new run.**
`evidence/stack-matrix/runner/analyze.py` compares runs as arms on shared
(task, attempt) results, first cycle apart from nudged final outcomes, with the
MASTER_SPEC statistics (Wilson, bootstrap over tasks, exact McNemar or paired
sign-flip with repeats, Holm) and lists unequal provenance; `power` sizes a
campaign with the exact McNemar power. Applied to `fix2-q36-35b` (binary
`9385a010`) and `fix3-q36-35b` (`7afe9fdc`), Qwen3.6-35B-A3B, 8 dev tasks, one
trial: first cycle 3/8 vs 4/8 (p = 1.0); final 3/8 vs 6/8, difference -0.375,
bootstrap 95% [-0.875, 0.125], exact McNemar p = 0.375. The review-round change
of 2026-10-01 stays a hypothesis; most of its gain came after nudges. Tests:
`test_analyze.py` (hand-worked values; power within 15% above Connor's
approximation).

## 2026-10-02 — Stack-matrix tasks frozen by split and digest

**IMPLEMENTED measurement procedure.** `evidence/stack-matrix/splits.json`
records every task's split and digest (20 dev, 11 heldout); `run.py run`
refuses tasks that are missing, moved or changed, unless `--allow-unfrozen`,
which each result records under `provenance.splits`. A refreeze needs a reason
and keeps the previous freeze and the changed tasks in its history. Computing
digests reads no task for tuning; no heldout content was inspected.

## 2026-10-02 — Stack-matrix runner: engine lease and run provenance

**IMPLEMENTED measurement procedure, no campaign run.** By D-2026-10-02-1 the
runner is the product-path evaluator. A campaign now takes an exclusive lease
(`~/Desktop/pwr-evidence/engine.lock`) and, before each task, stops if another
inference engine is running after a minute's wait (`--allow-busy-machine`
overrides and is recorded): the two-engine runs of 2026-10-01 were discarded by
hand, and a lease makes the runner refuse them instead. Each `result.json`
carries `provenance` (binary and runner digests, artifact revision and
quantization, engine libraries, effective sampling with sources, granted
window, machine, load at start and end, engines seen, seed none). Results from
before this entry lack the block; they are not compared on fields they do not
carry. Tests: `evidence/stack-matrix/runner/test_*.py` (9), in CI.

## 2026-10-02 — Goal deadline keeps the history of the turn it ends

**IMPLEMENTED correctness repair, not a capability experiment.** When a goal's
wall-clock limit passed during a turn, the executor dropped the turn and its
messages: the edits it had made stayed on disk while the session's history
went back to before the turn, so a later prompt (a person's, or a campaign
nudge) continued without them. The turn is now asked to stop and given
`GOAL_STOP_GRACE` (10 s) to return its history, which the session keeps; the
goal still ends out of budget, and a turn that does not return in the grace is
dropped as before. Changes what a run continued after a deadline sees; the
deadline itself, the 3,600-second default and the action counts are unchanged.
Regressions (paused clock): `the_goal_deadline_keeps_the_transcript_of_the_turn_it_ends`
(failed before), `a_turn_that_ignores_the_deadline_is_abandoned_after_a_grace`.
Fixture correction, not a weaker assertion: the `serve.rs` deadline test's fake
turn slept 120 s ignoring Stop; it now ends when asked, as a real turn does, and
still reports the time limit at 2 s. Local: fmt, Clippy, Rust 1,429 passed /
5 ignored / one Docker skip.

## 2026-10-02 — Review of the F1 commits: response bound on silence, unreachable compaction trigger

**IMPLEMENTED corrections of two behaviour changes made at `c24028f5` and
`84b4654c`; MEASURED deterministic regressions, no model experiment.** A
review of the four F1 commits reproduced their checks (Rust 1,424 passed /
5 ignored / one Docker skip, fmt, Clippy) and found two changes to what a
campaign measures that no measurement supported.

- **Response timeout.** `c24028f5` made the 900-second response timeout an
  absolute deadline over opening and streaming. A timeout is a backend fault,
  retried three times: a fixture streaming for longer than its bound ended as
  *backend failing* after three full deadlines, "this is the server, not the
  model". On 2026-10-01 one Qwen3.6-35B-A3B response on `rust-semver`
  (`~/Desktop/pwr-evidence/runs/fix2-q36-35b/rust-semver/mlx-trace.jsonl`)
  took 799 s: 369 s prefilling 43,406 tokens, then about 415 s writing one
  tool call's arguments (a whole file), during which only content-free
  progress chunks reached the client. A dense or XL model would cross 900 s on
  healthy replies, and a bound counting only text would have cut this one. The bound is now on silence: opening,
  then each wait for a chunk, at most `timeout_secs`. Hung engines stay bounded
  (MLX's own 300-second engine-silence watchdog, this bound on other
  backends); reply length stays bounded by `max_tokens`, the loop detector and
  Stop. Control: the absolute deadline on the same fixtures. Regressions:
  `configured_response_bound_covers_opening_and_silence`,
  `a_response_that_keeps_making_progress_outlives_the_bound` (failed before).
- **Compaction trigger.** `84b4654c` stopped refusing an objective above the
  compaction trigger, but a turn of several steps then compacted on every step:
  objective 6,000 characters, trigger 1,024, window 16,384, five 3.2 KB reads
  -- two compactions (the first grew the prompt 2,390 → 2,704 estimated
  tokens), stopped on the compaction budget after three reads. A trigger the
  turn cannot get under is now said once and then ignored; only a window with
  no answer room compacts. Regressions:
  `an_objective_above_the_trigger_does_not_spend_compactions_it_cannot_use`,
  `a_compaction_that_cannot_get_under_the_trigger_is_not_repeated` (both
  failed before; the second also with only its post-compaction branch removed).

Not changed: `COMPACTIONS_PER_TURN` (2), the 900-second default, the retry
count, sampling. The review also found that `312070c6` withdrew the per-mode
card reading of `b14982f0` without a measurement; on the installed models the
resolved values do not change (Qwen3.6 has a declared profile; Qwen3.5 cards
fall to the 0.6/0.95/20 floor, the same values), so nothing was reverted.
Capability effects: unknown.

## 2026-10-02 — F1 executor cancellation and verification correctness

**IMPLEMENTED correctness repair, not a capability experiment.** Parent
`312070c6`. The existing chosen response timeout now covers runtime opening
and streaming. Stop/drop signal managed worker cancellation; Stop also reaches
context preparation, permissions, baseline/closing checks and Goal review.
Protocol EOF releases permission waiters. Empty or recognized zero-test checks
cannot certify CLI/scripted completion; zero-test declared acceptance is never
eligible for Goal baseline exemption. Completion descriptions follow each policy.
No defaults were tuned, no weights loaded and no heldout inspected. Ordinary
aggregate conversation bounds, real cancellation latency and model capability
remain unknown/PLANNED; full F1 audit is incomplete. Regression/check evidence:
[executor follow-up](reviews/2026-10-01-audit.md) and
`~/Desktop/pwr-evidence/logs/mission-20261001-f1-executor/`.

## 2026-10-01 — F1 sampling source correctness (measurement-changing)

**IMPLEMENTED A05 correction; MEASURED deterministic regressions, no model experiment.**
Parent `84b4654c`. Before the fix, the parser accepted unscoped benchmark/coding
prose, fenced continuation values, mode-bound recipes under unknown mode and
out-of-range top_k; cache schema 3 reused those chosen recipes. Additional
failing regressions exposed excluded-label continuation, section-end, HTML and
indented-code leaks. Control is the prior parser/cache on the same fixtures;
criterion: zero excluded-source acceptance and no guessed mode or hybrid recipe.

**Decision: keep the correctness correction; W5.6 remains PARTIAL.** Named scopes,
excluded ancestry, matching fences and code boundaries, complete alternatives,
explicit fixed-mode parsing and repository/revision/mode cache schema 4 are
implemented. Pinned source-file provenance is retained; original fallback uses
the already-verified artifact/base relationship. UI/chat/eval enrichment uses
unknown mode because later reasoning/finalization planning can change `think`.
Fixed-mode APIs preserve legitimate recipes; product selection after final
per-generation planning remains PLANNED. The simpler safe control is neutral
recommendations followed by the existing artifact/default precedence, chosen
here rather than adding another dynamic planner before executor parity.

Old Qwen fixture fragments now include their actual sampling scope and an
explicit mode; the permissive unscoped-prose assertion now requires rejection.
These replace unsafe expectations, not weaker verifiers. There are 19 new
sampling tests, including a real local HTTP/cache path, no model or Hub traffic.
No new numerical floor/profile tuning; rejecting old card choices can change
sampling on existing deployments, so prior campaign arms are not interchangeable.
Task capability/performance effects remain unknown; no inference, heldout
inspection, model download or push.

**MEASURED final checks:** Rust exit 0, 1,405 reported passed / 0 failed / 5 ignored across 102 target summaries; one reported pass is a Docker socket skip. Clippy with denied warnings, fmt, desktop 107 tests / 13 files and production build, sidecar 41 tests, milestone agreement and diff checks pass. Existing npm dependencies were reused;
hosted CI, clean npm installation and the Docker case remain unverified.
Starting process-safety and Semantic Decision Layer hunks remain uncommitted.
Evidence: `/Users/vitosantanelli/Desktop/pwr-evidence/logs/mission-20261001-f1-sampling/`.

## 2026-10-01 — F1 context correctness (measurement-changing)

**IMPLEMENTED; MEASURED deterministic regressions, no model experiment.**
Parent `b9db2eab`, product conversation executor, fake providers plus real
Settings dispatch. A01–A04 and adjacent count/no-op/audit defects were reproduced
before their respective fixes. Preparation errors no longer become grants;
physical fit differs from the policy trigger; productive compactions exhaust a
harness budget; serialized tool arguments count toward the next prompt.
Compaction invalidates old history counts while retaining learned fixed costs.
Read-only model inventory remains usable when the saved model is unavailable.

**Decision: keep as correctness fixes.** Control is the same path without each
fix; the error-path invariant is no invented grant, unsafe generation or
fictitious generation attempt. Policy/physical regressions retain objectives
and reject true overflow. No task-capability or speed improvement is claimed.
The cap of two compactions, 32,768 ceiling, 100 actions, sampling defaults and
Goal continuation policy are unchanged. Exact preflight and eval parity remain
PLANNED. No inference, heldout inspection, download or push.

**MEASURED final checks:** Rust exit 0, 1,386 reported passed / 5 ignored,
one Docker socket skip; Clippy with denied warnings, fmt, desktop 107 tests /
13 files and build, sidecar 41 tests, milestone agreement and diff checks pass.
The overflow regression also passed after a lint-only fixture edit. Hosted CI,
clean npm installation and the Docker case remain unverified. Evidence/notes:
`/Users/vitosantanelli/Desktop/pwr-evidence/logs/mission-20261001-f1-context/`;
[context audit follow-up](reviews/2026-10-01-audit.md#f1-context-follow-up--after-f0-adoption).

## 2026-10-01 — Adopt the local harness mission

**Decision, not a capability result.** The owner approved F0 and continuation
("ok procedi"). D-2026-10-01-2 adopts the S/M/L/XL coding mission, measured
extension boundary and all-tier F6 exit criterion before Windows. The former
contract is archived whole; W IDs remain traceable and G1/G2 stay unpassed.
Numerical exit thresholds await initial baseline and separate owner approval.
No source/default change, inference, campaign, gate pass, push or release in
this documentation cycle. Local checks are recorded in the initial audit.
Other sessions' dirty code and research/doc hunks are preserved outside the
mission commit.

## 2026-10-01 — Mission F0 baseline checks and pure sampling reproduction

**MEASURED local checks, not model capability.** At
`ae1e36c1e5dbe80f7fa3ee781072948b106a0208` on the owner's M2 Max with the
preexisting process-safety/research working-tree changes preserved: Rust exit
0, 1,372 reported passed / 5 ignored, including one host-dependent Docker skip;
desktop 107 tests and production build; sidecar 41 tests on Python 3.11.15,
mlx 0.32.3 / mlx-lm 0.31.3; formatting, denied-warning Clippy, milestone agreement
and diff whitespace checks pass. Docker unavailable is environmental. Hosted
CI and a clean npm dependency installation were not run.

**MEASURED pure parser defect.** A small Rust probe calls the current public
sampling parser: a benchmark coding recipe returns `temperature=1.0`, a next-line
fence opener returns `temperature=0.6`, and conflicting thinking/instruct coding
sets choose thinking without an actual mode argument. These reproduce the
source-boundary/mode problem, not sampling efficacy. Convert them into failing
crate regressions before the minimal fix (proposed W5.6).

The [initial audit](reviews/2026-10-01-audit.md) records source-verified findings,
coverage gaps and local artifact paths. F0 contract/decision are drafts pending
owner approval. No code/default changed, inference or capability/performance
baseline ran, heldout was inspected, model was downloaded, campaign was queued,
commit was made or push was dispatched. Next: F0 approval, correctness regressions,
product/eval parity and prospective baseline protocol. Negative capability
claims cannot be inferred from absent measurements.

## 2026-10-01 — Semantic Decision Layer (research proposal; no experiment run)

**HYPOTHESIS, not implemented or measured.** Can an 8B local generator plus
semantic ranking and deterministic verification outperform 14B/30B vanilla
agents at equal or lower total computational cost? The
[research proposal](research/semantic-decision-layer.md) separates generation,
selection and verification, specifies model-agnostic candidate-ranking levels,
controls, metrics, risks and success criteria. No scorer has been selected,
no campaign has run, and no product, routing or roadmap decision is made.

## 2026-09-30 — W1.4 goal budget (working tree; commit pending)

**IMPLEMENTED, not a capability measurement.** The product goal path now shares
limits across baseline, generation, completion verification and review: defaults
208 actions, 6 refusals, 9 verification runs, 1 review round and 3,600 seconds.
Workspace overrides and actual counters accompany the outcome. Calls inside a
batch respect the remaining actions. Deadline expiry cancels work at the
existing async/safe points. Ordinary chat and scripted evaluation budgets are
unchanged. Any new product-path campaign must record these effective limits;
results from the previous unbounded refusal path are not silently equivalent.
No model campaign was run for this change.

## 2026-09-30 — A model switched in the middle of a 130k-token conversation (found by hand)

**What happened.** GLM-4.7-Flash, selected in a conversation Ornith 35B had built
up to ~163k tokens. Its first generation spent **32.5 minutes** in prefill
(`reasoning.started` at `elapsed_ms` 1,952,557: ~66 tokens/s for ~130k tokens,
GPU at 98-100%, 45 GB resident, 4.9 GB of swap in use) -- the prompt cache is
per model, so the whole history is read again from nothing. It then reasoned for
736 tokens and was stopped by the engine's repetition guard; the reply after the
retry message was "736 token." repeated.

**Two causes found, one fixed.**
1. The message that reached the model carried the engine's own account of the
   stop ("after 736 tokens; repeated-window ratios: reasoning=5732bp"), which
   a degenerating model latched on to. The model is now told only that the reply
   was repeating itself; the log and the person keep the numbers. Fixed, with a
   fixture.
2. The sampling: GLM's `generation_config.json` sets only `temperature: 1.0`,
   and every other parameter fell to the sidecar's "off" (top_p 0, top_k 0),
   so the model sampled from the **whole, untruncated distribution** at
   temperature 1.0, in a 4-bit quantization, at 130k tokens of context. Not
   fixed: it is the second case, after Qwen3.5-9B's greedy temperature 0, of a
   model with no declared truncation; a decision is pending on a floor for
   every model.

**Also not fixed.** The prefill's progress exists in the engine's events and is
used only to show the engine is alive; nothing tells the person a model switch
costs a full re-read (plan W5.5/W7.1).

## 2026-09-30 — A working site reported as failing its checks (found by hand)

Bonsai 27B in the desktop built a site whose script held a card template with
`<img src="${car.image}">`. The web asset check took the placeholder for a file
and the turn ended "the newly discovered project checks failed: pwr:web-assets"
with the banner "the work is not verified". The site worked; the check was
wrong. Placeholders filled in at run time are now not references (`web.rs`); a
fixture uses the same shape and the real workspace passes. The mark and the
banner were right about what the check said -- the fault was the check's.

## 2026-09-30 — A file too long for one tool call (found by hand, in the app)

**What happened.** Bonsai 27B 1-bit in the desktop, asked for a whole site in one
HTML file: the model tried to write it in a single `write_file`, generated for
about eleven minutes (~16k tokens at 23.8 tok/s), and was cut off inside the
call (`turn.failed`, outcome `runaway_reply`, "stopped inside an unfinished tool
call", 703 s and 682 s). It then made two small calls, which reset every
consecutive counter, and tried the whole file again -- four times in forty
minutes. Not the sandbox: the failing generations never reached a tool. The
engine was busy the whole time (CPU time advancing, stack in `async_eval`).

**Cause in the code.** The engine reports the cut-off as `Truncated`, which the
turn classifies as a runaway; the retry's 8,192-token bound applied only to two
other messages of that class, the model was told only to "do less in one turn",
and the counter that ends a turn was consecutive.

**Change.** A cut-off tool call now (a) bounds the retry at 8,192 tokens, (b)
tells the model the file is too long for one call and to write a first part
then add the rest with `replace_text`/`apply_patch`, and (c) counts toward
three cut-off replies *in all* per turn, after which the turn stops.

**Outcome.** Two fixtures: the retry's cap is 8,192 in a 262k window (16,384
without the change) and the message carries the instruction; a turn cut off
between small calls stops at the third instead of repeating. Not yet seen to
help the model in the app: the model may still need several attempts to
split, and a 1-bit 27B at 24 tok/s spends minutes on each.

## 2026-09-30 — Two completion holds now apply on every path (a measurement-changing change)

**Question.** Does anything change for a campaign when the scripted loop and
the app's conversation hold the same completions (plan W2.4, decision
D-2026-09-30-6)?

**Change.** The scripted loop (`pwr run`, `eval run`) now holds a first
`complete` in a run that has done nothing, once; the conversation now holds a
`complete` over a program the turn created and never ran, once. Same
predicates and wording as the loop that already had each.

**Effect on measurement.** A scripted run that completes as its first act, or a
conversation that builds and never runs, now spends one extra model turn
before it can finish. Campaigns recorded before this commit did not have the
scripted hold; **they do not pair with later ones under `--strict` unless
`harness_rev` is declared as the treatment**, which the build identity already
forces. No campaign has been run on the new behaviour.

**Also changed the same day** (decision D-2026-09-30-6): `eval run
--reasoning-effort` now defaults to `medium` (`off` restores the earlier
template-controlled reasoning), and `record_progress` is offered to a scripted
run only when it has a plan. Both change what a campaign sends the model.

**Kept** (not yet measured). To be reassessed on the small-apps corpus once the
maintainer allows model runs; the hold's cost is one turn, its value is the
completions of 2026-09-30 it would have stopped.

## 2026-09-30 — Review verified; no experiment run

The technical review of 2026-09-30 was checked claim by claim
([verification](reviews/2026-09-30-verification.md)). No campaign was run. The
Rust suite was run once (one environmental failure, the Docker test). The
decisive comparison is specified in the
[implementation plan, W8.4](plan/implementation-plan.md#w84-the-confirmatory-campaign)
and has not started.

## 2026-09-29/30 — Small models building from scratch

**Question.** Where do 9–20B models fail when asked to build small projects
from nothing, and which harness changes help every model?

**Conditions.** `corpus/small-apps-v1.json` (four tasks: a bank login page, a
unit-converter CLI, an HTTP todo API, an inventory module with tests; hidden
verifiers that run the code), scripted loop, arm B1, seed 1, one trial per
task, grants equal to the desktop's Standard mode; harness changing between
campaigns. Deployments: gpt-oss-20b, Qwen3-14B, Ornith-1.5-9B,
Qwen2.5-Coder-14B (MLX, maintainer's M2 Max). Artifacts under
`.pwr/small-runs/` (local).

**Outcome** (hidden-verifier passes, first campaign → last measured):
gpt-oss-20b 3/4 → 4/4; Qwen3-14B 2/4 → 3/4; Ornith-1.5-9B 1/4 → 3/4 (third
campaign stopped); Qwen2.5-Coder-14B 0/4 → 1/4. One trial per task:
**development evidence, not a comparison**. Qwen3-14B answered the same bank-page
prompt four different ways across four runs.

**Kept** (for every model): an import that does not resolve is answered with
the install to run; edit results carry the changed lines; `complete` over
unread results waits a turn (both loops); a scripted run that built a program
and only syntax-checked it is asked once to run it; a command failing the
same way twice is not run a third time; five failed runs after edits → hand
over; `file://` placeholders named; an `npx` that fetched a missing package
says so; a conversation's `complete` before anything was done is asked about
once (`1d657268`); format repairs (invented tool names and argument
spellings with one reading, quoted `cwd`, Qwen2.5's fenced and unterminated
calls, `<|endoftext|>` as a stop token); Ornith-1.5-9B's profile at the
vendor's temperature 0.6; `eval run --reasoning-effort`.

**Open.** Several of these fixes exist in one loop only; the campaigns
measured the scripted loop, not the app's (plan W2.4). Full record:
[release/v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md).

## 2026-09-30 — implementation consolidation (working tree)

Implemented bounded goal execution, acceptance artifact protection and persisted
hashes, named human authorization, failure fingerprints, PDF/index/embedding
bounds, typed conversational outcomes, persistent objectives, default-off
cancellable summaries, journal concurrency protection and strict comparison.
Tests use deterministic fakes, filesystem fixtures and real macOS confinement;
they do not establish model improvement. G1/G2/G3 remain unpassed, the scripted
runner is not yet unified, and no confirmatory campaign or release was run.

Consolidation validation: workspace Rust 1,327 passed / zero failures / five
explicitly ignored tests; lexical fallback regression passed separately; CLI
216 unit tests included in the workspace run; desktop 106 tests and production
build passed; installed-engine Python sidecar 38 tests passed; workspace Clippy
with warnings denied, formatting, diff and release-shell syntax checks passed.
GitHub CLI is not authenticated here, so remote CI was not dispatched and G1
remains unpassed. Changes are in the working tree, without an integration commit.

## 2026-09-30 — Sampling floor and multi-mode cards (measurement-changing)

**What changed.** (1) A card that lists one sampling set per mode (Qwen3.5's
"Thinking mode for general tasks / precise coding tasks / Instruct mode…")
now gives its *thinking, coding* set; before, nothing was found and the model
ran greedy (`crates/pwr-models/src/sampling.rs`). The "none found" cache is
schema 3 and expires after a day. (2) When nothing anywhere declares a
temperature (no profile, card, generation config or person), the engine
request uses temperature 0.6, top_p 0.95, top_k 20 instead of greedy
(`pwr_mlx::resolve_generation_sampling`, source `pwr_sampling_floor`); an
explicit temperature 0 stays greedy and a vendor's lone temperature is not
topped up.

**Why.** Qwen3.5-9B, several lmstudio-community conversions and Qwen2.5-Coder-14B
ran greedy (manual pass, 2026-09-29/30), which Qwen's cards warn leads to
endless repetition, the commonest failure of a small model in the loop.

**Effect on comparisons.** Campaigns run before this revision (harness
`414d3ae1`) used greedy for these models; they are not paired with later ones
unless the sampling is declared as the treatment. The A/B is run on the dev
split (`base-q35-9b-a` before, the `fix-*` runs after).

## 2026-09-30 — Per-turn action limit 26 → 100, configurable (measurement-changing)

A turn stopped at 26 actions and a goal's per-step allowance was capped at 26
inside one `take_turn`, whatever the goal allowed. A manual pass stopped a
working model mid-build. The default is now 100 and `actions_per_turn` sets it;
the stall detectors, not this number, stop a loop that repeats. Campaigns
through the app path at `414d3ae1` or earlier have the old cap.

## 2026-09-30 — Compaction ceiling (measurement-changing, HYPOTHESIS)

A conversation compacts at the lower of 75 % of the window and 65,536 tokens
when the person chose no window or threshold
(`Continuity::compaction_room`). Campaign tasks stay far below the ceiling, so
this changes no campaign result made so far; it changes long app sessions. Not
yet measured for quality (plan W4): it is justified by prefill time alone.

## 2026-09-30 — Presence penalty after a looped generation (measurement-changing)

Once a generation of a turn ends as `looping_reply`, the turn's later
generations ask for `presence_penalty` 1.0 (source `pwr_loop_recovery`) unless
a higher value is set. Cause: `base-q35-9b-a` and `fix1-q35-9b` (Qwen3.5-9B,
greedy vs the card's 0.6/0.95/20, `bash-rotate`) both failed at the hour, the
second with five looped or cut-off generations: the card's sampling did not stop
the reasoning from repeating one passage. Qwen's card names the remedy. Effect
to be measured on the dev split (run `fix2-*`).

## 2026-09-30 — The review round also runs without a declared acceptance check (measurement-changing)

`fix2-q36-35b` (Qwen3.6-35B-A3B) on the dev tasks `bash-rotate` and
`go-logstat` ended "technical checks passed, goal not verified" with a rule the
README states outright unmet (blank lines ignored; `--keep 1` deleting older
copies); no review had run, because the review round required a passed
acceptance check, which a workspace with none never has. The round now runs
once when technical checks pass and no acceptance is declared; the goal still
ends *not verified*. Harness revision after this entry is the pairing boundary:
`fix2-q36-35b` is the before, `fix3-q36-35b` the after.

## 2026-10-01 — Qwen3-Coder read as "no tool call" (adapter `qwen-v3`; measurement-changing)

Quick Calibration of `Qwen3-Coder-30B-A3B-Instruct-4bit` ended **Limited**
("tool_selection: no tool call was made"), which refuses agent tasks; every
`dev1` task on it failed in 0 actions. The engine trace showed the reply
`<function=read_file>…</function></tool_call>`: the opening `<tool_call>` never
arrives. The Qwen adapter now reads a `<function=` call with no opening tag
(revision `qwen-v3`, so earlier calibrations of Qwen-family models become
stale and are retaken). A model's own tool format being refused is a product
defect, not a model result: any Qwen3-Coder user had no agent mode.

## 2026-10-01 — A failure is stale once the adapter that read it changed

After `qwen-v3` Qwen3-Coder stayed **Limited** in every campaign run: the stored
verdict (`~/.pwr/model-evidence`) did not say which adapter had read the replies,
and Limited outranks everything. Provenance now records the adapter revision; a
Limited/Incompatible verdict recorded under none, or under another revision, is
stale and the model is calibrated again. A pass is kept.

## 2026-10-01 — Mistral family adapter (`mistral-v1`; measurement-changing)

Devstral-Small-2-24B was Limited ("no tool call was made") in a calibration
sweep of all installed models: it writes `[TOOL_CALLS]read_file[ARGS]{…}` and no
adapter read that form. `MistralFamilyAdapter` reads it (and the older JSON
array form), for `mistral`, `devstral`, `magistral`, `ministral`; the engine's
live text holds `[TOOL_CALLS]` back like the other call markers.

## 2026-10-01 — Liquid (LFM2) family adapter (`liquid-v1`; measurement-changing)

LFM2-24B-A2B and LFM2.5-8B-A1B were Limited in the calibration sweep: LFM2.5
writes `<|tool_call_start|>[read_file(path="…")]<|tool_call_end|>` (Python call
syntax) and LFM2-24B `<function_call>{json}</function_call>`. `LiquidFamilyAdapter`
reads both, with a small reader of Python literals (strings with escapes,
numbers, booleans, lists, dicts) and `<think>`; a block that does not decode is
kept in the text and nothing is guessed.

## 2026-10-01 — Tool calls and their results carry ids and names (measurement-changing)

Gemma 4 12B and 26B were Limited: "tool_result_continuation: the MLX engine
failed: TypeError: can only concatenate str (not NoneType) to str". Gemma's
chat template names a tool result from the id of the call it answers; PWR sent
calls with no `id` and results with no `name`. `template_messages` now gives
every call an id (its own, else nine letters/digits -- Mistral's template
insists on that) and every result the call's id and the tool's name, matched by
id or, failing that, by position. Every MLX model's prompt gains these fields;
Qwen-style templates ignore them.

## 2026-10-01 — Quick Calibration `quick-calibration-6`: tools are used, not chosen (measurement-changing)

`tool_selection` and `tool_arguments` (critical) now ask whether the model makes
one call to an offered tool with arguments that fit its schema; reaching for
`read_file` first moved to `tool_choice`, not critical. Gemma 4 26B started
with `run_command ls`, a sound first step, and was refused agent tasks. Every
earlier local calibration is stale (the suite version is compared) and is
retaken on use.

## 2026-10-01 — Granite 4.1 reads its `<tool_call>` block (`granite-v2`; measurement-changing)

Granite 4.1 8B was Limited ("no tool call was made"): its template writes
`<tool_call>{"name": …, "arguments": …}</tool_call>`, and the adapter read only
a bare JSON array. It reads both now.

## 2026-10-01 — Mistral gets alternating roles (measurement-changing)

Devstral's first campaign turn failed before a word: "TemplateError: After the
optional system message, conversation roles must alternate user and assistant
roles except for tool calls and results". PWR sends several user messages in a
row (repository passages, the task, notes). For the Mistral family, runs of
user (or call-free assistant) messages are joined into one, in order, in the
request to the engine (`alternating_roles`); other families' prompts are
unchanged.

## 2026-10-01 — Python-style lists in XML parameters (adapter, measurement-changing)

`dev3-qwen3-coder` on `bash-rotate`: six `run_command` calls refused as "`args`
looks like a list written as JSON, but it is not valid JSON": the model writes
`<parameter=args>['python3', '-m', 'unittest']</parameter>`. A parameter that
is wholly a Python list or dict literal is now read as one (Qwen-family XML
parameters; same reader the Liquid adapter uses).

## 2026-10-01 — A clean start after two degenerate replies (measurement-changing)

`dev9-qwen3-coder` on `bash-rotate`: thirty-two sound generations, then at
26,900 tokens of failing-and-retrying history nine replies in a row collapsed
into "!!!!!" or "call call call" (engine repetition stop), and the turn ended
`protocol`. Controls (2026-10-01, same engine, same model): a fresh 30,000-token
prompt is answered correctly, and a synthetic 40-turn chain of cache-reusing
requests to 32,000 tokens stays coherent. So the collapse comes with the
conversation, not with length or the cache. After two such replies in a row the
turn now compacts the history to an 8,192-token room (two clean starts per turn
at most) and carries on. Hypothesis: it rescues a model that is lost in its own
history; measured on `dev` next.

## 2026-10-01 — A file written again and again is named to the model (measurement-changing)

`serial1-qwen3-coder` on `bash-rotate`: 130 actions in 60 minutes, `bin/rotate`
written about seventy times with the same tests failing each time. The stall
detector looks for windows that leave the workspace as it was; each write
changed it. The conversation now counts writes per file, and every twelfth one
carries a note in its result naming the count and the ways out (read the
failing output line by line, change the approach, or say what blocks). It
states and does not decide; no limit was added. Effect to be measured on `dev`.

## 2026-10-01 — The anti-loop presence penalty gets a window that can see a loop (measurement-changing)

Reading mlx-lm 0.31.3's `make_logits_processors`: the presence penalty looks only
at the **last 20 tokens** (`presence_context_size=20`), whereas a vendor's
presence penalty (Qwen's 1.5) is defined over everything generated so far. A
loop of a passage longer than 20 tokens is invisible to it, so the penalty
added after a looped reply (entry above) could not do what it was added for.
The request now carries `presence_context_size` (1,024 after a loop; absent
otherwise, so every other generation is unchanged) and the sidecar passes it
on. A card-declared presence penalty still runs on the engine's 20-token
window; whether to widen that for the models that declare one (Ornith, Qwen3.5)
is a separate, unmeasured decision.

## 2026-10-01 — Compaction ceiling 65,536 → 32,768 (measurement-changing)

Same hypothesis as the entry above, tightened on evidence found by reading
rather than by testing: a silent KV-cache corruption reported against exactly
the pinned engine (MLX 0.32.0, mlx-lm 0.31.3) on natural-language prompts of
about 60k tokens and more, where generation collapses into token id 0 ("!")
([jundot/omlx#3777](https://github.com/jundot/omlx/issues/3777)); and PWR's own
collapse at 27k tokens of history (`dev9-qwen3-coder`). Neither is proven to be
the same defect.

## 2026-10-01 — `mlx` 0.32.0 → 0.32.3 (measurement-changing)

Compared in a scratch environment before the pin moved (one model at a time):
the sidecar's 39 tests pass on 0.32.3; Gemma 4 12B prefill of a 4,976-token
prompt 23.7–24.1 s against 25.3–27.4 s on 0.32.0; Qwen3-Coder-30B answers a
fresh 30,000-token prompt and a 40-turn cache-reusing chain to 32,000 tokens
coherently on both (neither reproduced the 27k-token collapse seen in a real
run). After the pin moved, Gemma 4 12B, Qwen3.5-9B, Devstral and gpt-oss passed
the critical Quick Calibration checks. Not shown: that it removes the
long-conversation collapse. A backend patch version is a *reduced*-confidence
change for existing calibrations, not a stale one.

## 2026-10-01 — `start_service` without a port finds the port the program listens on

Found in the app, not by a campaign: a goal with Gemma 4 26B building an Angular
site failed three times to start `npm run start`, first because `run_command`
refuses a program that serves until stopped (correct), then because
`start_service` with no port waited on a port PWR reserved and **never gave to
the program** — `ng serve` listened on 4200, and the start failed after 30 s for
a server that was up. Omitting the port now waits on the reserved port or on any
port the service's own process group listens on and reports the one that
answered (`wait_until_ready_on_any`, `lsof`); naming a port stays strict.
Neither Full access nor Goal mode was involved.

## 2026-10-01 — The clean start only above 20,000 tokens

In the app, a Gemma 4 26B goal writing a landing page had six clean starts
(`context.compacted`, trigger `automatic`) at 11,000–15,000 tokens; the replies
went on repeating after each (`turn.failed`, `runaway_reply`, answer repeated
windows 40–96 %). The history was not what the model was lost in, and each cut
cost it its reads and a cold prefill. The mechanism now acts only above 20,000
tokens, where the collapse it was made for was seen (27,000, Qwen3-Coder). Why
Gemma 4 26B repeats while writing a large component is **not known**; the next
step is its raw output (`PWR_MLX_TRACE`).

## 2026-10-01 — The engine's repetition stop now counts for the presence penalty

Raw output captured from the app (`PWR_MLX_TRACE`, Gemma 4 31B and 26B, prompts of
22,000 tokens): a reply that begins "Now I'll implement `app.ts`." and then
writes "I'll use `web/src/app/app.ts`." until the engine stops it after
600–1,200 tokens (`finish: repetition`); the next reply loops the same way.
These are real loops, not a false alarm on repetitive code. The state log showed
the retries asked for **no** presence penalty: only PWR's own `Looped` fault
set it, and the engine's stop arrives as a cut-off reply. Both now count. Whether
a penalty over 1,024 tokens ends such loops is **unmeasured**.

## 2026-10-01 — Prefill chunks bounded in time, so Stop is seen (measurement-changing)

Found in the app: after Stop during the cold prefill that follows a compaction
(Gemma 4 31B, 6-bit), the model picker did not switch for about three minutes —
the selection was written to the config only when the engine, which sees a stop
only *between* prefill chunks, finished one 8,192-token chunk. Reproduced with a
small model (a model switch after a cancelled generation took 101 s) and
measured with Gemma 4 12B: a cancel during a 8,900-token prefill was seen after
32.7 s with 8,192-token chunks and 2.3 s with time-bounded ones (first chunk 512
tokens, then about 3 s of work at the speed just measured, 256–8,192 tokens).
Prefill speed is unchanged within noise (4,976 tokens: 25.0 s at 1,024-token
chunks, 25–27 s at 8,192). The app still shows nothing while a switch waits
(plan W5.5).

## 2026-10-01 — Gemma 4 loops are an upstream weights defect, not PWR's reading

Raw output from the app (Gemma 4 12B, prompt 15–17k tokens, tool declarations
in the prompt): replies made of `<|channel>thought\n<channel|>` repeated, or
`<|channel>` runs, stopped by the engine's repetition guard; earlier, one
sentence repeated (31B and 26B). Searching the upstream reports found the same:
a "thought\n…" attractor on long agent prompts, ~44–60 % of trials on the 12B,
reproduced at full precision and across temperatures and top-k
([google-deepmind/gemma#622, #727](https://github.com/google-deepmind/gemma/issues/727),
[HF discussion 41](https://huggingface.co/google/gemma-4-12B-it/discussions/41));
a changed chat template lowered it only from ~44 % to ~35 %. So it is not fixed
by anything PWR sends, and the note is now shown beside every Gemma 4 model.
Banning `<|channel>` was considered and not done: the same model writes
`<channel|>` before a tool call even with thinking off.

## 2026-10-01 — The prompt says a new project goes in the root (measurement-changing)

Reported from the app: every model built its project in a new subfolder
(`ng new web`, `website/`) of a workspace that was already the project. Cause: a
generator names its folder after the project, and the conversation prompt said
only that paths are relative to the repository root. It now says the repository
is the project and a new one goes directly in the root (point the generator at
`.`), unless a subfolder is asked for or the root already holds a different
project, and to say so, not to clear the person's files, when a generator
refuses a non-empty folder. Whether models obey is **unmeasured**; the Angular
topology guidance also assumes a root `angular.json`, which a subfolder
project hid.

## 2026-10-02 — Libra manual failure: delivery evidence and workspace recovery

**MEASURED:** the owner's Nemotron 3.5 manual run on the C01–C15 build left
Libra empty after 47 tool actions (failed invented outside write, one listing,
45 commands), yet the UI claimed delivery. Read-only journal diagnosis and
red regressions are described in [the repair review](reviews/2026-10-02-libra-delivery.md).
**IMPLEMENTED:** artifact evidence separated from possible command effects;
ACP preserves it; unavailable checks no longer imply delivery; replay/live
failures remain failed; one failed-write completion hold and explicit workspace
anchor/recovery instructions. The golden refused-edit turn changes only its
delivery flag to false. No verifier, sandbox boundary, sampling or investigation
threshold is relaxed. Real-model recovery effectiveness is **UNKNOWN**; repeat
the manual test before the battery. No inference/campaign was run by this agent.

## 2026-10-02 — Compaction at 75 % of the granted window, no default ceiling (measurement-changing)

**MEASURED** from the owner's Libra journal (read from a copy): Nemotron 3.5
Lightning 30B, window 262,144 computed by the app, no threshold chosen. The
conversation compacted twice, at 34,039 and 31,557 engine tokens (estimates
24,779 and 24,451), 13 % of the window, while the app showed "compact at 75 %";
the following turns answered from the summary (the owner's routing complaint was
restated as "the user reported..." and not fixed). Cause: the 32,768-token
default ceiling (D-2026-09-30-7, item 3) applied whenever the person had chosen
neither a window nor a threshold, and a computed window is not a choice.
**IMPLEMENTED** (owner's decision D-2026-10-02-3): no default ceiling; 75 % of
the granted window, a workspace ceiling `compact_ceiling_tokens` by hand. The
Libra configuration regression returns 196,608 and returned 32,768 before.
Coherence near that size and prefill cost after a switch are **UNKNOWN**.

## 2026-10-02 — A generated project moves into an empty workspace's root (measurement-changing)

**MEASURED** in the same journal: with the 2026-10-01 prompt rule, the model ran
`npm exec npm create next-app@latest libro-ecommerce` (after `npx create
next-app@latest ...`, which ran the unrelated package `create`) and wrote every
later file under `/Users/.../Libra/libro-ecommerce/`. A generator pointed at `.`
is no reliable alternative: create-next-app refuses `.pwr/` and the npm-invalid
name `Libra`. **IMPLEMENTED** (D-2026-10-02-4, `scaffold.rs`): in the full
harness, after a successful command in a workspace with no project, the one new
folder holding a manifest is moved into the root unless the request names it or
a name collides; later paths under it (relative, absolute, `cwd`) are read as
the root's and the model is told. Unit tests and a two-loops replay of the Libra
sequence; the minimal control is unchanged. Real-model effect **UNKNOWN** until
the owner repeats the task.

## 2026-10-02 — The first folder that becomes a project is the root, by hand too (measurement-changing)

**MEASURED** in the owner's third Libra session (a fresh empty folder
`Libra/libra`, build `2a639ab4`): no generator this time. The model made
`libro-ecommerce/` (`make_directory`, then `mkdir -p` in `sh -c`), wrote 13
files under it, ran `cd ./libro-ecommerce && npm install ...` (twice stopped at
the 120 s command timeout), and the generator-only rule moved nothing; the
person's dev server then failed on imports the model never wrote. **IMPLEMENTED**
(D-2026-10-02-4 revised): while the root holds no project, the first folder that
becomes one -- a manifest left by a command, or about to be written -- is taken
as the root; `cd ./folder`, `folder/...` and its absolute path inside `sh -c`
scripts are read as the root's. Two-loops replay of this session. Real-model
effect **UNKNOWN**.

## 2026-10-02 — `read_terminal`: the person's terminal, read-only (behaviour-changing)

At the owner's request (D-2026-10-02-5), a desktop conversation can read the
recent output of the person's terminal tabs, after a once-per-conversation
permission; text from xterm's buffer, redacted, at most 1,000 lines per tab.
Tests: the person asked once for two reads, secrets redacted, refusal told to
the model, no tool without a declaring client (two-loops); the round trip
through `pwr serve` with and without the capability; the desktop's answer and
its bounds. No model has used it yet: **UNKNOWN** whether models call it when
told about an error.

## 2026-10-02 — Browser checks for text and vision models

Continued the owner's authorized PWR browser integration: `check_page` reads
rendered DOM text and console for text models; `look_at` returns the same with
a screenshot for vision models. Corrected the decoder rejecting `look_at`
although it had been offered. Tests exercise actual Chrome, generated DOM text,
console exceptions, redaction, and image delivery only to vision models.
A separate local HTTP status probe does not follow redirects. The user's
embedded preview state and interactions are not inspected. Model behavior
on the owner's task remains unmeasured.

### 2026-10-02: recover the desktop layout and command timeout hint

The owner's Libra journal showed a patch to page.tsx using globals.css's hash;
its refusal is correct and the file guard remains in place. A separate valid
run_command was rejected for the extra timeout field. The decoder now drops
that hint and retains host execution limits, with a regression test; other
unknown fields remain errors. The unusable-reply stop message is shorter, not
a promise that model repetition is fixed. Restored the pre-redesign Focus
layout with current palettes and system fonts. Added diagnostic composer
requests to Terminal and Preview, preserving existing drafts. Browser UI check
confirmed URL insertion, retained draft and focus return. No real-model
recovery claim is made.

### 2026-10-02: discard the engine after a Metal command-buffer fault

The owner's e2e-test journal records a backend fault with `[METAL] Command
buffer execution failed: Impacting Interactivity`. The stream previously
cleared only the pending request and reused the same loaded sidecar on retry.
For this specific GPU execution failure it now drops the process, returning
an unavailable error with the original diagnostic. The existing bounded retry
loads a fresh engine, preserving the conversation and file edits. Transport
read failures also discard the broken process. A simulated JSON-lines engine
regression checks an error on the first process, an empty engine slot after
failure, and a successful reload in a second process. This does not establish
the cause of the GPU fault or demonstrate recovery under real-model GPU load.

### 2026-10-06: verified proposals as an opt-in phase of Goal mode (W2.9)

Changes what a goal does before its first turn when `goal_budget.proposals`
is above zero; the default, zero, leaves every measured path as it was. With
it on and an acceptance check failing, the executor asks the model for whole
files with no tools, applies each through a scripted turn, runs the full
verification and keeps the file only when fewer named tests fail and none is
new; then the ordinary goal continues. The failure fingerprint now names
`node:test` and Python `unittest` failures, which changes the identity of a
failure for those toolchains on every path (reproduction and goal repetition
included).

Evidence is from outside the product only (pwr-evidence
`batteries/20261003-small-model-diagnostics`, 2026-10-05/06; Ornith 1.5 9B
and Qwen3.5 9B, MLX 4-bit; 2-10 runs per cell; summarized in the plan item).
The product path differs from what was measured there: files are applied in
the workspace and restored, not tried in a copy; the budget is the goal's wall
clock, not a token count; the generation takes the provider's default
sampling. Simulated-host tests cover keep, restore, deletion of a refused new
file, a declined edit and an unavailable model; a real turn loop with a
scripted provider covers policy and hash binding. No model was run on this
path. Nothing here is a capability claim.

### 2026-10-06: proposals on the product path, and on per model (W2.9)

Measured, then changed. With `goal_budget.proposals` 5 against 0, binary at
`72b2597c`, 600 s goals, two runs per arm: ledger Ornith 1.5 9B 5, 5 -> 10, 11
of 17; Qwen3.5 9B 9, 14 -> 8, 12; python-todo (an empty workspace scores 6 of
14) Ornith 3, 6 -> 6, 6; Qwen 13, 13 -> 14, 13. Evidence and the five stopped
launches: pwr-evidence `product-path-20261006`.

What a campaign now measures differently: `goal_budget.proposals` is absent by
default and then follows the model's profile, which declares 5 for
`ornith-ai/Ornith-1.5-9B-MLX-4bit` and nothing for any other model; a goal
with that model and a failing acceptance check therefore starts with the
phase unless the workspace sets `"proposals": 0`. The phase now gives way
after four proposals in a row (or two per file) keep nothing, not after two
passes; that rule has not been run with a model. Two runs per cell say which
way one cell went, not by how much.

### 2026-10-06: a switch to tell a goal where its failing checks point (W2.10)

Adds `goal_aids.pointers` to the workspace configuration, off by default and
turned on by no profile, so nothing measured so far changes. With it on, a
goal whose acceptance check fails at the baseline has the files its failing
tests use appended to its first request. Reason: arXiv 2609.20804's weakest
model (30B) ended 58 % of its runs while locating the problem, and Ornith 1.5
9B here read a whole project and edited nothing in ten runs of ten. Covered on
the simulated host; no model has run with it.

### 2026-10-06: a switch for a ten-tool catalogue in goals (W2.11)

Adds `goal_aids.core_tools`, off by default and turned on by no profile. With
it on, the catalogue a goal's turns are offered is cut to ten tools. A
hypothesis from practice, not from a measurement; no model has run with it.

### 2026-10-06: a switch for proposals as edit blocks (W2.12)

Adds `goal_aids.block_edits`, off by default and turned on by no profile. With
it on, the proposals phase asks for SEARCH/REPLACE blocks for a file that
exists and applies them only when each matches exactly once. Simulated-host
tests only; no model has run with it.

### 2026-10-06: a goal can be bounded by model work (W2.13)

Adds `goal_budget.work`, absent by default: with it absent a goal is bounded
exactly as before, and its model work is now counted and reported in
`_meta.pwr.goalBudget.spent.work`. With it set, the goal ends when the model
has generated that many tokens (a prompt token read counting an eighth), and
the proposals phase reads its shares in tokens. Written so that runs made in
macOS Low Power Mode are comparable with runs at full power; that property is
tested on simulated streams and has not been checked with a model.

### 2026-10-06: a switch for a model-kept plan shown before every reply (W2.14)

Adds `goal_aids.plan`, off by default and turned on by no profile. With it on
a goal's catalogue gains `update_plan`, and what each generation is sent ends
with the plan as the model last wrote it, or with a line saying there is none
yet. The conversation that is kept does not contain it. The scaffold is the
one arXiv 2609.20804 measured on a 30B model; nothing here has run with a
model.

### 2026-10-07: a refused inspection command names the tool that does it

Read from the product-path runs of 2026-10-06 and 07 (three models, the
ledger, todo and notes tasks): of 311 actions that failed, 94 were tests
failing, which is the work, and 70 were requests to run a program the
workspace does not declare, refused because no person was there to allow
them. The most asked for were `ls` (14), `sh` (10) and `find` (7): mostly
an action spent to ask for what `list_tree`, `read_file` and `search` do
without asking. The refusal now ends by naming that tool
(`pwr_tools::tool_instead_of`); what a person is asked, and what is allowed,
is unchanged. Tested through the real turn
(`a_refused_listing_command_is_answered_with_the_tool_that_lists`); whether a
model then takes the tool has not been measured.

The same reading, not acted on: 21 calls that did not fit their tool's schema
(`replace_text` without `path` or `find`, `apply_replace` without
`replacement`, `run_command` with `args` as a map), 13 patches whose hunk was
already applied, 13 runs of a file that does not exist, 8 refusals by the
shrink guard, 7 stale hashes.

### 2026-10-07: the archive and the unused corpora leave the tree

By the maintainer's decision. `docs/archive/` (69 documents written before
2026-09-30), five corpora nothing in the code or the suites reads, and nine
analysis scripts of the September campaigns are removed; all of it is in the
history at `309266d5`, and the links to archived documents now point there.
The rule "nothing is deleted" in CONTRIBUTING and docs/README becomes
"nothing is lost": the commit that replaces a document says what replaced it.
No behaviour changes.

### 2026-10-07: three arguments under a neighbouring name, and a hunk's refusal

From the same reading of failed actions. Repaired, each with one reading
(`repair_form`): `apply_replace` sent `replace` with no `find`; `write_file`
sent `replacement`; `apply_patch` sent `replacement` and no hunks, which is
`apply_replace`. Four refusals of the 21 schema mismatches. And a hunk missing
`find` or `replace` was refused by listing the call's own keys as both what
the tool takes and what was sent; it now says what a hunk takes (four more).
Not repaired, because nothing says what was meant: a missing `path` (5) and a
missing `expected_hash` (4). Unit-tested; not yet run with a model.
