# Experiment log

Dated entries, newest first, for every experiment and every change that
alters what a campaign measures (prompt, catalogue, loop, budgets, adapters).
Each entry names: the hypothesis or question, the conditions compared (commit,
deployment, corpus revision, arm, seeds), the outcome with counts —
including an inconclusive or negative one — and what was kept, revised or
removed. Raw artifacts are cited by path even when they are not public.

Entries through 2026-09-17 are in the [archived log](archive/experiment-log.md);
the archived [roadmap](archive/roadmap.md) and [backlog](archive/backlog.md)
hold the campaign notes of 2026-09-18 to 2026-09-28.

---

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
