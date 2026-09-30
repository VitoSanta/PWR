# R3 design — H2, evidence that survives compaction

**Status: DESIGN, 2026-09-15.** Not preregistered, not implemented, and not yet
selected: R2's choice rule is applied to the rerun in
`experiments/r2-rerun-20260915-8648aed`, and this design is used only if
`unfinished` with re-read churn is still the largest testable B1 class there.
Numbers below come from the confounded full pilot and the first part of the
rerun; the preregistration must restate them from the rerun.

## The loss this targets

[H2](local-agent-research.md): *evidence-linked working state improves long
tasks; conversation compaction loses requirements and stale evidence is reused.*
On this corpus the loss shows up as cost rather than as forgotten requirements.

- **Re-reads.** In the 26 B1 primary trials of the full pilot that exhausted the
  action budget, 288 of 711 actions (41%) re-read a file that had not changed,
  230 of them after the run's first compaction. In resolved trials the share
  was 23%. The rerun's GLM B1 part shows the same shape: `pyparsing/core.py`
  re-read 12 times in one trial, `idna/core.py` 8 times.
- **Right work, no completion.** In the rerun's GLM B1 part, 9 of 30 trials
  had the hidden verifier passing and no accepted completion, and all 9 ran
  out of actions.
- **Compaction discards more than it has to.** At the 16,384-token window the
  history budget is 8,192 tokens (`HISTORY_BUDGET_SHARE` 0.5). Compaction in
  those trials took the history from about 8,200–9,900 estimated tokens down
  to about 600–3,500 — typically to a fifth of the budget. What survives is
  the task, the session ledger (paths and hashes read, edits, commands,
  denials) and the last exchange cut to half the budget. What a file *said* is
  gone, so the next turn reads it again, and a re-read of an unchanged file is
  told "you have already been shown this file", which after a compaction is
  true of the run and false of the prompt.

## Hypothesis and gates

Frozen from the research contract, restated for this treatment:

- **Primary:** ≥10 pp verified-completion uplift over the control on the
  long-horizon set, with a positive paired interval (task-cluster, per
  `evaluation.md`), noninferior on clean short tasks.
- **Mechanism check:** ≥50% fewer re-reads of unchanged files after compaction,
  and **zero** stale evidence delivered (an excerpt whose hash is not the file's
  hash on disk when the prompt is sent).
- **Cost, reported not gated:** all-attempt prompt tokens, generated tokens and
  wall time. The treatment keeps more tokens per turn, so it can buy completion
  with prefill; that trade has to be visible.

## Arms

All arms are B1 with the same binary, tools, catalogue, action and time budgets,
window, prompts and adapter. They differ only in what `compact_history` keeps.

| Arm | Compaction keeps | Why it is here |
|---|---|---|
| **C0** — current | task, session ledger, last exchange bounded to half the history budget | The control the pilot measured. |
| **C1** — recency fill | C0, plus the most recent whole exchanges, newest first, until the same token target as T | Separates "keep more context" from "keep the right context". If C1 matches T, the benefit is budget use, not evidence structure, and the simpler policy wins. |
| **T** — evidence state | C0, plus the evidence section below, up to the same token target | The treatment. |

Token target for C1 and T: 60% of the history budget after compaction (4,915 of
8,192 at the pilot window), leaving room for the next exchange before the next
compaction. The share is a design choice to fix before preregistration, not a
measurement; a pilot of C1/T at 40/60/80% on development tasks may set it.

## The evidence section (T)

Composed deterministically from the event log and the workspace at compaction
time — never from the model's own summary — as one `tool` message:

1. **Objective.** The task as given, plus any revision delivered at an action
   boundary, newest last and labelled as revisions.
2. **Checks.** The current status of every required check, with the failing
   diagnostics' file:line locations. From the latest verification event.
3. **Changes made.** Per edited file: current hash and a unified diff against
   the baseline, capped per file.
4. **Evidence windows.** For each file the run read, the line windows it
   actually used, rendered from the file **as it is on disk now**:
   - lines around every edit target (`replace_text` find text, patch hunks);
   - lines around every search hit the run then read or edited near;
   - lines named by a failing diagnostic;
   - the last explicit `first_line`/`max_lines` window read, if nothing above
     applies.
   Each window is headed `path:start-end (artifact_hash …)`. Windows are
   merged when they overlap and ranked: edited files, then files named by a
   failing check, then most recently read. The section is filled in rank order
   up to the token target and says what it left out.
5. **Staleness.** A window is always re-rendered from disk, so its bytes are
   never stale. If a file's hash differs from the one the run last saw, the
   window is headed `changed since you read it` so the model does not reason
   from a remembered version.

The `already_read` note is **unchanged in every arm**: rewording it would be a
second factor. Whether it should say "the content is in your evidence section"
is a follow-up with its own factorial, not part of this test.

## Tasks and injections

- **Development** (tuning the token target, fixture shape): the R2 pilot's own
  tasks where B1 ran out of actions. Development results are not evidence.
- **Confirmation:** a new, repository-disjoint long-horizon set — tasks that at
  a 16,384 window force at least two compactions, in real repositories not in
  R2's six. Sized from the rerun (below).
- **Injected conditions**, per the contract, on a declared subset: a requirement
  revision delivered at action N (the steering path the conversation already
  has; the evaluator needs a scripted injection), and an external edit to a file
  the run has read, at action N (the stale-evidence case). Neither exists in
  `pwr eval run` today.

## Sample size and stopping

Recomputed from the rerun's paired discordance before preregistration. From the
pilot's provisional figure (McNemar, α 0.05, power 0.8, Δ 10 pp): about 155–233
task-seed pairs per deployment per comparison, treated as a floor because trials
within a task correlate. Three arms double the comparisons (T vs C0, T vs C1).
If the host budget cannot carry that, the cheaper design drops C1 and accepts
that a positive result cannot tell evidence structure from retained quantity.
C0 is never dropped: R3 requires a contemporaneous control, and the pilot's C0
trials ran on another binary. Fixed n, no interim looks.

At the pilot's speed (~5.5 min per GLM B1 trial) 180 pairs across three arms is
about 50 hours per deployment. That is the real constraint, and it argues for
one deployment at confirmation with the second as a transfer scope, as R4 does.

## Implementation plan (offline, before any campaign)

1. `RunTuning::context_policy: ContextPolicy { Current, RecencyFill { share },
   EvidenceState { share } }`, default `Current`, so nothing changes unless
   asked. B0/B2 ignore it.
2. `pwr eval run --context-policy current|recency-fill|evidence-state
   [--context-share 0.6]`; recorded in the manifest and report conditions and
   refused by strict pairing unless declared as the treatment.
3. `compact_history` branches on the policy; the evidence section is a pure
   function of `(events, workspace, budget)` so it can be tested without a
   model.
4. Fixtures (fake provider, local workspaces): windows re-rendered after an
   external edit are never stale; the section respects the target and states
   what it omitted; edited files outrank read ones; C1 and T land within a few
   percent of the same token count; C0 output is byte-identical to today's.
5. Evaluator: scripted injection of a requirement revision and of an external
   file edit at action N; outcome fields for re-reads after compaction and for
   stale evidence delivered, so the mechanism check needs no ad-hoc script.
6. Development runs on the R2 unfinished tasks to fix the share and catch
   defects; then freeze a binary and preregister.

## Implementation status — 2026-09-15

Built, offline, with no campaign run on it:

- `pwr_orchestrator::evidence`: `ContextPolicy` (`current`, `recency-fill`,
  `evidence-state`, share in percent), `evidence_section` as a pure function of
  events and workspace, `stale_file_contents` for kept tool results, and the
  `ActionBoundary` hook. `RunTuning::context_policy` defaults to `Current` and
  `RunTuning::boundary` to `None`; the product run sets both explicitly.
- `compact_history` fills C1 and T to the same target and records the policy,
  what it added and `stale_file_contents_kept` on every `context.compacted`
  event, C0 included.
- `pwr eval run --context-policy … --context-share 60`, B1 only, recorded
  in the manifest and report, checked on resume, and refused by strict pairing
  unless `context_policy` is declared.
- Tasks may declare `injections`: `revision` (delivered as a message from the
  person who set the task, recorded as `task.revision`) and `external_edit`
  (applied to the workspace, recorded as `injection.external_edit`), each after
  a given number of actions. The field is omitted when empty, so no existing
  corpus revision moves.
- `TaskOutcome::mechanism`: re-reads of unchanged files, those after the first
  compaction, compactions, evidence windows, files disclosed as changed,
  stale file contents kept, revisions delivered and external edits applied.

Deviations from the design above, decided while building:

- **Changes made** are shown as evidence windows around each edit rather than
  as a diff against the baseline: the loop keeps no copy of the baseline, and
  a copy would be a second source of truth for the workspace.
- **Revisions** open the evidence section and outrank every window. Today's
  compaction drops them once the exchange that carried them is compacted; that
  loss is part of what the comparison measures, and a fixture pins it.
- **Objective** is not repeated in the section: every policy already keeps the
  original task message.
- The conversation loop's own compaction is untouched; R3 runs on the
  scripted loop, and R1's shared runtime is where the two would meet.

Fixtures: `evidence.rs` unit tests (windows from disk, changed files labelled,
ranking and budget, searches), the three-policy compaction test in `lib.rs`,
`tests/evidence_state.rs` end to end through B1 (the revision survives only
under T, no stale contents kept), `pwr-eval/tests/mechanism.rs`, strict
pairing on `context_policy`, and the CLI's policy parsing and injection tests.

Injected variants, added the same day: `corpus/longhorizon-v1.json` now holds
the nine base tasks unchanged and eight variants with their own ids, all
passing `check-corpus`. Five `-revised` variants withhold one rule the hidden
check asserts from the statement and deliver it as a revision after 4 actions;
the upstream fix satisfies the rule and each task's wrong implementation is the
one that does not, so a run that loses the revision cannot pass. Three
`-edited` variants have a colleague insert a comment beside the change after 8
actions, which moves the file's hash and nothing else (visible and hidden
checks were rerun with the comment in place). The variants exist only for B1
under the three policies; B0 and B2 deliver no injections.

Not built yet: the development run that shows whether these tasks force two
compactions and whether 4 and 8 actions land before and after them.

## Decision rules

- **Retain** T as the default when the primary gate and the stale-evidence
  check pass and T beats C1 on completion; otherwise the claim is about budget
  use and C1, the simpler policy, is what is retained if it passes.
- **Revise** when the mechanism check passes (re-reads fall) but completion does
  not move: the loss is elsewhere, and the report says where the actions went.
- **Remove** when neither moves. The negative result is recorded and C0 stays.

## Decisions, 2026-09-15

1. **Three arms**: C0, C1 and T.
2. **Token target 60%** of the history budget, fixed; no development sweep.
3. **One deployment** at confirmation, chosen after the R2 rerun; the other is a
   transfer scope, not pooled.
4. **The long-horizon corpus starts now**, in parallel with the rerun, as
   `corpus/longhorizon-v1.json`, from repositories disjoint from R2's six.

## Corpus, first batch — 2026-09-15

`corpus/longhorizon-v1.json`: nine tasks from upstream fixes committed
2026-06-27 to 2026-09-15 in pycparser (1), bottle (2), pyasn1 (2) and sqlglot
(4) — five bug fixes, one feature, and three multi-file changes of three to five
files. Every task passes `pwr check-corpus`: visible check green at the
parent, hidden check red there and green at the fix, the fix within its allowed
files, and one deliberately wrong implementation per task that passes the
visible check and fails the hidden one
(`experiments/r3-h2-preparation/check-corpus-longhorizon-v1.json`). Hidden
checks are the fix commit's own test modules run standalone, with two recorded
edits: one sqlglot task adds the four `identity.sql` cases its commit added, and
one pyasn1 task drops an assertion that names a private attribute the statement
cannot. The builder is kept beside the check result.

Selection rules, fixed before any trial: stdlib `unittest` only (the host has no
pytest), no compiled or network dependencies, and the files the change belongs
in total more than about 60 KB, twice the history budget at a 16,384 window.
Python-Markdown was cloned and excluded by that last rule (its fixes touch 8–27
KB files). A sqlglot optimizer task was excluded because `tests.test_optimizer`
needs a dependency the host lacks, and a second bottle ETag task because it
duplicated the conditional-request code of another. `max_actions` is 40, the
budget the existing long tasks use; that it is not the pilot's 26 is a design
choice to restate at preregistration.

Nine is a first batch, not the confirmation set: the provisional sample size
needs about sixty tasks at three seeds. Whether these nine actually force two
compactions is unmeasured until a development run.
