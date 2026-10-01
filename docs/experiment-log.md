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
