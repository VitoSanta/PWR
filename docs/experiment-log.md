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
