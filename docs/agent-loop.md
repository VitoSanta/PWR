# The agent loop

**Checked against `develop` at `0776ff4f`, 2026-09-30.** How a turn, a goal
and a scripted run proceed, every limit that bounds them, and the defects the
plan fixes. Three loops exist; see [architecture.md](architecture.md#three-execution-semantics)
for why that matters.

## The conversation turn

`converse::take_turn` (`crates/pwr-orchestrator/src/converse.rs:829`), called by
`run_chat_turn` in `crates/pwr-cli/src/main.rs` for the app (`pwr serve`) and
the console (`pwr chat`).

Each step of the loop:

1. **Steering.** Messages the person sent with *Send now* (`_pwr/steer`) are
   appended as user messages and recorded as a new objective revision
   (`conversation::record_steering`). Only the revision number is kept in the
   checkpoint, not its text (plan W4.1).
2. **Room.** If the prompt estimate reaches the compaction threshold (75 % of
   the window by default, 50–90 % per workspace), the history is compacted
   (see [context.md](context.md#compaction)). At most two compactions per
   turn; a third need stops the turn as looping. If there is nothing left to
   fold, the turn stops with *context full*.
3. **Envelope.** The generation's budget is planned: prompt tokens (the
   engine's last count plus an estimate for what was appended), the answer
   allowance (16,384 tokens unless sampling names one), and the reasoning
   budget for the chosen Reasoning Effort (see [models.md](models.md#reasoning-effort)).
4. **Generation** through the provider, streamed to the app: reasoning and
   answer on separate channels.
5. **Parsing.** Tool calls in the family's convention are normalised to
   canonical actions (`pwr-compat`). An unreadable call is answered with what
   was wrong; three in a row end the turn.
6. **Completion holds.** A `complete` is not carried out, and the model is told
   why, when it arrived in the same reply as other calls whose results it has
   not read (`COMPLETION_OVER_UNSEEN_RESULTS`, `converse.rs:2482`), or when the
   turn has written, run and read nothing in a workspace (asked once;
   `COMPLETION_WITH_NOTHING_DONE`, `converse.rs:2476`).
7. **Execution.** Each call is checked against policy and run
   ([tools-and-sandbox.md](tools-and-sandbox.md)). A `write_file` onto an
   existing file becomes a whole-file replacement (`own_overwrite`,
   `converse.rs:2498` — a defect, below). Every call and result is recorded in
   the event log; results go back to the model with their call id.
8. **Detectors** (below) may end the turn with a stop reason.

The turn ends when the model answers in prose, `complete` is carried out, the
person presses Stop, or a detector or budget stops it. It returns a
`TurnReport` (actions, whether it edited, whether it completed or declined,
the stop reason) and the new message list.

**After the turn** — not inside it — `run_chat_turn` runs the repository's
checks if the turn edited anything and appends the verdict to the answer and
to the history (`main.rs:4851-4927`; see [verification.md](verification.md#after-a-conversation-turn)).

### Limits of a turn

| Limit | Value | Where | What happens |
|---|---|---|---|
| Actions before checking in | 26 | `ACTIONS_BEFORE_CHECKING_IN`, `converse.rs:99` | The turn stops and says so; the next message continues |
| Compactions per turn | 2 | `COMPACTIONS_PER_TURN`, `converse.rs:76` | Stop as looping |
| Consecutive empty replies | 3 | `EMPTY_TURNS_BEFORE_GIVING_UP`, `converse.rs:60` | Stop as silent |
| Consecutive unreadable calls | 3 | `UNPARSEABLE_CALLS_BEFORE_GIVING_UP`, `converse.rs:66` | Stop as unparseable |
| Consecutive backend faults | 3 | `BACKEND_FAULTS_BEFORE_GIVING_UP`, `converse.rs:112` | Stop as backend failing; edits so far are kept in the history |
| Reasoning finalization retries | 1 | `REASONING_FINALIZATION_RETRIES`, `converse.rs:118` | Stop as reasoning unfinished |
| Unstructured reply guard | 3,000 / 12,000 chars | `AGENT_UNSTRUCTURED_REPLY_GUARD`, `converse.rs:130` | A long prose reply after long thinking is retried once at 8,192 max tokens |
| Same refused action | 3 | `REPEATED_REFUSAL_LIMIT`, `repetition.rs:21` | The model is told to stop proposing it |
| Echoed results | 3 in 10 | `ECHO_LIMIT`, `ECHO_WINDOW`, `repetition.rs:106-107` | Flagged to the model |
| Same failed command | 2 | `REPEATED_FAILURE_LIMIT`, `repetition.rs:162` | Not run a third time |
| Failed runs in a row after edits | 5 | `FAILED_RUN_LIMIT`, `repetition.rs:193` | The model is told to hand over what is ready |
| No-progress windows | 3 of 6 actions | `NO_PROGRESS_LIMIT`, `NO_PROGRESS_WINDOW`, `stall.rs:26-30` | Stop as no progress |
| Reasoning kept per step | 16,000 chars | `REASONING_KEPT_CHARS`, `converse.rs:2515` | Older reasoning is cut from the start |

Each limit has its own counter; there is no shared recovery budget (plan W2.6).

### Stop reasons

`StopReason` (`converse.rs:154`): `Interrupted`, `ContextFull`, `Looping`,
`Silent`, `ToolCallInReasoning`, `Unparseable`, `BudgetSpent`,
`BackendFailing`, `NoProgress`, `ReasoningUnfinished`. How they reach the app
is in [pwr-serve.md](pwr-serve.md#stop-reasons).

## Goal mode

A loop around the turn in `serve.rs` (`crates/pwr-cli/src/serve.rs:1966-2236`),
used when the app sends a prompt with Goal on.

1. Before the first turn, the full verification runs once; checks already
   failing are named in the request so the goal neither repairs them nor is
   held open by them.
2. After each turn:
   - a declined or stopped turn (other than *budget spent*) ends the goal;
   - a turn with no actions and no completion counts as idle; three idle
     rounds in a row pause the goal as *stalled*;
   - on `complete`, the full verification runs:
     - checks pass, the goal edited something, and no review has run yet →
       a **review round**: a second reading of the request against the code
       by the same model (about a minute, reasoning bounded to 4,000 tokens),
       whose findings are sent back as guidance;
     - checks pass (after the review, or with nothing edited) → the goal ends,
       *verified* only if a declared acceptance check exists and
       `.pwr/checks.json` is unchanged since the session began
       ([verification.md](verification.md#goal-acceptance));
     - only checks that were already failing fail, and none of them is an
       acceptance check → ends, naming them;
     - technical checks pass but no acceptance check is declared → ends,
       *not verified*;
     - otherwise the model is sent back with the evidence; the same failing
       set three times → *blocked*.
   - without `complete`, the goal continues with a nudge, unless the action
     total reached `GOAL_MAX_ACTIONS` (208) → paused as *budget*.
3. A checkpoint note is shown every 10 actions.

| Limit | Value | Where |
|---|---|---|
| Actions | 208 | `GOAL_MAX_ACTIONS`, `serve.rs:450` |
| Idle rounds | 3 | `GOAL_IDLE_LIMIT`, `serve.rs:455` |
| Same failing set on completion | 3 | `GOAL_SAME_FAILURE_LIMIT`, `serve.rs:459` |
| Review rounds | 1 | `review_done` |
| Checkpoint note | every 10 actions | `GOAL_NOTICE_EVERY`, `serve.rs:462` |
| Wall-clock | none | — |

## The scripted run

`pwr run "<task>"` and every `pwr eval run` trial use
`run_action_loop_with_prompt_budget_and_context_tiers`
(`crates/pwr-orchestrator/src/lib.rs:2936`). Unattended: grants only what
`--approve` names. It differs from the conversation in ways a campaign
measures and the app does not ship:

- a hard action budget (`DEFAULT_MAX_ACTIONS` 26, `lib.rs:2088`, or
  `--max-actions`), with two provider turns per action (`TURNS_PER_ACTION`);
- a baseline of the checks before editing, checks on completion, and a
  recovery cycle (reproduce, classify, diagnose, retry within budget; stop on
  environment, policy or non-determinism);
- completion holds of its own: unseen results, and *never ran* — a program
  that was only syntax-checked must be run once (`completion_held`,
  `lib.rs:5857`), a hold the conversation does not have;
- `record_progress` (a ledger the loop carries) and `propose_verifier` (a
  check the person adopts) in its catalogue;
- ledger compaction, and optional context policies (`--context-policy
  current | recency-fill | evidence-state`);
- context-tier retry: a prompt the backend refuses is retried at a calibrated
  smaller window;
- `--plan` decomposition, `--provision` toolchain installs, `--session`
  continuation;
- completion with no usable verifier ends `verified: false,
  verifiable: false` (`lib.rs:3783-3815`).

The B0 (conventional loop) and B2 (fixed staged workflow) controls live in
`crates/pwr-orchestrator/src/baseline.rs` and share the tools, policy and log.

## Known defects

| Defect | Evidence | Plan |
|---|---|---|
| The goal's action budget is checked only when the model did not complete; refused completions with alternating failures run without limit; no time limit | `serve.rs:2028`, `2182` | W1.4 |
| Goal failures are compared by check name: progress inside one suite looks stuck; alternating suites look like progress | `serve.rs:2139`; `main.rs:3756-3773` | W1.5 |
| `write_file` onto any existing file uses the hash at execution time, so a stale rewrite overwrites a newer edit | `converse.rs:2498` | W1.1 |
| Post-turn verification happens after the turn ended; the post-turn note is `✓` whatever it says | `main.rs:4851-4927` | W2.2, W2.3 |
| Three loops with different holds, compaction, recovery and catalogues; fixes land in one | this page | W2.4 |
| A dozen independent limits and no shared recovery budget | table above | W2.6 |
| The objective's text is not kept outside the compressible history | `conversation.rs:52`, `compaction.rs:48-54` | W4.1 |
