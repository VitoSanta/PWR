# Executor parity: what the scripted loop does that the app's turn does not

**Checked against `develop` at `66582cc6`, 2026-09-30.** The first step of plan
[W2.4](implementation-plan.md#w24-converge-the-scripted-runner-onto-the-executor):
an inventory of every behaviour of the scripted loop (`pwr run`, `pwr eval run
--arm b1`) and of the conversation turn the app ships, with where each lives
and what to do with it. **Every disposition below is PROPOSED.** Each changes
what a campaign measures, so each needs a dated entry in
[decisions.md](../decisions.md) before it is carried out (plan W2.4, step 2).

Sources: the scripted loop is `run_action_loop_with_prompt_budget_and_context_tiers`
(`crates/pwr-orchestrator/src/lib.rs:~2936`); the conversation turn is
`converse::take_turn` (`converse.rs`) sequenced by the session executor
(`executor.rs`). Counts of where a name occurs are a guide, not proof; each row
was read at the cited place.

## The inventory

| # | Behaviour | Scripted loop | Conversation / executor | Proposed disposition |
|---|---|---|---|---|
| 1 | Action budget | hard cap, `DEFAULT_MAX_ACTIONS` 26 (`lib.rs:2088`) or `--max-actions`, two provider turns per action | soft check-in at 26 per turn (`converse.rs`); goals bound by `GoalLimits` (actions, refusals, verifications, reviews, wall clock) | **Move.** The executor's `GoalLimits` becomes the only budget; `--max-actions` maps to `GoalLimits::actions` |
| 2 | When checks run | baseline before editing, checks on `complete`, inside the loop | after each editing turn (`close_turn`); full verification on `complete` in a goal | **Move.** The goal policy's sequence is the one; the scripted loop calls it |
| 3 | Recovery cycle: reproduce a failing check, classify (`FailureClass`), retry within edit/verify and context-tier budgets, stop on environment/policy/non-determinism | scripted only (`classify_with_reproduction`, `RecoveryDecision`) | not present; the model is sent back with the evidence and the shared recovery detectors apply | **Decide on evidence.** Keep in the scripted control until W8.4 measures it; if it earns its cost, move it into the goal policy (plan W3.3 already fingerprints failures for both) |
| 4 | Completion hold: results not yet read | both | both | Same |
| 5 | Completion hold: nothing written, run or read this turn (asked once) | not present | conversation only (`COMPLETION_WITH_NOTHING_DONE`) | **Moved (2026-09-30):** the scripted loop now holds it too (`acted`, `completion.held` with reason `nothing_done`) |
| 6 | Completion hold: built a program and only syntax-checked it — asked once to run it | scripted only (`completion_held`, `lib.rs`) | not present | **Moved (2026-09-30):** the conversation now holds it (`converse.rs`, same predicates as the scripted loop, no checks passed because a conversation checks after the turn) |
| 7 | Completion with no usable verifier | ends `verified: false, verifiable: false` | goal ends *not verified* (no acceptance check declared); conversation says verification unavailable | **Same meaning already**; both are `TurnOutcome` `checks: unavailable`, `acceptance: not_declared` once W2.1 migrates the scripted result |
| 8 | Catalogue | full: adds `record_progress`, `propose_verifier`; no `remember`, `recall_project`, `wiki_query`, `look_at` | removes `record_progress`, `propose_verifier`; adds the four | **Keep two catalogues by purpose, one executor.** `propose_verifier` needs a person; `record_progress` has no measured value (drop it unless W8 shows otherwise) |
| 9 | Plan decomposition (`--plan`) | optional, off by default | none | **Remove from the executor;** keep only as part of the B2 control |
| 10 | Task ledger and `--session` continuation | scripted (`task_ledger`, `session_ledger`) | conversation has checkpoints, snapshots and resume | **Keep scripted-only for now;** resolved with the state map (plan W6.2) |
| 11 | Compaction | ledger compaction; optional context policies `current`, `recency-fill`, `evidence-state` (`--context-policy`) | one mechanical compaction (`compaction.rs`), the objective kept whole (W4.1) | **One compaction (the conversation's).** The policies stay experiments until W4.7 compares them on the executor |
| 12 | Context-tier retry (retry a refused prompt at a calibrated smaller window) | scripted | conversation takes tiers too (`take_turn`'s `context_tiers`) | Same; confirm the scripted tiers come from the same filter |
| 13 | Repetition and stall detectors | `RefusalStreak`, `NO_PROGRESS_*` | refusals, echoes, repeated failure, failed-run handover, no-progress | **Same detectors, different sets.** Unified by the shared recovery budget (plan W2.6) |
| 14 | Reasoning envelope | opt-in through `eval run --reasoning-effort` (2026-09-29) | always (`plan_reasoning`) | **Move:** always on; a campaign that wants the old behaviour declares it as the treatment |
| 15 | Overwriting an existing file | `write_file` refuses an existing file and names the hash | rewrite of the version the conversation read; refused if unread or changed (W1.1) | **Keep the scripted refusal** as the stricter form; it is a control detail, not a product path |
| 16 | Toolchain provisioning (`--provision`) | scripted flag: grants any program and the network together | conversation gets the same through the `toolchain_install` and network grants the person answers | **Keep for scripted unattended runs;** not part of the executor |
| 17 | Steering, Stop, queued messages | none (unattended) | conversation | **Executor policy `Conversation`/`Goal` only;** a scripted trial has no person |
| 18 | Person's memory, wiki, recall | none, by design | conversation | Same: offered to conversations only |
| 19 | Approvals | grants named by `--approve`; nothing asked | asked through the host | **One `ApprovalPrompt`;** a trial's host answers from its declared grants |

## What W2.4 would build

An `EvalHost` implementing `SessionHost` for a trial: its turn runs
`take_turn` with the task composed as the conversation composes it; its
`verify` runs the corpus's verifier as the task's acceptance; its answers to
permission questions come from the trial's declared grants. `eval run` then
calls `executor::execute` with the goal policy, and `--mode product-path` is
the only mode. The scripted loop stays in `baseline.rs` as the B0 and B2
controls (plan W8.3), sharing tools, policy and outcome.

Measurement consequence: a campaign run on the executor is a different arm.
Under `compare --strict` it pairs with an older campaign only if `harness_rev`
and `mode` are declared as the treatment, which is the intended effect: old
scripted results stay valid as what they are (R2, small-apps) and are not
mixed with executor results.

## Decisions needed before the moves

1. Rows 3, 9, 10: move, keep as control, or remove?
2. Rows 5 and 6: confirm both holds move into the executor for every path.
3. Row 14: reasoning effort always on in campaigns?
4. Row 8: drop `record_progress` from the scripted catalogue?
5. Whether the scripted CLI (`pwr run`) is kept as a user command at all once
   `eval run` measures the executor, or reduced to a thin call of it.
