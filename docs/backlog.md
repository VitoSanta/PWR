# Backlog — everything not yet built, and when it may be measured

**Written 2026-09-17, during the pause.** This is the breadth of the work;
[`roadmap.md`](roadmap.md), section "Where we are and what is left", is the
order. Where the two disagree, the roadmap's order wins and this file is wrong
and should be fixed.

Each item says which block it belongs to and what it depends on. Nothing here
has a date: this project's sequencing follows evidence, not a calendar.

## At a glance — 2026-09-24

`- [x]` is done, `- [ ]` is open; an open item that is partly built starts with
a **Status** line. The order of work is in [`roadmap.md`](roadmap.md), "Update
— 2026-09-23".

**Ideas and experiments discussed with the maintainer**

| Idea | State | Item |
|---|---|---|
| Message queue, newest message first, server commands | done | D.E2E-26 |
| Prompt cache kept across a turn | done, confirmed on a run | D.E2E-21 |
| A record of every failed generation | done; read it at the next real stall | D.E2E-22 |
| Documents retrieved by section | done, measured | C.22 |
| Long documents read as an outline | built, not measured | D.E2E-15 |
| Local context filter (embedding, offline) | built, opt-in; modest measured gain; needs a discriminating task with repeats | C.22 |
| "System One" decider in the Jev shape | tried with a 1.5B and with Rizzo Flow (4B): both lower precision behind the fused ranking; set aside for context selection | C.22, C.22a |
| Encoder-ranked evidence after compaction | future experiment; deferred while the current runtime and context work is consolidated | C.22b |
| Search over installed dependencies | done, used unprompted | C.12 |
| Installed dependencies read-only | done | D.E2E-29 |
| Permission modes Ask / Auto | done | R.2 |
| Unconfined commands refused | done; Linux and Windows sandboxes still missing | R.3, E.1 |
| Small-model study against a fixed baseline | baseline exists (B0); campaign not run | R.10 |
| Per-model fallback registry | first version built: fallbacks for errors of form, measured on the A1 replay (6/6 repaired, no regression); a per-model report from the audits | C.24 |
| Chat mode: talk without a workspace | built: reads only what is attached (files, folders, images), cannot edit or run; 733 prompt tokens against 2,888 for a greeting in a workspace | C.26 |
| Images for vision models; the rendered page | steps 1-2 built: one engine loads a vision model once through mlx-vlm, text unchanged token for token, images attached in the app reach the model; the rendered page (step 3) not built | C.25, D.E2E-12 |
| Single-agent adaptive harness; skill packs | idea | C.23, C.21 |
| MCP, web, apps, plugins | not started, gated: one extension at a time | C.9, C.12 |
| Context panel and "Compact now" | built 2026-09-23: one compaction for both triggers, preserves task, files changed, open errors and check verdict; threshold settable 50–90 %; `context.compacted` audited ([models-and-context.md](models-and-context.md)) | C.1 (conversation half) |
| Unknown models, Quick Calibration, Reasoning Effort | built 2026-09-24: five profile states (no model is Verified yet), provenance-scoped evidence with deterministic reuse rules, a nine-request mechanical calibration, Low/Medium/High thinking budgets clamped by the context and enforced by the MLX engine with one bounded forced close ([model-compatibility.md](model-compatibility.md)) | B.11 |
| Hardware profile, Model Manager, fit rating | built 2026-09-23: normalized host profile, Hub search (JSON API only), per-variant fit from the window arithmetic, verified resumable downloads into the engine's folder; not yet run on Windows | D.6, B.4, B.8 |

**Largest open blocks**: Windows (E.1, E.2, B.7/R.4), packaging and signing
(D.17), first run and onboarding (D.10), model downloads in the app (D.6,
B.8), suites A5/A6 and the product gate (A.7, A.8, C.8), the 16 GB class
(B.4), one runtime for chat and scripted runs (R.11, C.1).

---

## Part 0 — Why measuring starts before the harness is finished

Raised 2026-09-17, and it is the right question to ask: *the harness is
incomplete; a measurement taken today is invalidated by the first change to it,
so would it not be better to finish the changes and measure afterwards?*

The objection is half right, and the half that is right has already cost this
project a campaign. The answer is that "measurement" here covers three
different things with three different lifetimes, and only one of them is
invalidated by a harness change.

### A campaign — the objection holds, completely

A campaign is preregistered, sample-sized, expensive and produces a claim. For
these the objection is not merely valid, it is **already enforced in our own
code**: `compare_strict` in `pwr-eval` refuses to compare two campaigns whose
`harness_rev` differs unless that difference is the declared treatment. A
campaign run on a harness that is about to change buys nothing, which is exactly
what happened on 2026-09-17 when R3's development run was stopped after 35 of 79
trials.

**Rule adopted: no campaign until the measurement regime is settled.** R3 stays
shelved. R4, R5 and R6 stay behind it. This is the objection, written into the
plan.

### A regression suite — the objection inverts

Areas A1–A4 are not campaigns. They are minutes per trial, and their purpose is
precisely *to be re-run after every harness change*. Saying they would be
invalidated by the first change is like saying unit tests are invalidated by the
first commit: being re-run is their function, not their failure mode.

Anthropic's own guidance on agent evaluation puts it directly -- "owning and
iterating on evaluations should be as routine as maintaining unit tests", and it
recommends building evals "to define planned capabilities before agents can
fulfill them, then iterate until the agent performs well", starting from
"20-50 simple tasks drawn from real failures" rather than waiting for a large
suite ([Demystifying evals for AI agents](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents)).
The redesign already requires this property of A1–A4 -- "fast by design (minutes
per trial), so a harness change can be checked against them the same day" -- and
that requirement is what makes the objection not apply to them. **If re-running
A1–A4 after a change were expensive, the objection would apply and the suites
would be designed wrong.**

There is a second reason not to defer them. "Finish the harness, then measure"
assumes we know what finished means. We did not: six months produced a harness
whose completion rates measured context starvation, and nothing in the build
said so -- only a measurement did. Defining the eval tasks is itself the test of
whether the requirement is concrete enough to build against.

### An outside reference — the objection cannot apply

The Bionic reference measures *another* harness. No change to PWR can
invalidate it; it only has to be recorded precisely enough to be re-read later
(model, quantisation, window, tasks, date, what was judged). It is the one
measurement here with an unlimited shelf life, and it is also the cheapest.

### And a fourth thing, which the objection correctly reclassifies

The re-measurement at a realistic window (step 3 of the roadmap's order) sat in
the plan looking like a campaign. It is not one, and calling it one would repeat
the mistake. **It is a diagnostic**: a small number of tasks, no preregistered
gate, no capability claim, asked only whether the starvation hypothesis of the
redesign's Part E holds. It will be invalidated by later harness changes and
that is acceptable, because its output is a decision about where to work, not a
number to publish. Labelled as such below.

### One thing the objection assumes that is not true

"Finish the harness, then measure" treats the harness as a container that is
finished at some point, with the result a property of the model inside it. It is
not: for a fixed model, success rates differ substantially across agent
harnesses on the same tasks, which is why recent work argues for treating the
harness as an experimental axis in its own right rather than bundling scaffold,
prompt, runtime and termination policy into each release
([Harness-Bench](https://arxiv.org/abs/2605.27922); the magnitude reported there
is worth checking against the paper before it is quoted anywhere as ours).
Every harness choice *is* a result. There is no moment when the harness is done
and measurement begins -- there is only measuring the harness you have, and
saying which one you had.

### What this means in practice

| Kind | Invalidated by a harness change? | Gate |
|---|---|---|
| Outside reference (Bionic) | Never -- it measures another harness | None. Do it first. |
| Regression suite (A1–A4) | It is re-run, which is its purpose | None. Build alongside the changes. |
| Diagnostic (Part E re-measurement) | Yes, and acceptably so | Label it a diagnostic; claim nothing from it |
| Campaign (A5, A6, R3–R6) | Yes, and expensively | **Blocked until the regime is settled** |

---

## Part 0b — What public data already settles, and what it never will

Raised 2026-09-17: is there not enough published data about models to build a
strategy without running campaigns ourselves? Substantially yes, and it removes
a body of work this backlog had not yet named.

### Free, exact, and needing no measurement at all

A model repository's `config.json` gives layers, KV heads, head dimension and
`max_position_embeddings`; the quantised repositories' file sizes give the
weights' memory. Part D's bytes-per-token follows from those by arithmetic. So
**"does this model run on this host, and up to what window" is answerable on
paper, for dozens of models, without downloading a single weight.** That is a
large part of the catalogue (B.3) obtained for nothing.

### A prior for the shortlist, not a decision

Published leaderboards narrow the candidates: the
[Berkeley Function Calling Leaderboard](https://gorilla.cs.berkeley.edu/leaderboard.html)
(v4: AST evaluation, parallel calls, multi-turn, an agentic section) covers what
area A1 asks; [RULER](https://arxiv.org/abs/2404.06654) covers effective context;
SWE-bench Verified, Terminal-Bench and Aider cover agentic coding.

**Consequence for the plan: we never evaluate many models ourselves to choose
candidates.** Public prior plus the arithmetic above gives the shortlist; only
two or three models ever enter A1-A6. What remains local -- memory, window,
speed -- is minutes of measurement per entry, not a campaign.

### What a leaderboard cannot transfer

1. **The harness.** For a fixed model the result differs substantially across
   harnesses ([Harness-Bench](https://arxiv.org/abs/2605.27922)). A BFCL score
   says the model can call tools, not that it does so inside PWR.
2. **The quantisation.** Leaderboards run at or near full precision. See below.
3. **This machine's speed.**

### The quantisation finding, and why it is uncomfortable for us

- 8-bit is close to lossless -- under about 1% on standard metrics.
- 4-bit loses substantially, and **worst on long-context tasks**: up to roughly
  16% on RULER/ONERULER-style retrieval regardless of method, and much larger
  drops on some long-input tasks
  ([Does quantization affect models' performance on long-context tasks?](https://arxiv.org/abs/2505.20276),
  EMNLP 2025).
- Uniform 4-bit **consistently degrades agentic performance across model
  families**, harming long-context reasoning and decision-making specifically
  ([Mix-Quant](https://arxiv.org/pdf/2605.20315)).

PWR's plan is a 30-35B model at 4-bit, at large windows, on long-horizon
agentic tasks. That is the combination this literature identifies as the worst
case, and the engine spike measured exactly it. Roughly, a ~14B at 8-bit and a
~30B at 4-bit cost similar memory; the plan has always silently chosen the
second. **This is not evidence that our 4-bit is the problem. It is a free,
strong hypothesis, checkable against A1 in minutes rather than by a campaign** --
and it becomes open decision 6 in the redesign.

### And a third ceiling on the window

RULER's finding is that needle-in-a-haystack is indicative only of a superficial
form of long-context understanding, and that models' *effective* context is
often well short of the length they claim. Two consequences, pulling opposite
ways and both useful:

- It supports dropping the needle check (open decision 1): passing it proves
  little, so it is not worth rebuilding.
- It undermines Part B's "the maximum comes from the metadata":
  `max_position_embeddings` is an optimistic number. The working window needs a
  **third ceiling beside memory and latency -- published effective context** --
  or it will be set to a length the model does not actually hold.

---

## Part 0c — A harness that writes itself

Proposed 2026-09-17: a harness that rewrites and extends itself from context,
using the model or a web search to fill the gaps it runs into.

It is three ideas wearing one name, and they get three different answers. The
first is the best thing proposed in this round; the third is the only thing in
these documents that is refused outright.

### Diagnosis — take it, and take it first

The valuable core. The harness already holds every number that would tell it it
is in a bad regime, and reports none of them *as a regime*. Making it say so --
to the user in the moment, and into the campaign record -- is cheap, safe and
needs no experiment, because reporting changes nothing that has to be
attributed. It is item **C.11**, and it is the item on this list with the best
ratio of value to cost. The old regime's central mistake was a constant nobody
re-measured; a harness that reads its own instruments would have surfaced it
months earlier.

### Adaptation — already the plan, and already fenced

"The harness adjusts its own policy from what it observes" is R4, and the
research contract states it as **H6: measured adaptation beats a strong fixed
policy.** The fence matters more than the idea: H6 must beat *the best fixed
policy*, globally and per deployment, on repository-disjoint holdout, paying for
its own probes -- because beating a badly chosen default proves nothing, and a
badly chosen default is exactly what we have. Adaptation on top of an unmeasured
base cannot be told apart from the base being wrong. So: keep it, keep its gate,
and keep it behind the fixed baselines it has to beat.

### Filling gaps by searching — legitimate, and correctly placed

Reaching for documentation or the web when the model lacks knowledge is a
capability, not a harness that rewrites itself. It is **C.12**, an R6 extension,
admitted one at a time with scoped policy and observable effects. Its prior is
unusually good for a local deployment, whose thin and dated knowledge is its
weakest point relative to a hosted model.

### Self-modification at runtime — refused

The harness generating and running its own code, tools or policy during a run is
refused, for three independent reasons any one of which is sufficient.

1. **It destroys attributability, which is the only thing this project has
   finished.** `compare_strict` refuses to compare two campaigns whose
   `harness_rev` differs unless that difference is the declared treatment. A
   harness that rewrites itself has a different, unrecorded revision *per run*:
   every run becomes its own harness, and no two results are ever comparable
   again. R0 was closed to prevent precisely this.
2. **It is on the wrong side of the confinement boundary.** Policy, sandbox,
   approvals and the audit live in the core so that the thing being supervised
   never holds them -- the same argument `pwr-serve.md` makes for the app,
   which "cannot bypass it because it never holds it". An agent that can edit
   the code enforcing its own sandbox is not sandboxed.
3. **It cannot be measured.** A system that changes during its own measurement
   has no measurement.

There is one bounded form, and it is not a runtime capability at all: **PWR
proposing a change to PWR as an ordinary contribution**, reviewed by a person
and its checks before it merges. That is a task an agent can be given, with a
human and CI where they belong, and it would make a good A6 task. It is the
self-improvement idea with the loop closed through review instead of through
`exec`.

---

## Part 0d — The product thesis, in one sentence

Stated 2026-09-17, and it is the redesign's Part D said as a promise to a user
rather than as a requirement on the runtime.

> **PWR runs well on your machine without you knowing how to make it run
> well.** It reads the host and the model, chooses a configuration that fits,
> and says which limit set it.

The differentiator is not loading a model -- Ollama and LM Studio do that. It is
that today the configuration is the user's to choose, and almost nobody chooses
it correctly. The best available evidence for the feature is this project: a
team building an evaluation harness, with a purpose-built calibration probe,
held the window at 16,384 tokens for six months on a number wrong by a factor of
six, and read the resulting completion rates as model capability. If the people
building the instrument get it wrong, a user has no chance. Democratising local
inference is not handing someone the weights; it is removing the expertise
needed to make the weights work.

### Two words that must not be used for it

**Not "the maximum the machine can run".** `MASTER_SPEC.md`'s third design
principle already says it: *"largest context is a candidate policy, not a
requirement."* Three findings agree. Effective context is often well short of
the claimed length (Part 0b), so the largest window is not one the model holds.
A larger model at 4-bit at its maximum window may lose to a smaller one at 8-bit
at a moderate one -- open decision 6, undecided. And the spike measured a 49 GB
peak on a 64 GB machine: "maximum" means the host is unusable for anything else,
which is an invasive product, not a democratic one. The promise is **the best
operating point for the work, given this machine**, with the binding ceiling --
memory, latency or effective context -- shown.

**Not "a boost to local inference".** PWR does not make inference faster;
MLX and llama.cpp do, and the engine spike measured LM Studio's MLX path
prefilling faster than our own in-process code. Promising speed invites a
benchmark against LM Studio that finds parity, and loses the credibility that
the real claim needs. The gain is **not running a bad configuration** -- which
in our own case was worth the difference between 2 of 35 tasks resolved and
whatever the same harness does when it is not starved. That is a large number,
and it is not a speed number.

### Which makes the thesis measurable — item A.15

If the promise is "ships a good configuration by default", then it is a claim
that can be measured, and nobody has measured it: **the same tasks, on the same
host, under PWR's automatic configuration against a naive one** -- the
defaults a normal user would land on. It measures neither the model nor the
harness but the configuration, which is the product's actual contribution.

`MASTER_SPEC.md` is deliberately untouched by this section; whether the thesis
belongs in the contract is the maintainer's call.

---

## Block A — Measurement

Nothing here produces a product feature. It produces the ability to tell whether
anything else worked.

- [x] **A.1 Outside reference -- done 2026-09-18.**
      `experiments/a1-bionic-reference-20260918`. Qwen3.6-35B-A3B (MLX 4-bit)
      in Bionic at a 262,144-token window resolved **4 of 5**; PWR on the same
      model and tasks at 16,384 resolved 0. The fifth was a false completion:
      a wrong change to one-shot `decode()` hidden by rewriting two tests
      outside the allowed files. By the preregistered rule: **the gap is
      ours.**
- [x] **A.2 Part E diagnostic -- done 2026-09-18.** At the host's window on
      PWR's MLX engine, 4 of 5 with 64 actions: the same four Bionic
      resolved (roadmap step 5; `experiments/part-e-orient64-mlx-20260918`).
      Was: the same tasks at the host's window, with
      `HISTORY_BUDGET_SHARE`, the action budget and the stall guard revisited.
      Explicitly a diagnostic, not a campaign.
- [ ] **A.3 Suite A1 — tool calls.** **Status 2026-09-23:** replay half built and in `cargo test` (roadmap step 6); open for the live half (valid-call rate on short scripted tasks). Record: Valid-call rate, malformed calls by kind,
      turns lost, recovery rate. Replay of recorded malformed outputs.
      *Replay half built 2026-09-19:* `suites/a1-tool-calls.json`, 46 cases
      from the 794 raw replies of the 2026-09-18 engine runs, run by
      `pwr eval suite` and by `cargo test` (seconds, no model). One format
      for all areas (`pwr_eval::suite`); every case states the right
      outcome and its origin, and a `known_gap` where PWR does not produce
      it yet -- 9 at creation: `run_command` without `executable` (5 lost
      actions), multi-line argument arrays (2), unreadable XML calls reported
      as `no_tool_call` (2) -- **all closed the same day** in the decoder:
      the program is taken from `args[0]` when `executable` is missing,
      argument lists with raw newlines are read, and unreadable markup is
      refused as `unreadable_call` with how to write it. On all 794 replies
      the decoded rate went from 97.4% to 98.2%. Harvesting it found the engine's loop guard
      cutting legitimate code (fixed: the guard now requires back-to-back
      repetition). Still to add: other families' replies, and the live half
      (valid-call rate on short scripted tasks).
- [x] **A.4 Suite A2 — navigation -- first version 2026-09-19.** Correct location found, actions and tokens
      to find it, re-reads of unchanged files. `suites/a2-navigation.json`:
      the nine repository questions of external-v1/v2, m6-hard-v1 and
      m5-frozen-v1, re-audited, plus one built from Part E's forty-action
      search for a test's class. First run 9 of 10: the miss was a decoder
      fault (an XML `1000` read as a number for search's text field; fixed,
      and added to A1); re-run of that case on the fixed binary, 10 of 10.
- [ ] **A.5 Suite A3 — editing.** **Status 2026-09-23:** first version built and run 6/6 (roadmap step 6); open because it does not yet tell two harness revisions apart on Qwen3.6 -- smaller models are the discriminating runs. Record: Applied first time, escape and whitespace
      failures, out-of-scope changes. *First version 2026-09-19:*
      `suites/a3-editing.json` over six self-contained tasks built by
      `suites/build_a3_corpus.py`, each checked against a reference fix and
      the recorded wrong fix; `sh suites/run.sh a3`, ~3 minutes on the MLX
      engine. First run 6 of 6, and it found one gap: a CRLF file took 14
      turns because a model cannot type a carriage return (fixed: edits keep
      a CRLF file's endings; the re-run took 7 turns, 8 of 8 edits applied, all first
      time). **Limit found the same day:** the binary with
      the JSON-escaping bug (`34990a4`) also passes 6 of 6 -- on a short task
      the model writes the regex itself rather than copying escaped text, so
      one seed of an easy task does not catch a probabilistic harness bug.
      Deterministic harness bugs belong in unit tests and replay (A1); A3
      needs harder tasks (larger files, several steps), several seeds, or a
      weaker model before it can tell two harness revisions apart.
- [ ] **A.6 Suite A4 — verification.** **Status 2026-09-23:** first version built and run 3/3 (roadmap step 6); open for verifier adoption in product-path mode. Record: Checks run, false completions, verifier
      adoption, completions declared against red checks. *First version
      2026-09-19:* `suites/a4-verification.json`, three tasks whose wrong fix
      is a shortcut past the check (skip expected failures, change the test,
      stop at a partial check), each refused by its hidden verifier
      (`suites/build_a4_corpus.py`). First run 3 of 3, no false completion;
      it caught a two-line fragment wiping a 25-line test file under the
      shrink guard's 40-line floor (lowered to 10). Verifier adoption
      (product-path mode) is not in it yet.
- [ ] **A.7 Suite A5 — context and compaction.** The corpus exists
      (`longhorizon-v2`, 53 tasks, 27 independent); the area, its metrics and its
      gate do not.
- [ ] **A.8 Suite A6 — end to end.** Needs a corpus of realistic tasks that does
      not exist, at the host's window, read against A.1.
- [ ] **A.9 Re-audit existing corpora into the six areas.** `external-v1/v2`,
      `m6-hard-v1`, `longhorizon-v1/v2`, `realistic-v1`, `longhaul-v1`.
- [ ] **A.10 Per-area metrics and gates** in the evaluator, so a campaign that
      names an area cannot report a metric from another one.
- [ ] **A.11 Incremental campaign reports.** Today the report is written at the
      end, so a campaign stopped part-way leaves nothing -- no per-task outcomes,
      no malformed kinds, no action counts. It has already cost 2 h 26 of work.
      *The run that most needs partial results is the one too slow to finish.*
- [ ] **A.12 Resident memory measured during a campaign.** Calibration observes
      memory pressure; evaluation observes none. One deployment held 44.9 GB
      resident of 64. Part D's larger windows make this a safety property, not a
      statistic.
- [ ] **A.13 Artifact schema migration.** A persisted artifact compares
      `schema_version` and an older one is refused rather than upgraded, so a
      schema change orphans recorded evidence.
- [ ] **A.15 Automatic configuration against a naive one.** The same tasks, the
      same host, PWR's computed configuration against the defaults a normal
      user would land on. It measures the product thesis of Part 0d rather than
      the model or the harness, and it is the number that belongs on a front
      page in place of a speed claim. **A cheap first version rides on A.2 at
      almost no extra cost**, since that diagnostic already runs the same tasks
      at the old regime's inherited 16,384 and at the computed window -- which
      is a naive configuration against an automatic one by another name. The
      full version belongs beside A.8. *Depends on: A.2 for the cheap form,
      B.1 and B.2 for the real one.*
- [ ] **A.16 The harness revision was the empty hash in 146 reports.** Found
      2026-09-18: `harness_rev` is stamped at build time from the commit and a
      BLAKE3 fingerprint of the source, and some builds could not see the
      source -- `git` failed and the fingerprint covered zero files, giving
      `unknown-source-af1349b9...3262`, which is BLAKE3 of nothing. Cargo
      built on regardless, so the label looked valid. 146 reports carry it,
      across the R2 pilot, the R2 harness revision and the R3 development run
      (and the Part E diagnostic, whose binary predates the fix): distinct
      harnesses under one identity, which `compare_strict` cannot tell apart.
      Their provenance survives only because every experiment ran a frozen
      binary named by its commit, recorded in its `run.sh`. **Fixed for new
      builds** in `build.rs` (an empty source tree is now a build error, and
      the manifest directory is read when the script runs rather than baked in
      when it was compiled). *Remaining:* the evaluator should refuse the
      empty-source revision as a pairing condition, and the affected analyses
      should say which binary each report came from. Why the build script
      could not see the tree is not established: it reproduced under cargo
      and not when the same script was run by hand.
- [ ] **A.17 Why PWR is slow where Bionic is quick -- investigation opened
      2026-09-18.** **Status 2026-09-23:** a large part answered since: reasoning off by profile (D.E2E-24), and the prompt cache that missed on every user message (D.E2E-21, fixed and confirmed: 5-11 s per step instead of 100-130 s). Long prefill itself remains. Record: In A.1 the maintainer drove Bionic through the five tasks in
      little time; in the Part E diagnostic PWR, same artifact, same window,
      took 8 minutes on the first task, the whole 60-minute task budget on the
      second (9 turns, three replies cut off) and 14 on the third. The window is
      no longer the constraint -- the largest prompt so far is 65,068 tokens and
      nothing compacted. Suspects, each with the test that would settle it, to
      run after the diagnostic ends rather than during it:
      1. **Sampling.** `strategies/models.json` has no profile for Qwen3.6 under
         any name, and profiles match the model reference exactly, so PWR
         sends no sampling at all (the report says `backend_default`). LM
         Studio's own model file recommends temperature 0.6, top-k 20, top-p
         0.95, and Bionic -- LM Studio's app -- applies them. Qwen's guidance is
         that thinking mode without them falls into endless repetition, which
         is what a 56,000-character reasoning block ending in a cut-off looks
         like. *Test:* the same opening prompt through the API with and without
         the recommended values, reasoning length compared; then a profile for
         `qwen/qwen3.6-35b-a3b@4bit` with the card's values. *The leading
         suspect, and cheap to fix.*
      2. **Reasoning length itself.** LM Studio ignores `enable_thinking: false`
         for this model (engine spike) and exposes a reasoning budget that is
         not set on the loaded instance. *Test:* the same turn with a budget.
         This is also C.15's first half.
      3. **Checks run by the harness.** A run verifies after edits, and a
         repository's own suite is not free; Bionic runs nothing it is not asked
         to. *Test:* time spent in verification from the trace against time in
         generation.
      4. **LM Studio's disk prompt cache**, observed full at its 68,000 MiB cap
         and evicting (2,500 records since the model loaded). Whether turns
         re-process their prompt from zero is not established. *Test:* time to
         first token per turn from `lms log stream --stats`.
      5. **Four parallel slots** on the loaded instance, and whether the MLX
         engine divides the window among them. *Test:* one slot against four.
      Until these are separated, no statement about PWR against Bionic is a
      statement about the harness.

      **Narrowed the same day from LM Studio's own server log**
      (`~/.lmstudio/apps/bionic/server-logs/2026-09/2026-09-18.1.log`, which
      covers the A.1 session and the diagnostic both):
      - *Suspect 4 is cleared.* Prompt-cache reuse was 96.5% for Bionic and
        88.2% for PWR; PWR is not re-processing its prompts.
      - *The difference is a tail, not the typical turn.* Bionic: 278 requests
        over the five tasks, turn median 7 s, 90th percentile 30 s, longest
        163 s. PWR: 61 requests, median 10.5 s -- comparable -- but 90th
        percentile 661 s and longest 783 s. About one PWR turn in ten runs
        eleven to thirteen minutes, and nothing like it happens in Bionic on the
        same model file. Those are the runaway reasoning turns.
      - So the cause is in **what PWR's requests ask of the model**, not in
        the engine: suspects 1 and 2 -- sampling, and an unbounded reasoning
        length -- lead, and the log does not record request parameters, so the
        controlled test is still needed. Bionic's requests go through LM
        Studio's SDK, not the REST API, and may carry a reasoning budget or a
        generation cap that PWR's do not.
      - **Suspect 1, sampling: cleared** by a controlled test
        (`experiments/a17-sampling-20260918`): with and without the recommended
        values, three seeds, every request reasoned into a 12,000-token cap with
        no answer.
      - **Suspect 2, reasoning: confirmed, and it decides the architecture**
        (`experiments/a17-reasoning-20260918`). On the same prompt, reasoning
        switched off answered in 53-74 s with the right mechanism twice out of
        twice; reasoning on ran three to ten times longer and was wrong twice.
        Through the OpenAI-compatible endpoint PWR uses it **cannot be
        switched off** -- the native `reasoning` field and the template's
        `enable_thinking` are both silently ignored; `/no_think` in the system
        prompt shortens it without stopping it. The switch belongs to the chat
        template, which pre-fills an empty `<think></think>` block, so only a
        caller that renders the template controls reasoning, budget included.
        LM Studio's native endpoint does not help: its tools would run outside
        PWR's policy and audit. *Interim:* `/no_think` for the Qwen family.
        *Real fix:* the embedded engine (open decision 4).
      - *Interim applied 2026-09-18:* a profile for
        `qwen/qwen3.6-35b-a3b@4bit` in `strategies/models.json`, with
        `/no_think` as a prompt directive and LM Studio's sampling recorded
        with its source. **A conflict it surfaced, left for a decision:** that
        file's tests require every deployment serving 262,144 tokens to have
        262,144 as its *minimum* (the old product requirement that a model
        which cannot see a large repository does not qualify), while Part D
        makes the window follow the host. The minimum is enforced nowhere on
        the computed path today, so the profile complies with the rule rather
        than quietly rewriting a product requirement; it has to be settled
        before a small-host tier is catalogued.
      - *And it validates B.2.* LM Studio's own auto-fit logs
        `full_kv=20480B/token`, the figure `window.rs` computes, and a peak of
        41.06 GiB at 262,144 against about 45 GiB from `window.rs`. It models
        the prefill transient per token by component (`prompt_inputs=4096`,
        `attention=65536` bytes) from the attention shape, not as a multiple of
        the cache -- the better model for `PREFILL_TRANSIENT_FACTOR` to move to,
        and the one that would stop overstating a dense model like Seed-OSS. It
        also takes its ceiling from Metal's recommended working set (51.84 GiB
        here) less a 3 GiB reserve, where `window.rs` takes three quarters of
        physical memory.
- [ ] **A.14 A legibility rubric.** A6 names "human-rated legibility of the run"
      as a primary metric. There is no rubric, no procedure and no rater.

## Part R — From the external review of 2026-09-23

Each item was checked against the code before being written here; the
verdicts are in [`external-review-2026-09-23.md`](external-review-2026-09-23.md).
Ordered by what they protect, not by size.

- [x] **R.1 CI green again, and checked the way CI checks (2026-09-23).** The
      public `main` went red with the 2026-09-23 push (`cargo fmt --check`),
      and clippy with `-D warnings` had never been run on the branch. Fixed
      locally (formatting, four lints); a change is now verified with `fmt
      --check`, `clippy --all-targets -D warnings`, `cargo test` and the
      milestone check, judged by exit codes. Pushed; the first CI run of the
      branch's code then failed two tests that pass on the maintainer's Mac,
      and CI now names failing tests as annotations, readable without an
      account (job logs are not). Both were real: the sandbox allowed reads
      of `/Applications/Xcode.app` only, so `git` from a renamed Xcode
      (`Xcode_15.4.app` on the runner, `Xcode-beta.app` on some Macs) could
      not read its own configuration -- every `/Applications/Xcode*.app` is
      readable now; and the GGUF routing test asserted a 262K window that only
      a large-memory host computes, where the runner correctly refuses it --
      the test now accepts either the trained length or an explained refusal.
- [x] **R.2 Conversation defaults that ask before they reach -- done 2026-09-23,
      as two modes the maintainer chose.** `permission_mode` in the workspace's
      chat config: **Ask** (the default) asks before `DependencyChange`,
      `NetworkAccess`, `ToolchainInstall`, `HistoryRewrite` and `Publish`;
      **Auto** grants every permission -- the question is lifted, the sandbox
      and the policy's limits are not. A config saved before the modes that
      still holds the old default nobody chose is migrated to Ask; a list
      someone chose is kept. `_pwr/approvals` takes and returns `mode` (and
      `asking`, what is actually asked now); the app has an Ask/Auto switch
      beside Goal, amber in Auto. Tests:
      `chat_approvals_follow_settings_and_session_grants`,
      `configurations_from_before_the_modes_are_migrated_to_ask`. With Ask as
      the default, the installed-dependency guard (D.E2E-29) holds in the app.
      Was: **R.2 Conversation defaults that ask before they reach.** The
      conversation pre-grants every approval except history rewrite and
      publish -- so dependency changes, network access, toolchain installs,
      local services and verifier adoption happen without a question, and
      the installed-dependency guard (D.E2E-29) is inert in the app. Proposed
      default: ask before `DependencyChange`, `NetworkAccess` and
      `ToolchainInstall`; keep `LocalService` and `VerifierProposal` granted
      within the workspace. Settings already carry `ask_before`, so this is a
      default and a migration for existing configs, not new machinery.
      **The maintainer's decision.**
- [x] **R.3 Say when commands are not confined -- done 2026-09-23.**
      `SandboxPolicy::Preferred` now **refuses** a command it cannot confine,
      unless the person sets `PWR_ALLOW_UNCONFINED=1`; one decision shared
      by `will_sandbox` and `prepare_command` so they cannot disagree
      (`a_command_that_cannot_be_confined_is_refused_by_default`).
      `_pwr/approvals` reports `sandboxed`, and the app shows "commands not
      sandboxed". SECURITY.md rewritten to the current policy; its section on
      an unconfined re-run described a fallback removed on 2026-09-13 and is
      now history. Left: a sandbox state in the terminal console's header, and
      the Linux and Windows adapters themselves. Was: **R.3 Say when commands
      are not confined.** `SandboxPolicy::Preferred`
      runs a command unconfined where the platform has no sandbox (every
      platform but macOS), and nothing tells the person. Show the sandbox
      state in the app and the CLI header; refuse by default outside macOS
      until an adapter exists (Linux: bubblewrap or Landlock; Windows: a job
      object and AppContainer), with an explicit opt-out.
- [ ] **R.4 Keep llama.cpp's server alive across turns.** **Status 2026-09-24:** not needed for the macOS release, where the app runs MLX only; required before Windows. `llama-server` now binds `127.0.0.1` only (`PWR_LLAMA_HOST` removed). `LlamaProvider::chat`
      starts `llama-server` per generation and kills it after one stream, so
      every turn pays the model load and loses the KV cache; `prepare_context`
      reports the requested window without asking the server. Before any
      llama/Windows performance claim: one server per deployment and window,
      reused across turns, and the served context read back. Validate with a
      multi-turn live smoke counting loads and time to first token.
- [x] **R.5 README and platform claims that match the code -- done
      2026-09-23:** status table as of that day (what runs live, what is
      opt-in, llama.cpp as limited because its server reloads every turn),
      setup through `setup-mlx.sh` and the Tauri app, the two permission modes
      and the macOS-only sandbox. Was: **R.5 README and platform claims that
      match the code.** The README
      describes code "inspected on 2026-09-13", says nothing has run against a
      live model, and calls llama "metadata inspection". Rewrite the status
      from `roadmap.md`'s current table; state supported platforms plainly
      (macOS on Apple silicon today; llama.cpp and Windows in progress).
- [ ] **R.6 A licence inventory.** Generate the Rust (`cargo-deny` or
      `cargo-about`), npm and Python licence lists, check them in CI against
      an allow list compatible with Apache-2.0.
- [x] **R.7 `.gitignore` for secrets and weights -- done 2026-09-23.** Was: Add `.env`, `.env.*`
      (keeping `.env.example`), `*.gguf`, `*.ggml`, `*.safetensors`, `*.pem`,
      `*.key`, `*.p12`, `*.pfx`. Nothing of the kind is tracked today.
- [ ] **R.8 The app in CI, with tests.** The Angular client has no spec files
      and CI does not build it. First: a build job and tests for the agent
      store (queue, steer, streaming, tool upserts), which is where the app's
      bugs of 2026-09-22 were.
- [ ] **R.9 Decide what research evidence is public -- first step done
      2026-09-23, by the maintainer's decision: removed from the public
      repository for now.** `.pwr/models` and `.pwr/calibrations` (108
      files) are untracked and ignored; they stay on the maintainer's machine,
      where the roadmap's numbers can still be checked. Still open: which of
      them to publish again, anonymised, as fixtures. Was: **R.9 Decide what
      research evidence is public.** `.pwr/models` and
      `.pwr/calibrations` are tracked on purpose, so numbers in the roadmap
      can be checked. The public branch carries no personal path; it does carry
      one machine's observations. Keep, anonymise into a fixtures folder, or
      drop from the public tree -- **the maintainer's decision.**
- [ ] **R.10 The research thesis and its metric.** Adopted as the frame for
      small-model work: *can a local harness raise verified success on
      repository tasks for 7–14B models at equal model, hardware and
      permissions, and by how much against a fixed tool-loop baseline?*
      Headline: absolute paired uplift in percentage points against that
      baseline (not against the bare model); failure reduction only when the
      baseline fails often enough; per-task paired analysis with intervals
      over tasks and seeds. C.22's small-model experiment is its first
      instance. **The baseline arm already exists** (found 2026-09-23 while
      scoping it): B0, `run_conventional_loop` in
      `crates/pwr-orchestrator/src/baseline.rs`, reachable as `pwr eval
      run --arm b0` -- same action protocol, parser, family adapter,
      policy-enforcing executor and event log as PWR (B1), and only the
      loop differs: no planning, no summary compaction, no loop detection, no
      recovery at a lower tier, acceptance left to the hidden verifier. B2 (a
      fixed staged workflow) is there too, and `compare_strict` pairs arms and
      refuses undeclared differences. So the study needs no new machinery: it
      is a campaign -- suites A3 (editing, 6 tasks) and A4 (verification), then
      A2, with Qwen3-14B under `b0` and `b1`, at least two seeds, compared
      strictly. Estimated at 4-6 hours for A3 and A4 at the 14B's speed. One
      gap to close first: evaluations run the lexical ranking only (so a
      measured regime cannot change with what is installed); the semantic
      ranking needs to become a *declared* condition of a campaign before C.22
      can be measured this way.
- [ ] **R.11 One runtime contract for chat and scripted runs.** Accepted as
      direction; not first. Shared fixtures for plans, state, compaction,
      completion and recovery, then converge.

## Block B — Model and engine

- [x] **B.1 Loading without a probe -- done 2026-09-18, not yet run against a
      live backend.** `pwr run`, `eval run` (`--profile` now optional),
      automatic routing and the console's model preparation no longer require
      or run a calibration. Without one they compute the window (B.2), load
      the model at it, read back what the backend served, and build an
      execution profile labelled `Computed` whose id is derived from what
      decided the window, so two campaigns under the same conditions still
      pair. The run and campaign outputs carry a `window` record: every
      ceiling, the binding one, where the facts came from. A named
      calibration is still honoured and checked. `bootstrap_profile` is gone;
      its label stays readable in old artifacts. Open decision 1 turned out
      not to block this -- both of its answers drop the probe as a gate -- and
      now only decides what F.3 deletes. **Found on the way: the console's
      chat defaulted to 8,192 tokens**, below even the campaigns' 16,384, and
      its "maximise" step raised it to the model's trained length with no
      memory check. Both now go through the computed window; a console saved
      under calibration is recomputed once on its next start. *Remaining:
      speed observed from real turns (the latency ceiling), and the first
      live run, which is step 3.*
- [x] **B.2 Computed window** (Part D), wired into every command 2026-09-18
      with LM Studio reading the model's `config.json` from disk (through its
      hub manifest, narrowed by variant, local endpoints only) and Ollama its
      GGUF metadata; an unknown memory ceiling brings in a 32,768 fallback
      unless a window was chosen explicitly. *Still open: GGUF under LM
      Studio has no config and falls back; the transient constant.* *First
      increment done 2026-09-18:*
      `pwr_orchestrator::window` computes the window from a model's
      `config.json` and the host's memory, keeps every ceiling and names the
      one that bound it; 12 tests, and it parses all four configs on disk
      (Qwen3.6-35B-A3B 20,480 B/token as the spike measured; Qwen3.8-27B,
      Seed-OSS-36B dense, gpt-oss-20b with sliding layers). *Not yet wired
      into any command.* *2026-09-18: the engine's own prefill proved the
      point -- a fixed 8,192-token step materialised 41.9 GB of attention
      scores at ~160k tokens and crashed a run the window said fitted; the
      sidecar now shrinks the step with the context. That removes the crash but
      not the cost: a cold 240,916-token prefill took 1,323 s (182 tokens/s),
      because the scores are still materialised per step. A fused attention
      path for prefill is the engine's next performance item.* *2026-09-19,
      measured and replaced for the engine:* MLX 0.32 fuses prefill attention
      for head dimensions 64, 80 and 128 and materialises scores for the rest
      (Qwen3.5/3.6/3.8 use 256). The engine's peak is weights + cache + the
      larger of a cache copy (while it grows) and the scores, when not fused:
      Qwen3.6 at 240,916 tokens 36.1 GB measured vs 35.7 predicted, Seed-OSS
      at 60,112 tokens 52.0 vs 51.9. The sidecar answers `attention` for a
      head dimension without loading anything, `ModelFacts` carries
      `prefill_scores_bytes`, and `window::decide` uses it; the step shrinks
      only when attention is not fused. Windows on a 64 GiB M2 Max: Qwen3.8
      107,520 -> 262,144, Seed-OSS 23,552 -> 59,392. The 4x rule remains for
      backends that cannot say (LM Studio, Ollama). Its weakest constant was the prefill transient,
      modelled as 4x the cache from one observation: it decides Seed-OSS's
      window between about 23k and 119k tokens, so it is the first thing to
      measure per architecture. Original scope: KV bytes per token from the model
      config, less sliding-window and hybrid layers, plus the prefill transient
      the spike measured at roughly four times the cache; latency ceiling from
      observed prefill; **and a third ceiling from published effective context**,
      because a trained maximum is an optimistic number (Part 0b). Which ceiling
      bound the window is recorded and shown.
- [ ] **B.3 Hardware catalogue.** Entries of (model, quantisation, engine) with
      weights' memory, KV bytes per token, trained maximum, measured speed, and
      the areas each has been evaluated in. The redesign's tiers are
      placeholders with no measurement behind them. **Built from the public
      prior plus arithmetic (Part 0b), not from campaigns**; only the two or
      three shortlisted models are ever measured in A1-A6. *MLX 64 GiB pass
      closed 2026-09-19:* Qwen3.6 is the default, Nemotron 3.5 Lightning the
      second fast candidate, gpt-oss useful with C.19 open, Qwen3.8 correct but
      slow, GLM-4.7-Flash and Seed-OSS excluded. The remaining catalogue work
      is A2 on promising models, the 16 GB class (B.4), and the equal-memory
      quantisation pair (B.3b).
- [ ] **B.3a Shortlist from public data.** `config.json` arithmetic for the
      memory and window question; BFCL, RULER and the agentic coding
      leaderboards for the capability prior. A paper exercise, no downloads.
      *Criteria agreed 2026-09-19:* decoder-only, Instruct or Coder, native
      tool calls in the chat template, preferably MoE with few active
      parameters, weights within ~35-45 GB on a 64 GB host; candidates named
      by the maintainer (a Nemotron and a 70-80B were raised) go through this
      paper check first.
- [ ] **B.3b Quantisation pairs at equal memory.** Carry a ~14B at 8-bit beside
      a ~30B at 4-bit in the catalogue so the trade-off is measured rather than
      assumed. *Blocked by open decision 6.*
- [ ] **B.4 The 16 GB class.** *Blocked by open decision 3: a 16 GB machine, or
      a memory cap the engine enforces on this one.*
- [x] **B.5 / B.6 The embedded MLX engine -- first version done 2026-09-18**
      (roadmap step 4, moved up). `crates/pwr-mlx`: a Python sidecar over
      `mlx-lm` (open decision 2 taken: sidecar, not C-API bindings, because
      mlx-lm is the reference implementation of the architectures) that
      renders the chat template itself -- reasoning off or budgeted, which LM
      Studio's endpoint ignores -- reuses the prompt cache across turns through
      a checkpoint, and stops a generation that loops; one sidecar per PWR
      process; calls read with the family adapter so they arrive structured.
      `--backend mlx`. The capability probe passes on Qwen3.6 in 148 s, edits 3
      of 3 (`experiments/mlx-engine-20260918`). It is a backend beside the HTTP
      ones rather than behind a separate `Engine` trait: the provider traits
      already were that boundary. *Remaining:* shipping a Python runtime with
      the app; the probe applying the model profile. *Done 2026-09-19:*
      streaming the answer text up to where a call may begin; stopping a
      generation on cancel (0.02 s) rather than draining it.
- [ ] **B.7 llama.cpp engine** **Status 2026-09-23:** generation works through a managed `llama-server`, but the server is started per generation (R.4); not a performance path yet. Record: (Windows, and comparable on macOS). *Required by
      the Windows build.* *Started 2026-09-20:* `pwr-llama` reads GGUF
      metadata and exposes it through the runtime backend as `--backend llama`;
      generation is deliberately refused until PWR owns a `llama-server`
      process and streaming/cancellation are wired. *Next increment, same
      day:* `LlamaServerPlan` now fixes the model path, loopback host/port and
      `--ctx-size` arguments without starting the process; a configured port
      of `0` is resolved to a concrete reserved loopback port so the HTTP
      endpoint is known before launch. The remaining work is lifecycle
      ownership, HTTP streaming, cancellation and version reporting. The
      OpenAI-compatible chat-completions request body is fixed offline,
      including tools, seed and sampling; `LlamaStreamDecoder` parses
      OpenAI-style stream events, accumulates partial tool calls and reports
      non-stop finishes as truncation without needing a live server, while
      `LlamaSseDecoder` handles transport chunking, CRLF and `[DONE]`. The
      HTTP client path now posts the request and converts an SSE response into
      PWR's `ModelStream` against a local fixture. `LlamaServer` now owns a
      spawned server process with kill-on-drop and readiness polling against
      loopback, and `LlamaProvider::chat` starts that server on demand and
      streams replies through the standard provider boundary. `backend_version`
      records the server's `--version` output, and provider cancellation stops
      an in-flight SSE stream against a live fixture. Remaining before using it
      for measurements: a real `llama-server` + GGUF smoke run on this machine.
      *Checked 2026-09-20:* this host currently has neither `llama-server` on
      `PATH` nor a `.gguf` under `~/.lmstudio/models`, so the smoke is blocked
      on installing/building llama.cpp and placing or pointing PWR at a GGUF.
      *Unblocked and smoked the same day:* Homebrew installed `llama.cpp`
      0.4.1 (`llama-server` build 10964), and the ignored real-smoke test
      passed against the local Qwen3 0.6B Q4_K_M GGUF under
      `PersonalTrAIner/artifacts/gguf` using `PWR_LLAMA_MODELS`,
      `PWR_LLAMA_SMOKE_MODEL` and `PWR_LLAMA_SERVER`.
      *Follow-up found and fixed by the smoke:* `models inspect --json` could
      emit and persist megabytes of tokenizer metadata for GGUF files; routine
      inspection artifacts now summarize large GGUF arrays while the full
      metadata still feeds digest, `ModelFacts` and window computation.
      *Measured path opened 2026-09-20:* Nemotron 3.5 Lightning 30B-A3B Q4_0
      downloaded through B.8, probed through `--backend llama`, and passed A4
      3/3 plus A3 6/6 with no false completions. The run also fixed the
      admission gate for backends that can control the served context window
      but cannot report prompt token counts.
- [ ] **B.8 Weights from HuggingFace.** Resumable, verified downloads of
      twenty-gigabyte artifacts; disk management; licence-gated repositories
      needing an accepted agreement or a token; two catalogues, because MLX and
      llama.cpp take different formats. *Same decision as B.5.* *Started
      2026-09-20:* HuggingFace artifacts in `strategies/artifacts.json` can now
      be turned into a deterministic, non-network `models download-plan`: full
      commit revisions are required, file paths must be safe relatives, and the
      output names the pinned Hub URL plus the local destination for every file.
      The registry also accepts per-file `bytes` plus `blake3` or `sha256`, and
      `models download` executes the plan only when every file has a size and a hash; it
      skips already verified files, resumes `.part` files with HTTP Range, and
      verifies before renaming. The execution path is covered by a local HTTP
      fixture, including fresh download, already-present and resumed-part
      cases, so it is tested without relying on the public network. Still open:
      licence-gated access beyond `HF_TOKEN`, and product-level disk-pressure
      UX. The CLI now has a download preflight: it counts only missing bytes
      after verified files and `.part` fragments, then requires a 5 GiB free
      margin before starting network IO.
      *Same pass:* the registry now includes two real GGUF candidates with
      pinned Hub commits, byte counts and SHA-256 from resolve headers:
      Qwen3.6-35B-A3B Q4_K_M (`unsloth`) and Nemotron 3.5 Lightning 30B-A3B
      Q4_0 (`ggml-org`). *Nemotron downloaded and smoked 2026-09-20:* the
      18.9 GB GGUF was fetched, SHA-256 verified, inspected through
      `--backend llama`, and passed the ignored real-smoke test against
      `llama-server`. *Measured later the same day:* after a capability probe
      wrote deployment evidence, the same GGUF passed A4 3/3 and A3 6/6 on
      `llama-server`, with all completions declared, none false, and task
      times between 0.9 and 2.9 minutes.
- [x] **B.9a Tool-guard tolerance, found through the engine (2026-09-18).**
      Edits carry the file's 64-character hash, which the model has to copy;
      Qwen3.6 at 4-bit doubled one letter and every edit was refused as stale.
      A claim within two edits of the current hash, or a twelve-character
      prefix, now names the file; a changed file's hash is sixty-odd edits
      away, so the guard keeps its purpose. The same run found Qwen3.6 writes
      calls as XML, which the Qwen adapter now reads (`qwen-v2`).
- [x] **B.9 Constrained decoding of tool calls -- done 2026-09-20.** The
      llama.cpp backend now marks native tool choice as required whenever it
      sends a tool catalogue. `llama-server` derives the tool grammar from that
      catalogue, so the generated call is restricted to an offered name and
      argument schema instead of asking the harness to parse a prose-shaped
      request. The backend advertises `constrained_tool_calls`, its request
      body is pinned by unit tests, and a live Nemotron smoke asked for an
      unoffered `write_file` while exposing only `read_file`: it returned one
      valid `read_file` object. This is a syntax guarantee, not a semantic one:
      the harness still rejects an inappropriate but well-formed action through
      its policy and tool guards. *CLI alignment, 2026-09-20:* the terminal
      console is kept as the thin manual/research control surface rather than
      removed before the app. `chat --model` now records an explicit MLX or
      llama.cpp selection,
      computes its window and opens a supervised chat without a probe;
      capability probes are optional research diagnostics. The current manual
      GGUF command is in `docs/current-cli.md`. Scripted `run` and `eval` still
      have their legacy evidence admission and must be aligned with B.1 before
      the frontend lab treats them as product-ready.
- [x] **B.10 Frontend lab after constrained decoding.** **Closed 2026-09-23 as superseded:** the Tauri app became the manual-test surface directly (streaming, actions, stop, sessions, permissions), without a separate lab. Record: Start once B.9 has made
      the tool-call channel stable enough not to redesign the UI every week.
      This is not the final product surface: it is the manual-test app for the
      next phase, covering local model selection/download status, run launch,
      live action/log streaming, stop/cancel, artifacts, and suite/manual task
      replay. Its job is to make engine and harness behaviour visible while R3
      resumes and before S2/S3 harden the toolkit/product decisions.

- [ ] **B.11 Unknown models and Reasoning Effort -- built 2026-09-24,
      alpha hardening.** A model without a profile is Provisional and usable
      with conservative defaults, never "unsupported"; Quick Calibration
      (nine fixed, mechanically scored requests: termination, instruction
      following, JSON, code, file selection, tool selection, schema-valid
      arguments, tool-result continuation, reasoning) moves it to Locally
      calibrated or Limited (agent tasks withheld, chat kept); Incompatible
      only on concrete evidence. Evidence carries full provenance (artifact
      digest, weight files, quantization, tokenizer, chat template, revision,
      backend and engine version, suite version, hardware class) and is
      reused, reused with reduced confidence, or treated as stale by fixed
      rules; local results live in `~/.pwr/model-evidence`, never in a
      repository. Reasoning Effort (Low/Medium/High, Medium default) is a
      thinking budget per generation, read from the chat template's own
      markers (native budget, explicit `<think>` stream, template level, or
      none/unknown), clamped to the context with a 4,096-token answer reserve,
      enforced by the engine with one forced close and one turn-level retry
      with thinking off. Decided with the evidence at hand: no model is
      Verified for the alpha (the registry is empty -- old evaluations lack
      artifact provenance); a Provisional model's window is **not** lowered
      below the computed one (a starved window is the known failure).
      Measured on this host: Qwen3-14B (no profile) Provisional → Locally
      calibrated in 21 s; Low closed at exactly 2,048 thinking tokens and
      answered. *Open:* calibrate the catalogue models so their budgets are
      measured rather than conservative; a Verified entry once an evaluation
      is re-run with provenance; budget control for harmony (gpt-oss) and
      llama.cpp. Details: [`model-compatibility.md`](model-compatibility.md).

## Block C — Harness capability

- [ ] **C.1 Unify compaction.** The two loops still hold two algorithms, kept
      apart because R3's control arm was today's compaction. With R3 shelved the
      reason has lapsed and the decision should be retaken. *2026-09-23: the
      conversation's automatic and manual ("Compact now") compaction are now one
      function (`pwr_orchestrator::compaction`), audited as
      `context.compacted` with its trigger; the scripted run's ledger
      compaction is the half still separate.*
- [ ] **C.2 Retune the action budget and the stall guard.** Both sized under the
      old regime; 40 actions is what a starved run needed. *2026-09-18: on the
      MLX engine at 262,144, Qwen3.6-35B-A3B went from 2 to 4 of 5 between 26
      and 64 actions once orientation was cheap, and used 20-52 actions per
      resolved task; a per-model strategy holds 64 until A6 decides the
      shared default.*
- [ ] **C.3 Decide `HISTORY_BUDGET_SHARE` on evidence.** One constant, currently
      0.5, currently the largest known lever.
- [ ] **C.4 Typed local decisions.** The loop's small choices -- relevant or
      not, compact now, is this check red, is it finished -- as a constrained
      decoding over a couple of tokens or a small local classifier, instead of
      generated prose. Belongs to A4 and A5, where what it replaces is already
      measured.
- [ ] **C.5 Mechanical work the harness should take off the model.** Manifest
      and dependency discovery; call sites and tests related to a file; ranking
      and de-duplicating context; token accounting; selecting the checks a
      change requires; correlating a diagnostic to file and line; generating a
      diff; counting edits and recovery attempts; classifying a failure;
      reproducing a flaky one. Each of these currently spends actions from a
      budget meant for thinking.
- [ ] **C.6 Semantic and procedural memory, full artifact rehydration.**
      Experimental, unproven.
- [ ] **C.11 The harness diagnoses its own operating regime.** Proposed
      2026-09-17 and the cheapest high-value item on this list. The run already
      holds every number needed to know it is in a bad regime -- compactions per
      action, history budget in tokens, share of re-reads that follow a
      compaction, actions spent without changing the workspace or the checks --
      and it reports none of them as a *regime*, only as after-the-fact
      statistics. A run that had said "I have compacted 30 times in 40 actions
      and my history budget is 5,000 tokens" would have exposed the old regime's
      central mistake months earlier, to the user in the moment and in every
      campaign record. **Diagnosis, not adaptation**: it reports, it does not
      change itself, so it costs nothing in attributability and needs no
      experiment to justify.
- [ ] **C.13 A repository map built on first open -- experiment to pursue.**
      Proposed 2026-09-18: index the repository once, so a problem about
      login loads the files login touches and not the whole tree. Half of it
      exists: `pwr-repo` persists an index incrementally by content hash,
      with each file's symbols, its imports (as names, unresolved) and the
      source it tests by naming convention, and `retrieve` ranks lexically and
      then walks the import graph from its top three seeds. Never shown to
      help, and A.1 says it was not the constraint at a large window. What the
      proposal adds, in order: **imports resolved to files**, per language;
      **reverse edges** -- who calls `login()`, the question a bug usually
      asks; **retrieval as a tool** the deployment calls mid-run, not only at
      the opening; and, only for the gap lexical matching cannot close
      ("login" against `authenticate`), **model-written summaries or
      embeddings, labelled unverified** and never a source of truth. The graph
      itself is facts and is computed, per MASTER_SPEC's first principle: a
      model-written dependency map would be wrong in places, stale after a
      commit, and hours of local inference on a first open. Its value is
      largest exactly where PWR's user is -- a small host or a
      memory-bound model (Seed-OSS gets about 23k tokens here) -- so it is
      measured in **A2 at a deliberately small window** against today's
      lexical retrieval. *After roadmap step 3.*
- [ ] **C.14 Tasks as a graph of small-context nodes -- experiment.** Raised
      2026-09-18 from "graph engineering" (LangChain, Salesforce's Agent Graph).
      Not a replacement for the harness: the sources describe harness, loop and
      graph as layers, and LangChain itself says forcing an inherently agentic
      task onto a fixed path is the wrong move -- which an unfamiliar-repository
      coding task is, and MASTER_SPEC's open question 4 already refuses a
      plan-first default without a comparison. Neither source publishes a
      measurement. Two things transfer. **The harness's own small decisions as
      deterministic nodes** -- that is C.4, now with outside support. And
      **nodes with their own clean context**: locate, then edit, then verify,
      each an agent run with a small window, which could beat one long loop on
      a 16 GB host or a memory-bound model -- PWR's own user. Those nodes are
      areas A2, A3 and A4, so each can be measured alone. Measured in A6 **at a
      deliberately small window**, against today's single loop, after the A1-A4
      suites; not adopted before.
- [ ] **C.15 Recover a runaway reply instead of discarding it -- experiment.**
      Raised 2026-09-18 from the Part E diagnostic, where Qwen produced 56,059
      characters of reasoning in one turn, ran past the reply bound, and lost
      the turn -- three times on one task. Today a malformed *call* is already
      recovered well (the model is told exactly which argument was wrong and
      which tool it meant; seen working in the capability probe). A runaway
      *reply* is not: the reasoning is thrown away, so the next turn reasons
      again from nothing, and the message sent back ("do less in one turn: one
      or two tool calls") diagnoses the wrong fault -- the model was making no
      calls, it was thinking. Three changes: **a reasoning budget the harness
      enforces on the stream** (stop at, say, 4,000 tokens of reasoning with no
      action, rather than at the 32,768-chunk bound minutes later); **feed the
      tail of the reasoning back** ("your reasoning reached this; do not redo
      it; the next step is one tool call"); and **name the right fault**.
      Cautions: re-supplying reasoning can anchor a wrong path, and Qwen's
      template deliberately drops earlier turns' reasoning, so this works
      against its training. Measured in A1, whose metrics are recovery rate and
      turns lost. Related: A.17's first two suspects.
- [ ] **C.21 Framework skill packs -- experiment, proposed 2026-09-20.** A
      prompt or repository that clearly names a framework (`Angular`, or
      `angular.json` / `@angular/core` on disk) may cause the harness to add a
      small, reviewed skill packet to the context: expected project layout,
      commands, verification habits, common edit traps and tool-use guidance.
      This is context policy, not magic training data: the packet must be
      short enough to compete honestly for the prompt budget, its trigger must
      be deterministic and recorded ("loaded Angular skill because ..."), and a
      model may propose a missing skill but not install one as evidence. It is
      measured as a declared treatment against a control with no skill, first
      on a framework-specific suite (for example Angular component, routing,
      form and build/test tasks with hidden checks), then in A5/A6 if it helps.
      Metrics: completions, false completions, actions, tool refusals,
      out-of-scope edits and verification cost. Blocked behind the engine work
      in step 8; can be prepared offline as corpus and packet drafts. *Manual
      product finding 2026-09-21:* an Angular standalone application bootstraps
      `App` from `src/main.ts` and renders `app.html`, but a Goal-mode task
      concentrated on a separately-created `app.component.*` tree, leaving the
      generated root page live. The lexical context was contaminated by that
      already-dirty tree and the existing unit test still asserted the default
      title. Prepare the Angular packet together with a deterministic entrypoint
      map and browser acceptance fixture; a prose reminder alone is not a
      sufficient treatment. *Implementation preparation 2026-09-21:* a short,
      deterministic Angular standalone packet and an entrypoint-map prompt
      section now load for `angular.json` / `@angular/core` workspaces. Their
      presence is recorded in compiled context as `workspace_topology` and
      `framework_guidance`; held-out Angular tasks must still measure whether
      this treatment improves the stated metrics before it becomes a default
      claim.
- [ ] **C.22 A fast encoder as the context filter -- idea, proposed 2026-09-22
      by the maintainer.** A small encoder/classifier (the maintainer's
      reference is a "TypeSafe Jev" architecture; not yet identified or
      verified here -- its speed and accuracy are claims to measure, not facts)
      runs before the generative model and returns typed, scored choices, so
      the model receives a prepared desk instead of everything:
      (1) **tool subset** -- only the tools the request needs go into the
      prompt (small gain today: the chat offers 23 tools; large once MCP or
      services multiply them); (2) **file and document relevance** -- mark
      which files or document sections matter for the current request or
      compiler error, then read only those (the largest measured gain:
      D.E2E-15, whole documents read at ~75 tok/s of prefill cost minutes);
      (3) **intent routing** -- read-only / file mutation / shell execution,
      to choose the instructions and context the turn gets. Constraints from
      MASTER_SPEC: the classifier *proposes*, deterministic code *decides* --
      permissions, sandbox and approvals stay policy, never a model's
      judgement, so (3) may add context or ask earlier but never grants or
      skips a guard; a wrong exclusion must be recoverable (the model can still
      ask for a tool or file that was filtered out, and the miss is logged); a
      fallback to today's full set when the classifier is unsure. Related:
      C.4 (typed local decisions, which this generalises), C.5 (mechanical
      work taken off the model), the repository graph's "embeddings labelled
      unverified", and C.23. First step when resumed: identify the model and
      its licence, then measure (2) offline on the website task's documents
      against the lexical ranking the context already uses -- precision of the
      selected sections, tokens saved, and whether completion holds.
      **Started 2026-09-22** (`experiments/jev-context-filter-20260922`, on the
      maintainer's machine; `pwr repo rank` prints what a turn would be
      given). Corpus: every tracked document split at headings -- 419 sections,
      200K tokens. Twelve requests a website session makes, labelled by the
      author: a fixed target for comparing rankings, not independent
      annotation. **The ranking a turn was given found the answering section
      for one request in twelve** (precision 0.03, recall 0.04); it ranks whole
      files by the terms they use anywhere, so a question about the engine
      returned `main.rs`. Section-level BM25 over the documents was four times
      better with no model, so the harness now retrieves documents by Markdown
      section -- BM25 over the whole section, shown truncated, weight on terms
      in the heading, at most two sections per document and half the passages
      so code still competes, constants swept on the set: **0.10 / 0.14 using
      three of five slots, no more tokens delivered**
      (`a_document_is_retrieved_by_the_section_that_answers`). The encoder arm
      has not run -- no embedding model and no torch on this machine, and the
      download needs the maintainer's approval; candidates and licences are in
      the experiment's `candidates.md`. **"TypeSafe Jev" identified
      2026-09-22** from the maintainer's link: TypeSafe AI's first "System One"
      model, early access, *hosted*. Unstructured state in, typed decisions
      out with a calibrated confidence, no string generation, claimed
      70-500 ms and $0.042/MTok input on the vendor's own evidence. **It runs
      on their service, not on the maintainer's Mac**, so it cannot be a
      dependency of a local-first product; the defensible reading is that it
      is the *shape* to implement locally -- a typed decision, a confidence,
      and deterministic code that decides -- with Jev as a ceiling to compare
      against if early access is granted. An opt-in cloud accelerator is the
      only other honest position and is a privacy decision to write down
      before it is built, not a technical one. Either way the calibrated
      confidence argues for the fallback C.22 already requires: below a
      threshold, the harness delivers the full set.
      **The maintainer decided 2026-09-22: no external cloud services**, so
      Jev is a reference and the shape is to be built locally.
      **First local arm run the same day** (`run_local_decider.py`, nothing
      downloaded): BM25 shortlists sections, then `Qwen2.5-1.5B-Instruct`
      already in the cache answers one typed question per candidate -- does
      this section answer the request -- with no text generated at all: one
      forward pass, the probability read from the softmax over the Yes/No
      logits. **Precision@3 0.22 and recall 0.18 against the shipped ranking's
      0.10 / 0.14, at ~2 s per request** when each candidate is shown 300
      characters (the same answer as at 1,200, so the cheap version is the one
      to build). Three findings shape what comes next: (a) the *shortlist* is
      the ceiling -- only 28% of the labelled sections are in BM25's top 20,
      and widening to 60 raised the ceiling to 42% while recall barely moved,
      so candidate generation (an embedding index, one local model downloaded
      once) is now worth more than a better decider; (b) **the confidences are
      not calibrated** -- they cluster near 0.5 and do not track correctness,
      and calibration is what the "below this threshold, deliver everything"
      fallback would rest on, so it must be measured before a threshold is
      chosen; (c) the decider **cannot be the engine's model**: one prompt
      cache serves the conversation and is worth ~100 s of prefill per step
      (D.E2E-21), so a mid-turn decision call would evict it. A filter is a
      second small model in its own process (~3 GB at bf16, ~1 GB at 4-bit).
      **Local embeddings measured 2026-09-23** (download approved by the
      maintainer; `mlx-embeddings` on MLX in an isolated venv, models cached
      once and run offline). `multilingual-e5-small` (0.5 GB, MIT) alone:
      P@3 0.25 / R@3 0.20 at **5 ms per request**, 4 s to index 420 sections --
      as good as the 1.5B decider at a four-hundredth of the cost. **Fused with
      the section BM25 by reciprocal rank: 0.28 / 0.22, the best arm measured.**
      `bge-m3` (1.1 GB, MIT) is the best candidate generator (recall@20 0.44
      against BM25's 0.28) and the weaker ranker -- after its pooling was
      corrected: the library mean-pools, bge-m3's vector is the first token,
      and the wrong pooling had measured it at 0.08. The 1.5B decider adds
      nothing on top of an encoder's shortlist (0.22 / 0.19): at this size it
      is not a useful System One decider. **Next: build it** -- an embedding
      index of workspace and reference documents with e5-small on MLX, cached
      by content hash, fused with the section BM25; its own process, so the
      engine's prompt cache is untouched. Results in the experiment's
      `results-embeddings.md`.
      **Built the same day, opt-in:** an embedding sidecar
      (`pwr_embed.py`, its own process, offline -- the hub is put in offline
      mode before anything loads) and a `SectionRanker` hook in
      `pwr_repo::retrieve_with`, fused by reciprocal rank with the section
      BM25; with a ranker, every document is a candidate, so a section that
      shares no word with the request can be reached. Section vectors are
      cached per workspace by content hash and model. `pwr repo rank
      --semantic` measured on the twelve requests: sections-only P@3 0.19 ->
      **0.24**, recall 0.16 -> 0.17, **11% fewer tokens delivered** -- real
      but short of the isolated experiment, because the product's lexical half,
      corpus and slot split differ, and twelve labelled requests are too few to
      tune those without fitting them. `setup-mlx.sh` installs `mlx-embeddings`
      and fetches the encoder once. **Next, the maintainer's hypothesis:** the
      value is for small models and small windows, so wire it into the turn
      and compare a small model at a small window with and without it.
      **First small-model comparison, 2026-09-23** (Qwen3-14B MLX 4-bit,
      window 40,960 bound by the model's trained length; a docs FAQ task over
      ~220K tokens of documentation with an acceptance check that verifies
      each answer against the document it cites; n=1 per arm, no capability
      claim). Both arms **verified**. Lexical: 14 steps, 15 actions, first
      prompt 6,393 tokens (3,027 of retrieved passages), peak 14,893, two
      whole-document reads, 940 s. Semantic: 12 steps, 13 actions, first
      prompt 5,840 (2,446 retrieved), peak 13,155, **no document reads at
      all** -- the passages it was given were enough -- 1,122 s, the extra
      time being generation, not prefill (92 s against 104 s). Direction as
      hypothesised -- fewer tokens, fewer actions, nothing read whole -- but
      one run each on a task both arms pass cannot say more than that; the
      next step is a task the lexical arm fails at this window, and repeats. What section retrieval
      does not fix is the case for the encoder: requests whose answering
      section shares no content word with the question ("why does this project
      exist" -> "Definition and initial user").
- [x] **D.E2E-28 Unattended end to end, from an empty workspace (2026-09-22).**
      One `pwr run` on a fresh `cargo init --lib` outside any repository,
      Qwen3.6-35B-A3B, no steering: an ISO-8601 duration parser with 18 tests,
      **verified** against the workspace's own `cargo test` at step 18, 19 tool
      actions, 5 minutes, two malformed calls recovered from. This is what
      "works end to end" means here -- with a check to answer to. The same
      task inside `experiments/` **declined**, correctly: `cargo` could not
      read the parent workspace's `Cargo.toml` from inside the sandbox, so the
      check could not run, and the model said so rather than claiming success.
      A crate created inside another repository is a real case (the sandbox
      denies the parent), and the refusal should name it before a person has
      to work it out; the model's own diagnosis was right but cost the run.
- [x] **D.E2E-29 A run made the checks pass by editing the library (2026-09-23).**
      Built to answer "does dependency search change an outcome"
      (`experiments/dependency-search-20260923`, three runs, n=1 each, no
      capability claim): an in-house package installed in `node_modules`, an
      acceptance test written first, and the right answer one line long.
      Without the dependency search a run could not find the library by
      searching -- `node_modules` is gitignored -- so it **edited the library**,
      changing one character of its alphabet until the expected string came
      out, and the audit recorded `verified: true` about a workspace whose
      library no longer does what it says. Installed dependencies are now
      read-only unless the run holds `DependencyChange`, which a manifest edit
      already required; reading and searching them stay open
      (`an_installed_dependency_cannot_be_edited_without_the_approval`).
      Two more findings from the same three runs: the model reached for
      `search in_dependencies` **at its fourth action, unprompted** (the open
      question when it shipped), and neither arm that passed used the
      library's behaviour -- one fitted a formula to the test's two cases. **A
      check that names two expected strings is satisfied by fitting two
      expected strings**; a contract for this task has to compare against the
      library's own output over inputs the run cannot enumerate.
- [x] **D.E2E-30 Protection that protected nothing, and a suite read wrongly (2026-09-23).**
      Three faults found while preparing the small-model experiment. (1) The
      scripted run (`pwr run`) built its policy with `protected` empty, so
      `.pwr/protected.json` was honoured in the conversation and never in a
      run: a run could rewrite the test it was measured by. (2) A protection
      file that failed to parse, or used any key but `protected`, protected
      nothing in silence -- the dependency-search runs wrote `{"paths": ...}`
      and their acceptance tests were editable throughout (none was edited,
      which is luck). Now absent means nothing protected, and present-but-
      unreadable stops the work with the expected shape
      (`a_protection_file_that_cannot_be_read_stops_the_work`). (3) From
      `bf18ddb2` to this fix the full suite was read by grepping for
      `FAILED|panicked`, which a compile error does not print: the
      orchestrator's `audit` test target did not compile after
      `in_dependencies` was added, and three commits were reported green.
      The suite is now judged by cargo's exit code (975 passed, 0 failed).
- [x] **D.E2E-32 A greeting answered mid-word (2026-09-23).** "Ciao" to
      Qwen3-14B came back as "Ciao! How can I a". The engine produced the
      whole reply (reproduced: "Ciao! Come posso aiutarti?", finish `stop`);
      the core lost the tail. Qwen3-14B writes its own `<think>` block, so the
      answer shown live began with the blank line after `</think>`, while the
      adapter's reading of the whole reply was trimmed; `Live::finish` looked
      for the shown text as a prefix of the whole, found none, and added
      nothing. The two are now compared without leading whitespace
      (`an_answer_after_an_inline_think_block_arrives_whole`). Any model whose
      template does not pre-open the think block was exposed to it.
- [x] **D.E2E-31 A fixed page, then fifty actions on a test setup nobody asked
      about (2026-09-23).** The maintainer asked, in the app with Goal on and
      Qwen3.6-35B-A3B, why the PWR site showed no text. The model found it
      (every element carried `animate-on-scroll`, `opacity: 0`, and no script
      ever added `.visible`), fixed it, and the build passed. It then ran
      `npm test`, which had been failing since before the request (`ng test`:
      "Cannot determine project or target"), and spent the next ~50 actions
      and three check-ins rebuilding the test setup -- a karma target, then
      vitest, rewriting `package.json` -- while the maintainer asked "it works
      now, why are you still changing things?". The engineer's own `ng serve`
      on port 4200 was **not** the cause: the model's `npm start` failed on the
      busy port once, early, and it moved on. Read from the audit
      (`experiments/web_pwr/.pwr/state.sqlite`), five faults, all fixed:
      (1) **goal mode had no notion of a check already failing** -- it now runs
      the checks once when a goal starts, tells the model which were already
      failing and that they are outside the goal (not to be repaired unless the
      engineer asks), and a completion whose only failures are those ends
      instead of sending the model back (`already_failing_note`, serve.rs);
      (2) **a copied hash with the right head and a borrowed tail** was refused
      five times running although each refusal carried the right hash, until
      the model rewrote the whole file -- `hash_matches` now also accepts a
      claim whose first sixteen characters are the current hash's (64 bits);
      (3) **a busy port** is refused before anything starts, named as probably
      the engineer's server -- before, a readiness check could have taken the
      engineer's server for the model's; (4) **the permission switch read
      backwards**: it showed "Ask" with its knob off, which reads as "asking is
      off", and the maintainer saw prompts they believed disabled (they were in
      Ask mode, and the prompts were `DependencyChange` for `package.json`, as
      Ask intends). It is now one label, **Auto-approve**, knob on = Auto, like
      the Goal switch; (5) **stop did not stop**: the old run, still going,
      hung on `npm install` and stop did nothing for minutes, because the turn
      read the flag only between actions -- stop now interrupts a running
      command, whose whole process group is killed with it
      (`ProcessGroupGuard`); an edit is still never dropped half-written.
      *Open:* a question steered into a goal turn ("why are
      you still changing things?") was not answered before the next edit; the
      goal prompt's "add focused deterministic checks" invited the new spec
      file, and should be weighed against a request that asked for none.
- [x] **C.22a Rizzo Flow as the context decider -- trial, 2026-09-23.** An
      open, local System One implementation (Rizzo AI Academy, Apache-2.0,
      Spark-X2.5-4B on llama.cpp), proposed by the maintainer. Run on the
      M2 Max's Metal backend (untried by its authors), reranking the best
      candidate list measured (section BM25 fused with e5-small, top 20).
      **Every variant lowered precision** -- boolean with the request as
      state 0.17, with every candidate in the state 0.14, abstention off
      0.14, a 4-level score 0.17 -- against 0.28 for the fused order it was
      given, at 5-7 s per request against 5 ms; it abstained on 42% of
      answers with abstention on, and its probabilities were sharply peaked.
      **Not adopted** for context selection, by the criterion fixed
      beforehand; a reference like Jev. Short classifications (task kind,
      "is the evidence enough") would be separate trials. **Protocol fix
      found on the way:** the labels name sections by line, the day's
      documentation pass shifted 16 of 45, and every arm now reads a pinned
      copy of the tree at the labelling revision; the encoder and fusion
      numbers reproduce exactly on it. Details:
      `experiments/jev-context-filter-20260922/results-rizzo-flow.md`.
- [ ] **C.22b Encoder-ranked evidence after compaction -- future experiment,
      proposed 2026-09-25.** Inspired by
      [fast-jev-compaction](https://github.com/tamaratran/fast-jev-compaction):
      decide separately whether an older tool call and its full result still
      help the current task, while preserving any retained bytes verbatim.
      Jev is a hosted reference, not a proposed dependency. Test whether
      PWR's existing offline `multilingual-e5-small` encoder can rank
      provenance-linked evidence windows for a fixed post-compaction token
      budget. The encoder supplies a ranking, never an authorization or a
      calibrated keep/drop verdict. Keep user requests and revisions, recent
      call/result pairs, changed-file state, open failures and the latest check
      verdict under deterministic rules; re-read selected file windows from
      disk and validate their hashes before delivery. A low similarity score
      alone must not erase evidence. Compare three policies at equal delivered
      token budget: current compaction, recency fill, and encoder-ranked
      evidence. Measure verified completion, unchanged-file re-reads after
      compaction, stale evidence delivered, prompt tokens, wall time and peak
      memory. Use multiple compactions and external edits in the tasks; freeze
      the treatment, baselines and acceptance criteria before confirmation.
      The earlier 1.5B and Rizzo Flow failures (C.22/C.22a) rule out assuming a
      Jev-like classifier will improve ranking. **Status: documented only;
      no implementation or campaign until the existing runtime and context
      paths are consolidated.**
- [ ] **C.23 Single-agent adaptive harness -- idea, proposed 2026-09-22 by the
      maintainer.** No planner/coder/reviewer sub-agents: one local model whose
      operating mode (explore, plan, implement, test, debug, review) the
      harness changes by swapping compact skills, instructions, context and
      available tools; skills loaded on demand rather than all in the system
      prompt; a complexity router choosing FAST (inspect-edit-verify),
      STANDARD or DEEP workflows and upgrading mid-task; a working state that
      keeps structured conclusions rather than the reasoning that produced
      them, with older context compressed. The maintainer's name for it:
      "Single-Agent Adaptive Harness". Caution recorded with it: 2026-09-22's
      failures were not context overload (262K window, little of it used) but
      missing sources, stale sources, no view of the rendered page and a
      self-written completion test; and the conversation became the loop on
      purpose (a deliberate move recorded in the roadmap), so modes
      should change context and tools, not reintroduce rigid phases. First
      experiment: one skill pack loaded only for Angular tasks (C.21),
      measured on the website task against today's run.
- [ ] **C.24 A per-model registry learned from the audit, with fallbacks for
      errors of form -- idea, proposed 2026-09-23 by the maintainer.**
      **Status 2026-09-23: first version built and measured.** *The registry*
      is read from the audits rather than written by hand:
      `scripts/model_forms.py ROOT...` walks every workspace's event log and
      prints, per model, each kind of refused call and failed generation with
      what the harness does about it now (repaired, refused because ambiguous,
      or below the harness in the engine); `run.started` now records the
      model's name and family, which the log lacked -- the first pass had to
      rebuild names from inspection artifacts. *What it found* (every audit on
      the maintainer's machine): 21 of the 31 multi-call refusals were
      parallel reads from before read batching (2026-09-07), already fixed;
      since then, one shape -- Qwen3-14B's three `replace_text` on one file --
      and five argument shapes from Qwen3.6, each with one reading. *The
      fallbacks* (`repair_form`, `merged_replacements`): `apply_replace` sent
      with `replace_text`'s fields becomes `replace_text`; `replacement` for
      `replace`; `move_path`'s `path` for `to`; `run_command` without `args`
      runs with none; several `replace_text` on one file against one hash
      become the one atomic `apply_patch` they describe; `list_tree` takes an
      optional `path` and defaults its bound. Only a missing field is filled,
      only from the one field that can mean it. *Measured on the A1 replay*,
      six cases from those audits added (63 in all): without the fallbacks
      all six are refused, with them all six decode, the two negative cases
      (edits to two files; two hashes for one file) are still refused, and
      none of the 55 earlier cases moved. **Open:** the merge applies to
      scripted runs; the conversation runs several edits in sequence and the
      second meets a stale hash -- merge there too, answering every call id;
      and "per model" is still a report, not a configuration: a model whose
      audit shows a shape often could be told about it in its prompt suffix,
      measured before adopting. Record of the idea: Record
      every setting per model in one place, and when a model rejects or
      garbles an input or a tool call, fall back to another form of the same
      operation that this model is known to accept. **What exists:**
      `strategies/models.json` holds per-model profiles (sampling, reasoning,
      template family) and the family adapters in `pwr-compat` repair
      calls per family, but both are hand-written. **What is new:** the audit
      already records, per model, every `action.malformed` and `turn.failed`
      with its kind and what followed, so the table "model X fails in this
      shape -> this other shape works for X" can be derived from evidence --
      e.g. nested JSON arguments -> one call per turn; whole-file
      `apply_replace` -> targeted `replace_text`; a runaway reply -> a lower
      reasoning budget. **Measurable offline, first:** the A1 replay suite
      (794 real replies, 98.2% decoded) is exactly the harness for a fallback
      ladder -- how many of the undecoded replies each rung recovers, in
      seconds and with no model. **The constraint that decides whether it is
      safe:** fallbacks apply to errors of *form* only, never to refusals of
      *policy*. Retrying a malformed call in another format is recovery;
      finding "another command that does the same thing" after a denial is
      exactly what had to be blocked this week -- Nemotron passing command
      lines to `echo` after refusals (D.E2E-26), a run editing a library to
      pass its test (D.E2E-29). The registry must tell the two apart by
      construction (the refusal classes already do: `denied` vs `invalid`),
      and a fallback after a denial is refused and logged. Related: C.4, C.22
      (1) -- the profile can also carry "this model copes with at most N
      tools", and the encoder then chooses which N.
- [x] **C.26 Chat mode: a conversation with no workspace -- decided and built
      2026-09-23 with the maintainer.** Asked whether a "chat only" mode made
      sense after a question turned into fifty actions (D.E2E-31), the
      maintainer chose one **detached from any workspace**, to which only
      files, folders and images can be attached for reading. Built that way:
      the core keeps chat mode's conversations, settings and images in a folder
      of its own (`~/.pwr/chat`, `PWR_CHAT_HOME`), advertised in
      `initialize` as `_meta.pwr.chatHome`; a session opened there gets a
      catalogue of `read_file` and `list_tree` only (a write or a command is
      refused as unavailable, `chat_only_tool_catalog`), a policy with no
      commands and no grants, its own short prompt, no checks, no repository
      passages and no goal mode. Attached folders become read-only reference
      folders -- `list_tree` now lists one given its path (it listed only the
      workspace), and a new chat starts with none of the last one's. In the
      app: "Chat without a workspace" under the workspace, "Open a workspace"
      to go back; the model in use comes along; Goal and Auto-approve are
      hidden. **Measured end to end** (`pwr serve`, Qwen3-14B MLX 4-bit): a
      folder attached with a question -- one `read_file`, the right answer,
      38 s; "add a function to calc.py" -- refused in words, the file
      unchanged, no permission asked; the first prompt was 733 tokens, against
      2,888 for "Ciao" in a workspace. *Open:* `search` inside attached
      folders; the second answer came in English to an Italian request.
- [ ] **C.25 Images, for the models that can see -- idea, proposed 2026-09-23 by
      the maintainer.** **Status 2026-09-23: steps 1 and 2 built; step 3
      open.** *Step 2, built the same day -- the unified engine:* when a
      model's `config.json` declares a `vision_config` and the engine's
      interpreter has `mlx-vlm`, the sidecar loads it **once** through
      `mlx-vlm` and wraps it (`VisionText`) in the shape of an mlx-lm model,
      so the one generation loop -- template rendering, reasoning budget,
      prompt cache and checkpoints, loop detection -- drives text and images
      alike. Qwen's vision models place tokens with three-axis positions and
      an image takes fewer positions than tokens, so the wrapper holds the
      whole prompt's positions and image embeddings and slices them by the
      cache offset: a prefill resumed from a checkpoint sees what one from the
      start would. Without `mlx-vlm` the same model loads through mlx-lm as
      before and an image is refused with a message. **Measured on
      Qwen3.6-35B-A3B MLX 4-bit (M-series, 64 GB, greedy):** text output
      identical token for token to mlx-lm's own load (120 tokens), 1.85 s vs
      2.08 s, peak 20.6 GB vs 19.7 GB (+0.9 GB, the vision tower); a prefill
      split in two gives the same output as one; an image turn (268 tokens)
      0.85 s of vision encoding, 3.6 s for 150 tokens, peak 21.3 GB; a
      follow-up about the same image reused the 268-token prefix and answered
      identically to a fresh prefill; over the sidecar protocol a follow-up
      reused 257 of 294 tokens; the reasoning budget still forces the close.
      *Wiring:* `ChatMessage.images` (stored files, `.pwr/images/<sha256>`);
      ACP `image` blocks accepted (`initialize` says `image: true`) and a
      `resource_link` to a PNG/JPEG/WebP/GIF treated as an image, which is
      how the app attaches one; refused before the turn when the chosen model
      has no encoder; the turn composition carries the images to the composed
      request (it had dropped them: the first end-to-end check answered "I
      cannot see an attached screenshot"; after the fix, `pwr serve`
      answered "Build failed: 3 tests" and "red" for the probe image, 10 s).
      *Not yet:* pasting an image from the clipboard in the app; images kept
      through compaction (a compacted history loses them); GGUF projectors
      (llama.cpp). Spike and drivers:
      `experiments/unified-engine-20260923/`. *Earlier status: step 1
      built, step 2 shown feasible.* *Step 1:* `MlxConfig::has_vision_encoder` reads a model's
      own `config.json` (a `vision_config`, or a `*vl` model type);
      `_pwr/models` returns `vision` beside `installed`; the app marks
      those models "sees images" in the picker and, when an image is attached
      to a model that cannot see, says so above the input instead of letting
      the image vanish. On this host: Qwen3.6-35B-A3B and Qwen3.8-27B.
      *Step 2, spike:* `mlx-vlm` 0.7.2 loads Qwen3.6-35B-A3B MLX 4-bit
      (`qwen3_5_moe`) and described a test image correctly in shape and
      colour -- and invented a background colour for a transparent PNG --
      at 21.4 GB peak. **The design constraint it shows:** the engine loads
      models with `mlx-lm`, which is text-only and is where the template
      rendering, reasoning control and prompt cache live; loading the same
      model a second time with `mlx-vlm` would hold two 20 GB copies. So a
      vision model has to be loaded once, through `mlx-vlm`, with the cache
      and reasoning control carried over -- an engine change of its own,
      measured on text first (no regression) and then on the rendered page
      (step 3). Record of the idea: Use images in the workflow where the model has a
      vision encoder, and say plainly which models do. **Checked on this host
      the same day** (from each model's `config.json`): Qwen3.6-35B-A3B and
      Qwen3.8-27B carry a vision encoder; Qwen3-14B, GLM-4.7-Flash,
      Seed-OSS-36B, Nemotron 3.5 30B-A3B and gpt-oss-20b do not. The engine
      passes text only today (`mlx-lm`); `mlx-vlm` (0.7.2) is what serves
      images and is already installed beside `mlx-embeddings`. Steps, in
      order: (1) **capability from the model, not from a list** -- read it
      from `config.json` at inspection, record it in the model's profile and
      show it in the app's model picker ("sees images" / "text only"); an
      image attached to a text-only model is a clear notice, never a silent
      drop; (2) **an image path in the engine** through `mlx-vlm` for models
      that have one, prompt cache kept for the text that follows; (3) **the
      first use with a measured need -- seeing the rendered page** (D.E2E-12):
      the website's style was written blind. The screenshot is taken by the
      harness, outside the sandbox, as a harness tool: Chrome cannot start
      inside the Seatbelt profile (the site contract's `mobile-layout.mjs`
      found this), and a model must not be given a browser to drive for this.
      Then user-attached images in the conversation. Measure (3) against the
      website task's style issues before adopting. Blocked on nothing but
      work; MASTER_SPEC lists vision workflows as on the intended path, one
      extension at a time.
- [ ] **C.12 Web search and documentation retrieval as a tool.** **Status 2026-09-23:** the offline half is built -- `search` with `in_dependencies`, used unprompted by a run; the web half stays behind R5. Record: An R6
      extension, admitted one at a time with scoped policy and observable
      external effects. *Refined 2026-09-18:* part of it exists -- `fetch_url`,
      under the network-access grant -- and what is missing is search, which
      for a product aimed at people who do not pay for APIs cannot rest on a
      paid service (a local SearXNG is one option). And for code, what a model
      does not know is usually already on disk: **the installed dependencies'
      source and documentation** (`site-packages`, `node_modules`, the cargo
      registry) are the exact version the project uses, offline, free, and not
      third-party pages. So the order is **installed dependencies first, the
      web as fallback**, the first half joining C.13's repository map.
      **The first half is built, 2026-09-23:** `search` takes
      `in_dependencies: true` and searches the packages the project declares
      where they are installed -- `node_modules` by `package.json`, a virtual
      environment's `site-packages`, and `Cargo.lock`'s versions unpacked in
      the cargo registry -- with the workspace search's own bounds and one
      shared file scan. Results are absolute paths that `read_file` can open
      and nothing can write; the sandbox opens one subpath per ecosystem
      rather than three hundred package directories. Measured on this
      repository: 285 declared packages, 17 ms to discover them, 4.6 s for a
      literal search across all of them
      (`installed_dependencies_are_searchable_and_readable_but_never_writable`).
      Not measured yet: whether a run reaches for it unprompted, and whether
      it changes an outcome. The web half stays blocked behind R5. Every
      web result is untrusted content that may carry instructions; it goes
      through the network grant and is treated as data, as the corpus's
      `injection-buried-in-a-repo` task already tests. Plausibly worth more to a local deployment than to a
      frontier one, because a small local model's thin and dated knowledge is
      precisely its weakest point; that makes it a candidate with an unusually
      good prior, not an exception to R6's rule. *Blocked behind R5.*
- [ ] **C.7 R4 — adaptation** against the best fixed policy. *Campaign: blocked.*
- [ ] **C.8 R5 — the product gate.** Twenty held-out tasks from five
      repositories, with interruption, restart, steering and pre-existing user
      changes. *Campaign: blocked, and also needs the app.*
- [ ] **C.9 R6 — one extension at a time.** Vision or browser inspection, one
      MCP integration, scoped workers. *Blocked behind R5.*
- [ ] **C.10 R3 resumed**, split into R3-A5 (mechanism, small window) and
      R3-A6 (completion, host's window). *Campaign: blocked.*
- [x] **C.5a The model is told how the checks run -- done 2026-09-18.**
      `latest_verification.commands` carries the check commands, and the
      system prompt says the checks run by themselves after an edit. Runs had
      spent 5-10 of 26 actions rediscovering the test command.
- [x] **C.16a Tool results carry files as they are -- done 2026-09-18.** A
      tool result held a file as a JSON string inside JSON, so the model saw
      `\\.` for `\.` and every file as one line. JSON calls undid it on decode;
      Qwen3.6's XML calls on the engine did not, and broke every edit near a
      backslash (Part E budget variant, tomli-optional-seconds). `content`,
      `stdout`, `stderr` and search hits now follow a one-line JSON envelope
      verbatim. Every engine result before this fix carries the bug.
- [x] **C.16 Enclosing symbol -- done 2026-09-18.** Search hits and read
      windows carry `within` (`line 234: class T > line 3764: def test_x`),
      read from indentation and definition keywords, no parser. Was: "which
      class or function holds line N", as a
      tool or as a field on read and search results. In the budget variant a
      model spent forty actions reading a test file backwards to learn the
      class of a test it had already found, with the answer on screen. Belongs
      to A2 (navigation) and to step 7's tool catalogue.
- [x] **C.17 Restore a file -- done 2026-09-18.** `restore_file` puts a file
      back as the run first read or changed it; the run keeps those bytes
      itself, because evaluation workspaces have their `.git` removed (so
      the model cannot read the fix from history) and a git-based restore
      would never have worked there. Was: twice in one run set a model broke a file with a
      whole-file replacement and had no way back. A3 (editing) and A5 (recovery).
      *Also found:* `vcs_status`/`vcs_diff` failed in the sandbox through
      macOS's xcrun git shim; the harness now runs the installed git directly.
      In evaluation workspaces they still have no repository to answer from.
- [x] **C.18 Question a whole-file replacement that shrinks the file -- done
      2026-09-18.** Refused, with the line counts, when a file of 40+ lines
      would keep under half of them; a partial read now says its hash is the
      whole file's and which tool edits only part. Was:
      `apply_replace` accepted a 49-line body for an 800-line `results.py`. A
      replacement losing most of a file should be refused with the numbers, or
      at least named, before it lands. A3.

- [ ] **C.19 A hash superseded only by the run's own last edit.** Proposed
      2026-09-19, not decided: gpt-oss re-sends the hash it read before its
      own edit, and 9-14 edits per suite are refused as stale (catalogue).
      Accepting such a hash for `replace_text` and `apply_patch` only --
      whose find text must still match exactly once -- and never for
      `apply_replace` or `write_file`, which would overwrite the run's own
      edit. A safety question for the maintainer before it is built;
      measurable on A3 with gpt-oss.

- [x] **C.20 Root-anchored paths -- decided and done 2026-09-19.** A path
      written as if the workspace were the filesystem root (`/slugify.py`, or
      `/workspace/slugify.py` after the container convention) is read as the
      workspace path it names, only when that exists inside the workspace;
      anything else absolute is still refused, now naming the rule. The
      protected-file guard normalises the same way (`/spec.md`, `./spec.md`
      cannot reach a protected `spec.md`; tested). Why: Nemotron 3.5 wrote
      such paths throughout the catalogue runs and kept them after a refusal
      that named the right path, then spent its budget on
      `os.path.abspath('.')`.

## Block D — Product surface: UX and UI

**This block is empty of code.** The core and the protocol exist; the
application does not, and the terminal console is not a product surface by
decision. By volume this is plausibly more work than every other block together.

- [x] **D.1 S2 — toolkit spike.** **Closed 2026-09-23 -- decided 2026-09-22: Tauri 2 + Angular (signals), `apps/desktop`,** after the Slint candidate proved unsatisfactory in use; its prototype has since been removed. Windows evidence moves to D.17. Historical record of the spike: **Started 2026-09-20:** the Slint candidate
      lives in `crates/pwr-app-slint/` and drives model catalog/selection,
      session launch, task send and cancellation through `serve`; it is isolated
      because its compiler conflicts with the terminal's exact Ratatui dependency.
      **GPUI excluded provisionally:** 0.2.2 upstream supports macOS/Linux,
      not the product's required Windows path. Slint still needs Windows and
      fixed-manual-check evidence after the frontend integration pass. Both build
      the same screen as `serve` clients on macOS and Windows, judged against
      criteria fixed before the spike. The clarified product --
      conversation-first, Windows required -- favours Slint; the spike decides.
      No web UI.
- [x] **D.2 Conversation view.** **Closed 2026-09-23 -- in the Tauri app:** a streamed conversation (reply and reasoning as they are written), one unified assistant turn, attachment cards for files and folders, drag and drop, reference folders. Record: **Attachment increment 2026-09-21:** the
      Slint conversation can queue a workspace file path and sends it as a
      read-only `resource_link` with the next prompt. Drag/drop and repository
      citation presentation remain.
- [x] **D.3 Action feed.** **Closed 2026-09-23 -- in the Tauri app:** each turn's actions grouped under it, live, with reads, edits and commands counted and retries shown; refinements belong to D.17. Record: What the agent is doing, now. This is the product's
      central promise and the hardest thing on this list to get right.
- [ ] **D.4 Permission prompt.** **Status 2026-09-23:** built in the Tauri app (allow once, allow for the session, reject), and two permission modes, Ask and Auto (R.2). Still open: the S2 criterion -- clear to someone who has never used PWR -- has not been tested on anyone. Record: **Spike control implemented 2026-09-20:** the
      Slint client displays an incoming request and forwards allow-once,
      allow-for-run or reject-once; a turn waits rather than accepting silently.
      The S2 criterion is that it be clear *to someone who has never used
      PWR*; nobody has tested that on anyone.
- [x] **D.5 Diff view.** **Closed 2026-09-23 -- in the Tauri app:** a Changes inspector with per-file diffs and +/- counts, the latest file open. Record: **Spike increment 2026-09-20:** the Slint client
      renders the latest core-emitted diff (path, old text and new text), without
      reading or writing workspace files. Still needs file navigation/history and
      a review-quality view. Files and diffs stay inside the core's sandbox and
      audit, and the app never writes them.
- [ ] **D.6 Model choice, download and preparation.** **Status 2026-09-23 (later):** the Tauri app has a Model Manager -- Hub search, per-variant fit for this machine, verified and resumable downloads into the engine's models folder, selectable at once for the running engine ([`models-and-context.md`](models-and-context.md)). Open: a Windows run, switching engine from the app, and preparing a GGUF for a workspace on MLX. Earlier: selection and the computed window are in the Tauri app; the download controls exist only in the Slint lab and still have to move. Record: **Selection increment
      implemented 2026-09-20:** `_pwr/models` reads the active catalog and
      selects only a discovered artifact for its workspace; it also returns the
      computed window. **Status increment 2026-09-20:** it also returns declared
      HuggingFace artifacts and final/`.part` byte state without hashing or
      starting a network request; the Slint lab displays it. Still needs actual
      download/progress notifications, packaged registry resources,
      and resumption of a preparation interrupted by quitting the app (`serve`
      open question 4). **Protocol increment 2026-09-21:** `_pwr/download`
      starts the existing verified/resumable downloader asynchronously with
      `cwd` and no required session, `_pwr/download_progress` emits byte
      progress, and `_pwr/download_cancel` leaves its `.part` for retry.
      The Slint lab exposes a Download / resume control and uses a stable
      client operation id. Status after an app restart is recovered from disk;
      the pinned registry is now embedded in the core as an installation
      fallback, while a workspace-local copy remains authoritative. Richer
      multi-artifact queue management is now sequentially exposed by the Slint
      lab with per-artifact Download / resume and Download all controls. With
      B.8 it also becomes a download manager.
- [x] **D.7 Window setting.** **Closed 2026-09-23 -- in the Tauri app:** smaller/larger window controls with the computed decision and its rationale, and a context meter (used / window / percent) fed by `_pwr/usage`. Record: **Core-backed setting increment 2026-09-21:**
      `_pwr/models` accepts `contextTokens`, applies it through the selected
      backend and returns the granted value plus supported options, binding
      ceiling, memory budget and rationale. The Slint lab exposes
      smaller/larger controls and displays those details. A richer calibration
      history remains.
- [x] **D.8 Sessions.** **Closed 2026-09-23 -- in the Tauri app:** conversations listed per workspace and resumed; the last workspace reopens. Still missing, tracked in D.17: replaying a resumed conversation's past actions in the timeline. Record: **List/resume increment 2026-09-21:** the Slint lab
      lists workspace sessions through `session/list` and resumes one through
      `session/resume`. Branch drift and interrupted-run state are still only
      visible in the core response and need a dedicated view.
- [ ] **D.9 Settings.** **Status 2026-09-23:** the Tauri app has the window and the Ask/Auto permission switch, and shows when commands are not sandboxed; the per-kind ask list is not exposed yet. Record: **Approval control increment 2026-09-21:** the Slint
      lab can replace the workspace's ask-before list with all supported action
      kinds or an empty list through `_pwr/approvals`. Model and window are
      visible; window and sandbox controls remain.
- [ ] **D.10 First run and onboarding.** **Status 2026-09-24:** a first-run screen installs the MLX engine (standalone Python and pinned packages through the bundled `uv`, progress, cancel, retry), then points to the Model Manager; an empty conversation offers the Model Manager too. A guided first task remains. Record: A new user opens PWR with no model
      at all. This path has neither a design nor a line of code, and it is the
      first thing every user will meet.
- [ ] **D.11 Evidence and verification view.** **Status 2026-09-23:** the Tauri app has Evidence and Core log panels beside Changes; a dedicated verification layout and stop classification remain. Record: **Command increment
      2026-09-21:** the Slint lab can request changes, verify, report and
      diagnose for the active session, rendering the core's text in its action
      feed. A dedicated evidence/history layout and explicit stop classification
      remain.
- [ ] **D.12 Error, empty, offline and cancellation states.** **Status 2026-09-23:** the Tauri app shows core state, stop and request errors; empty-catalogue and recovery copy remain. Record: **State increment
      2026-09-21:** the Slint header now distinguishes core connected, backend
      unavailable, cancellation requested and request errors; the catalog and
      action feed retain their detailed messages. Empty-catalog and recovery
      presentation still need dedicated copy and layout.
- [x] **D.13 Token-level streaming?** **Closed 2026-09-22 -- yes:** `TurnStep::Streaming` carries reasoning and reply chunks, sent as ACP `agent_thought_chunk` and live `agent_message_chunk`; the app renders them as they arrive. Record: *`serve` open question 1: worth changing
      `collect_reply` for, or reply-sized chunks until a client shows it
      matters?*
- [ ] **D.14 `reject_always`.** *`serve` open question 2: it needs a session
      denylist PWR does not have. Add it, or keep the three options PWR
      can honour?*
- [ ] **D.15 Read-only "ask" mode against "code" mode.** **Status 2026-09-23:** not to be confused with the Ask/Auto *permission* modes (R.2), which decide what is asked before it runs; this is about a mode that cannot edit at all. Open. Record: *`serve` open
      question 3.*
- [ ] **D.16 The app's language.** Never decided. PWR is for people who
      cannot pay for API access, and the maintainer works in Italian; this is
      not a detail to leave to the end.
- [ ] **D.17 S3 — the app itself**, **Status 2026-09-24:** v0.1.0-alpha release packaging is prepared for macOS on Apple silicon: ad-hoc signed DMG with the core, the engine scripts and `uv` bundled; not notarized (first launch goes through System Settings → Privacy & Security → Open Anyway). The GitHub release is not yet published. Notarization and Windows remain. Record: covering everything the console does, on
      both platforms, with signed installable builds.

- [x] **D.E2E-1 Commands without a shell reported false success (2026-09-22).**
      A Goal-mode session building the Angular site (Qwen3.6-35B-A3B, 169
      actions, workspace = repository root) learned `exec <program>` from one
      `npm exec ng new` that worked and then sent `args: ["exec", "ls", ...]`
      to every program, and `cd site && npm run build` seventy times.
      `/usr/bin/cd` exists on macOS, ran, changed nothing and exited 0, so the
      build never happened and each attempt read as a success. Fixed in
      `pwr-tools`: `run_command` takes an optional workspace-confined `cwd`;
      shell builtins (`cd`, `pushd`, `source`, ...) and lone shell operators
      (`&&`, `||`, `|`, `;` except after `find -exec`, redirections) are refused
      with a message naming `cwd`; an `exec <program>` prefix repeating the
      executable is dropped like the existing leading repeat. Tested in
      `sandbox_and_approvals.rs`.
- [x] **D.E2E-2 No-progress detection never stopped a goal (2026-09-22).**
      `no_progress.detected` fired five times in the same session and changed
      nothing: the tracker was created per turn, so every Goal-mode check-in
      reset novelty and the count. The tracker and the changed-file signature
      now live in `Continuity` and survive check-ins; three consecutive stalled
      windows end the turn with the new `StopReason::NoProgress`, which the
      goal loop does not continue past. A new operator prompt resets only the
      window count (`Continuity::operator_spoke`).
- [x] **D.E2E-3 The app could not change the context window (2026-09-22).**
      `_pwr/models` saved the requested window and then `compute_context`
      overwrote it on the same request (and on every refresh). The choice is
      now a persisted `context_setting`, passed to `window::decide` as the
      `Setting` ceiling, so memory still caps it; the app updates its label
      from the core's reply and steps from the value in force instead of an
      index that fell back to 8,192. The "8K" seen on 2026-09-21 was
      `CHAT_CONTEXT_DEFAULT`, left in place when the model's facts could not be
      read; the computed window for Qwen3.6-35B-A3B on this 64 GB host is
      262,144 (trained length binds).
- [x] **D.E2E-4 Process cleanup on close, verified (2026-09-22).** Closing the
      core's stdin (the app exiting) and `kill -9` on the core both left no
      `pwr serve` and no MLX sidecar behind: each link exits on EOF. A
      generation in flight when the parent dies ends at its next emit
      (broken pipe).
- [x] **D.E2E-6 Tool-call ids repeated across goal turns (2026-09-22).**
      Each turn numbers its calls from one and a goal runs several turns under
      one prompt, so after a check-in `turn1-call8` named a different action
      than before and a client could merge them. The goal loop now offsets each
      turn's ids past the highest already sent; asserted in
      `goal_mode_continues_past_a_checkpoint_and_requires_full_verification`.
- [x] **D.E2E-7 The project website, built by PWR in Goal mode (2026-09-22).**
      Workspace `pwr-website/` (not the repository root, so completion is
      judged by the site's checks rather than the whole Rust suite), sources
      given as attachments (README, `docs/architecture.md`,
      `docs/current-cli.md`, the CV). The acceptance contract was written
      before the session and protected (`.pwr/protected.json`): a TestBed
      spec driving the real router and navigation, and a check on the built
      `dist/` (title, meta description, `prefers-reduced-motion`, responsive
      rules). Qwen3.6-35B-A3B at 262,144 tokens repaired the corrupted pages,
      rewrote all six and added the root navigation: 65 actions, 19 minutes,
      one check-in, goal verified 4/4, contract hashes unchanged, no process
      left afterwards. Re-run independently: build, 13/13 tests, contract
      pass. Content is faithful to the sources (much of it near-verbatim from
      the README); design is clean but plain rather than premium. A mobile
      check (`acceptance/mobile-layout.mjs`, headless Chrome over CDP at 390px)
      was added afterwards for future sessions; the current build passes it.
      Observed and left open: the model bypassed the "would delete 290 lines"
      guard on a whole-file replace with `delete_path` then `write_file`,
      which was a legitimate rewrite here but defeats the guard's intent.
- [x] **D.E2E-8 What damages the website task, measured (2026-09-22).**
      Two runs, same model (Qwen3.6-35B-A3B, 262K), same prompt (the
      maintainer's own), same clean Angular scaffold in `pwr-website/`,
      Goal mode, only the CV attached, no contract in the workspace; judged
      afterwards by the external contract and a claim scan.
      *Baseline:* `read_file ../README.md` refused at the first step; the model
      read the workspace's Angular README instead and wrote the site from the
      CV -- which itself still says PWR is "for Ollama" -- and memory: 28
      Ollama mentions, headline "coding agent for Ollama", an invented
      repository URL (`github.com/VitoSanta/pwr`) and an invented
      `OLLAMA_MODEL` variable; its final message claimed "content sourced
      strictly from the CV and repository". Routing correct (10/11 contract
      tests). *With the parent declared as a reference folder:* correct URL
      and real doc links throughout, MLX and llama.cpp described, Ollama only
      in the CV-derived bio; routing still correct (the 3 contract misses are
      requirements the prompt never states: a textual Home link, the page
      title, `git clone`); one invented mock `doctor --json` output. Neither
      run produced a single `@keyframes` -- the prompt asks for "motion
      discreta" and no decorative gradients, and the model has no way to see
      what it rendered. Causes, in order: (1) no access to the documentation
      it was told to read -- fixed by D.E2E-9; (2) an out-of-date CV used as a
      source about PWR; (3) visual quality written blind: the Angular
      guidance says to inspect the rendered result in a browser, and no tool
      can (Qwen3.6/3.8 have a vision encoder, the sidecar is text-only
      mlx-lm); (4) prompt/intent mismatch on motion; (5) completion judged by
      a test the model wrote itself (it replaced the root spec with a
      `router-outlet` check, and tried to cut it to one line).
- [x] **D.E2E-9 Read-only reference folders (2026-09-22).** `chat-config.json`
      takes `reference_roots` (e.g. `[".."]`); they become the policy's
      `extra_readable`, `read_file` accepts `../` paths inside them (canonical,
      `.pwr`, `.git`, `.env*` closed, never writable), and the system prompt
      names them with their Markdown documents. The refusal for an outside
      path says such folders exist. Test:
      `a_declared_reference_folder_is_readable_and_nothing_more`. *Same day:*
      a folder outside the workspace attached to a prompt (the app's Folder
      button) now declares it as a reference folder instead of pasting a
      truncated snapshot (`an_attached_parent_folder_becomes_a_readable_reference`),
      and the app queues several attachments, so a CV and the project folder
      go together.
- [x] **D.E2E-11 Honest framework guidance (2026-09-22).** The Angular packet
      asked the model to "exercise routes through a browser" and "inspect the
      rendered result" with no tool able to; it now says it cannot see the
      page, must not claim visual verification, and must test the real home
      and navigation rather than `<router-outlet>`.
- [ ] **D.E2E-12 A way to see the rendered page (open).** Style is written
      blind. Qwen3.6-35B-A3B and Qwen3.8-27B carry a vision encoder, but the
      sidecar serves text through mlx-lm. Candidate: a `render_page` tool
      (headless Chrome over CDP, as `experiments/site-e2e-contract/mobile-layout.mjs`
      does) returning layout facts first, and a screenshot once the engine can
      pass images (mlx-vlm). Measure against D.E2E-8 before adopting.
- [x] **D.E2E-13 Rerun of the website task by the maintainer (passed 2026-09-22).**
      After D.E2E-14 to D.E2E-19, run through the desktop app with the CV
      and the project folder attached and Qwen3.6-35B-A3B: in the
      maintainer's words, "an excellent starting product", from a harness
      that that morning could not complete the task at all. Next: the
      frontend (S2 revised to Tauri + Angular).
      Reset `pwr-website/`, `sh experiments/site-e2e-contract/install.sh`,
      start the app there, attach the CV and the PWR folder, Goal mode,
      prompt in `experiments/site-e2e-contract/PROMPT.md` (CV for the bio only;
      the documentation is the authority on PWR; animations required).
- [x] **D.E2E-10 Command output without colour codes (2026-09-22).** Commands
      now run with `NO_COLOR=1`, `FORCE_COLOR=0`, `CI=1`; a failing `ng test`
      had reached the model wrapped in ANSI escapes.
- [x] **D.E2E-14 A path out through the parent and back in (2026-09-22).**
      With `..` declared as a reference folder the model read its own
      attachment as `../pwr-website/.pwr/chat-attachments/...` and was
      refused twice (`.pwr` is closed in reference folders). A path that
      normalises back inside the workspace is now resolved as the workspace
      path it is.
- [ ] **D.E2E-15 Whole documents cost minutes of prefill (measured, treatment built, 2026-09-22).**
      Maintainer's rerun (D.E2E-13), Qwen3.6-35B-A3B on the M2 Max: the prefix
      cache holds across steps and check-ins (242 new tokens in 3.7 s), but new
      tokens at 20-56K context prefill at about 75 tok/s. The model read
      `docs/roadmap.md` whole (~28K tokens): 21K -> 38K context in one step,
      222 s. Nine documents read before the first edit, ~26 actions and ~11
      minutes. A section-level read (headings first, then the part needed) or
      a cached per-document digest is the obvious treatment -- the context
      engine in the maintainer's single-agent proposal -- and should be
      measured on this task.
      **First treatment built, not yet measured (2026-09-22):** a Markdown
      document over 24 KB asked for whole (no `first_line`/`max_lines`) comes
      back as its first 80 lines plus its outline -- every heading outside
      code fences with its line number and the document's size in tokens --
      and the `read_file` description says to read sections by window. Code,
      short documents and windowed reads are unchanged
      (`a_long_document_read_whole_returns_its_outline`). To measure: the
      website task's time to first edit and context at first edit, against
      the D.E2E-13 rerun (~26 actions, ~11 min).
- [x] **D.E2E-16 The conversation shows the reply as it is written (2026-09-22).**
      Maintainer's rerun on Qwen3.8-27B: after a check-in the app showed
      nothing for 30+ minutes (one generation, reasoning that did not
      converge, no file written in 47 minutes) and the timeline showed each
      tool call three times (planned, running, done). The core collected the
      whole reply before saying anything. Now: `collect_reply_with` observes
      each chunk; `TurnStep::Streaming` carries reasoning and text deltas;
      `serve` sends them as ACP `agent_thought_chunk` and live
      `agent_message_chunk` (`_meta.pwr.live`); the app streams them into
      "Thinking" and reply bubbles, settles a streamed answer instead of
      repeating it, updates one card per tool call by id, follows the newest
      message, and shows a heartbeat ("the model is generating ... 4m 10s")
      when nothing visible arrives, as while a file is written into a tool
      call. Reasoning stays on its own channel, apart from action details.
      Open: progress while tool-call arguments are generated needs a field on
      `ModelChunk` (86 literals); a reasoning budget for a generation that
      does not converge.
- [x] **D.E2E-17 A stuck engine was waited on forever, then orphaned (2026-09-22).**
      Correcting the first reading of the 27B rerun: it was not reasoning. A
      `sample` of the sidecar showed its main thread blocked in
      `mlx::core::eval` on a condition variable at ~0% CPU. The turn waited
      30+ minutes because `collect_reply` bounds silence only after the first
      chunk, and after Stop and closing the app the sidecar stayed alive
      (PPID 1), holding the model, because a thread blocked in MLX never
      reads the end of its input. Fixed on both sides: the sidecar reports
      `prefill` progress per step (`self.prefill` and mlx-lm's
      `prompt_progress_callback`), and the MLX provider ends a reply after 300
      s without any engine event, dropping the sidecar (killed on drop) so the
      next request starts a fresh one; the sidecar also exits itself 10 s
      after its input closes, whatever its main thread is doing. Verified on
      Qwen3.6-35B-A3B: 16,510 tokens of prefill reported every ~14 s; test
      `ParentGone`. Open: why MLX hung (Qwen3.8-27B, ~57K context, after a
      check-in) -- not reproduced.
- [x] **D.E2E-18 A layout check that crashed Chrome once per turn (2026-09-22).**
      The maintainer's Send "crashed PWR": macOS showed "Google Chrome quit
      unexpectedly" (parent process `node`). Every chat turn runs the
      workspace's declared checks as a baseline before the model starts, and
      the contract's `mobile-layout.mjs` launched headless Chrome inside
      PWR's sandbox, where it aborted in `dlopen` of its framework under
      `/Applications` (three crash reports: 11:37, 11:51, 12:45). With the app
      readable it gets further and stops creating its profile socket in the
      system temp directory. The check is no longer declared; it is run by
      hand after a session. Kept: `.pwr/checks.json` may declare top-level
      `readable` absolute paths (`pwr_verify::declared_readable`), added
      read-only to the turn's and the verifier's sandbox. Also: the app's
      heartbeat now names what the silence can be (baseline checks, prefill,
      a long generation), and the baseline itself is still unannounced.
      Found on the way, open: an executable given as an absolute path
      containing spaces (`.../Google Chrome.app/...`) is refused as "a command
      line".
- [x] **D.E2E-19 The app crashed on Send (2026-09-22).** Reproduced without
      driving the window: `PWR_APP_SELFTEST=prompt:attachment:...` queues
      the attachments and presses Send after start-up, and a panic hook now
      writes to `~/Library/Logs/PWR/app.log` (a Rust panic leaves no macOS
      crash report). The panic was Slint's "Recursion detected": the
      auto-scroll added with streaming (`changed viewport-height => set
      viewport-y`) closed a binding loop through the conversation layout on
      the first message. Removed; the app survives Send with the CV and the
      project folder attached. Auto-scroll is not implemented.
- [x] **D.E2E-20 A game from scratch stopped after two files (2026-09-22).**
      First maintainer task in the Tauri app: an original platformer in plain
      HTML/CSS/JS, Qwen3.6-35B-A3B, empty workspace. `index.html` and
      `styles.css` were written; then `write_file` for `package.json` sent
      `content` as a JSON *object*, was refused as a schema mismatch three
      times, and the turn ended `Unparseable`. Before that, one step reasoned
      for ~8,000 words ("now I write the files", then more thinking) until the
      16K reply cap cut it with no action, about four minutes lost; no
      reasoning budget was ever sent although the sidecar supports one. Fixed:
      an object or array given for a file's `content`/`replacement` is written
      as its pretty JSON (`file_content_sent_as_an_object_is_written_as_its_json`);
      chat turns default `reasoning_budget` to 6,144 tokens unless a profile
      sets one (the engine closes the think block in Qwen's own words; the
      repetition check covers the known relapse); the stop reason is no
      longer repeated in the final answer; the app states the real outcome
      (protocol, provider, recovery, interrupted) instead of "Finished".
      *Correction, same day:* the maintainer's rerun hit the identical
      refusal -- the conversation never went through the shared decoder. It
      had its own `serde_json::from_value`, so none of
      `action_from_tool_call`'s repairs (lone strings for lists, the program
      in `args`, numbers and booleans for strings, and now structured file
      content) ever applied to a chat turn. `converse::decode` now delegates
      to `action_from_tool_call`, keeping only its own refusal of run-only
      capabilities (`a_conversation_accepts_file_content_sent_as_an_object`).
- [x] **D.E2E-21 The prefix cache missed within a turn (measured, fixed 2026-09-22).**
      Game run, Qwen3.6-35B-A3B, 37-44K context: steps whose prompts added
      only 2-3K tokens spent 103, 103 and 132 s in prefill, against 10 s for a
      step where the cache held -- about five of eleven minutes. The first
      step after a check-in is expected to re-prefill if the template drops
      earlier reasoning at a new user message; the others are not explained.
      Suspects: a rewrite of earlier history between steps (ledger,
      `already_read` rewrites, tool-body trimming) or the chat template
      rendering past assistant turns differently from how they were
      generated. Next: rerun a short task with `PWR_MLX_TRACE` and compare
      each request's rendered prefix with the sidecar's `cached_tokens`.
      **Cause found without a model run.** The sidecar reuses its cache only
      when the previous prompt is an exact prefix of the next (Qwen 3.5/3.6's
      linear-attention layers cannot be trimmed back). Qwen3.6's template
      gives an assistant turn an empty `<think></think>` block only while no
      user message follows it, so every user-role message in a turn -- the
      checks' report, steering, a check-in -- re-rendered all the assistant
      turns before it and the whole prompt was prefilled again: ~40K tokens,
      consistent with the 103-132 s measured. Of the six local models only
      Qwen3.6 does this; its template honours `preserve_thinking`, which the
      sidecar now always passes (others ignore it). Tests:
      `StableHistory` (sidecar, fails without the fix) and
      `each_request_of_a_turn_extends_the_one_before` (the harness does not
      rewrite history within a turn). **Confirmed on a real run the same
      evening** (`web_pwr`, Qwen3.6-35B-A3B, 95 actions, 71 generations,
      11K -> 105K context in 22 minutes): prefill now tracks the tokens each
      step *adds*, about 230 tok/s -- a step adding 1-3K costs 5-11 s and 45
      of 71 steps cost under 5 s, where before a step adding 2-3K at 40K cost
      103-132 s. It holds across check-ins. Prefill was 402 s of the run
      against 910 s of generation; the largest single cost was one step that
      added 14.7K tokens (69 s), which is D.E2E-15's territory, not the
      cache's.
- [x] **D.E2E-22 Generations that produce no action leave no trace (fixed 2026-09-22).**
      Game run: from 14:36 to 14:48 the engine generated twice (CPU ~70%)
      and the audit holds nothing -- no `turn.generated`, no error -- because
      a reply that fails, is truncated at the reply cap or yields no usable
      call is retried without an event. The maintainer could only watch the
      streamed reasoning. Record every generation with its outcome (usable,
      truncated, unparseable, cancelled) and its token counts, so a stall is
      diagnosable from the log. **Done:** every generation the turn cannot use
      now writes `turn.failed` (`unparsed_output`, `runaway_reply`,
      `backend_fault`, `context_limit`, `cancelled`) with what had streamed
      and how long it took, numbered in the same sequence as
      `turn.generated`; `pwr diagnose` names them as `failed_generations`
      with the time they cost.
- [x] **D.E2E-23 What the game run needed from a person (2026-09-22).**
      Qwen3.6-35B-A3B wrote a 14-file platformer and 37 tests, then spent
      ~40 minutes on 3-4 failing collision tests: first moving the player in
      the tests to avoid the bug (legitimate-looking edits whose comments said
      so), then, steered, restoring them and adding sub-steps (34 -> 36/37).
      The last failure needed two numeric details found only by running its
      code with a trace: a ~1e-17 s residual sub-step that reset `onGround`,
      and a bottom-row index `(y + height - 1)` blind to sub-pixel
      penetration. Lessons for the harness: a model cannot see its program
      run beyond pass/fail, so a debugging aid -- running a snippet or a
      single test with printed state -- is worth offering explicitly; and
      edits to the model's own tests after they fail deserve to be surfaced
      to the person, not judged by the core.
- [x] **D.E2E-24 Model profiles never applied outside the checkout (2026-09-22).**
      `strategies/models.json` was read by a path relative to the working
      directory, and the core runs in the workspace, so in every workspace but
      the repository itself no profile loaded: Qwen3.6-35B-A3B ran greedy
      (temperature 0 instead of 0.6, top_k 20, top_p 0.95) with reasoning on
      although its profile turns it off. It explains a day of symptoms: the
      ~8,000-word reasoning, two game replies ended by the repetition guard
      (at 4,992 and 2,976 tokens, invisible in the log, D.E2E-22), a session
      editing a threshold in circles and proposing identical find/replace
      pairs. The registry is now embedded at build time
      (`PACKAGED_MODEL_PROFILES`, like the artifact registry); a workspace's
      own `strategies/models.json` still wins
      (`model_profiles_apply_outside_the_checkout`). Every website and game
      run of 2026-09-22 ran without its profile and should be read as such;
      the reasoning budget (D.E2E-20) was a treatment of this symptom and is
      moot for profiles that turn reasoning off.
- [x] **D.E2E-25 The game finished; what the last hour added (2026-09-22).**
      With the profiles applied (D.E2E-24) Qwen3.6 generated in 1-3 s per step
      with no reasoning and the prefix cache held; told the two numeric fixes,
      it made them and `npm test` passed 37/37 -- after one more harness gap:
      an `apply_patch` hunk that dropped a blank line inside the block was
      refused three times as "does not appear". Edits now match a block whose
      lines are in the file once, blank lines and indentation set aside, and
      edit only that block (`loose_region`); `apply_patch`'s refusal names
      where the text diverges, as `replace_text`'s did
      (`an_edit_matches_its_block_despite_a_dropped_blank_line`).
- [x] **D.E2E-26 Servers and commands that were never run (2026-09-22).**
      Asked how to start the game, Nemotron 3.5 ran `python3 -m http.server`
      through `run_command` twice, each held to the 2-minute timeout: a
      command that serves until stopped (http.server, `npm start`/`run dev`,
      `ng serve`, `vite`, `php -S`, ...) is now refused towards `start_service`
      or towards telling the person the command. In a website run from an
      empty workspace it passed about a hundred command lines to `echo`
      ("echo 'ls -la'"), each a success that did nothing, and some to `sh`
      without `-c`; both are now refused with the way to run them
      (`a_command_passed_to_echo_or_a_bare_shell_is_refused`). Also in the app:
      Goal mode defaults off (a question is not a task), and messages written
      while a turn runs are queued and sent when it ends, or delivered into it
      with "↳ now" through `_pwr/steer`.
- [x] **D.E2E-27 A reference folder the refusal said did not exist (2026-09-22).**
      The Nemotron 3.5 website run (`web_pwr`, 146 actions, ended
      Unparseable) had the project folder attached as a read-only reference and
      never read it: its first move was `ls -la` with the project as `cwd`, and
      the refusal said no reference folder contained that path -- false. It
      then spent about a hundred actions on `echo "<command>"` (all "successes",
      D.E2E-26), believed it had run `ng new`, `npm install -g` and `rm`, wrote
      to `/pwr-website/...` that never existed, and left four files. The
      refusal now names the reference folder and the way in (`read_file` with
      that path; commands, search, list_tree and writes stay in the workspace).
      The chat instructions also say the newest message wins when it changes an
      earlier request -- asked by the maintainer after a run followed its
      context over their latest message. Found while fixing D.E2E-21: composing
      each turn replaced the system message with one *without* the reference
      folders, so from the first turn on the model was no longer told where
      the project's documents were; it keeps them now. Not fixed by the harness: the prompt
      told the model the workspace *contained* PWR, when it was empty.
- [ ] **D.E2E-5 Corrupted code from GLM-4.7-Flash (open, 2026-09-22).** A
      session on the site workspace with `GLM-4.7-Flash-MLX-4bit` at 16K wrote
      `ngOnInit0 {`, `styleUrl: '...css0;`, `</section2>` and dropped `</ul>`
      tags -- present already in the tool-call payload, so not the write path.
      Unknown whether it is the model at 4-bit or GLM detokenisation in the
      sidecar; reproduce with `PWR_MLX_TRACE` before attributing it.

## Block E — Platform and security

- [ ] **E.1 Execution isolation on Windows.** The sandbox is macOS Seatbelt
      only; Linux and Windows have probes, not equivalent confinement.
      **PWR does not ship where it cannot confine what the agent runs**, so
      this blocks the entire Windows half of the product. Nobody is working on
      it, and it is the most underweighted risk in the plan.
- [ ] **E.2 The core on Windows**, independently of the sandbox.
- [ ] **E.3 Signed, installable builds** for macOS and Windows.
- [ ] **E.4 Authorization claims** extended past macOS, with the same
      confinement tests Seatbelt passes.

## Block F — Structural debt

Declared rather than hidden; none of it blocks an alpha.

- [ ] **F.1 `pwr-orchestrator/src/lib.rs`** is about 7,000 lines and
      **`pwr-cli/src/main.rs`** about 10,000. The CLI still holds
      orchestration -- hardware probing, profile resolution, prompt
      construction, the evaluation runner -- that belongs behind the
      orchestrator's boundary.
- [ ] **F.2 No artifact table**, and migrations are two `execute_batch` calls
      rather than a mechanism.
- [ ] **F.3 Remove the calibration subsystem** once B.1 lands and open decision
      1 is settled. It touches sixteen files today and is obsolete by decision,
      but removing it is Part B's refactor, not a cleanup.

---

## The critical path

```
A.1  outside reference (half a day, no dependencies)
 └─ open decision 1 (needle recall)
     └─ B.1 + B.2  loading without a probe, computed window
         └─ A.2  Part E diagnostic        <-- the real gate
             ├─ A.3..A.6  suites A1-A4 ─> B.3 catalogue ─> A.8 ─> C.7..C.10
             └─ D.1 S2 ─> D.2..D.17 S3 ──┬─ E.1 Windows isolation ─> E.3
                                          └─ C.8 R5 product gate
```

**Runs in parallel, waiting on nothing:**

- **E.1**, Windows isolation. Independent of every research question and
  blocking half the product.
- **D.1**, the toolkit spike. It was waiting on R3; it needs the re-measurement
  only to know *what* to show, not to choose a toolkit.
- **A.11, A.12, A.13** and all of Block F.

**Honest proportions.** Block D is empty and is probably the largest single
piece of work in the project. Block E blocks the Windows half and has nobody on
it. Blocks A and B -- the part that has had all the attention so far -- are the
cheapest from here, because most of what they need is removing a constraint
rather than building a system.
