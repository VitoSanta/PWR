# The agent loop

**Checked against `develop` at `bff93062`, 2026-10-01** (the baseline was `0776ff4f`; every behaviour changed since is in this page). How a turn, a goal
and a scripted run proceed, every limit that bounds them, and the defects the
plan fixes. Three loops exist; see [architecture.md](architecture.md#three-execution-semantics)
for why that matters.

## The conversation turn

`converse::take_turn` (`crates/pwr-orchestrator/src/converse.rs`), called by
`run_chat_turn` in `crates/pwr-cli/src/main.rs` (the host's turn) for the app (`pwr serve`) and
the console (`pwr chat`).

Each step of the loop:

1. **Steering.** Messages the person sent with *Send now* (`_pwr/steer`) are
   appended as user messages and recorded as a new objective revision
   (`conversation::record_steering`). Only the revision number is kept in the
   checkpoint, not its text (plan W4.1).
2. **Room.** If the prompt estimate reaches the compaction threshold, the
   history is compacted (see [context.md](context.md#compaction)). The
   threshold is 75 % of the window (50–90 % per workspace) **under a ceiling of
   32,768 tokens** unless the person set a window (`context_tokens`) or a
   threshold (`compact_at_percent`) — a hypothesis, see
   [D-2026-09-30-7](decisions.md). At most two compactions per turn; a third
   need stops the turn as looping. If there is nothing left to fold, the turn
   stops with *context full*. Separately, after **two replies in a row that fell
   apart** (a loop, or the engine's repetition stop) **and the prompt is over
   20,000 tokens**, the history is compacted to an 8,192-token room and the work
   goes on from there (twice per turn at most; see *Replies that fall apart*
   below).
3. **Envelope.** The generation's budget is planned: prompt tokens (the
   engine's last count plus an estimate for what was appended), the answer
   allowance (16,384 tokens unless sampling names one), and the reasoning
   budget for the chosen Reasoning Effort (see [models.md](models.md#reasoning-effort)).
4. **Generation** through the provider, streamed to the app: reasoning and
   answer on separate channels.
5. **Parsing.** Tool calls in the family's convention are normalised to
   canonical actions (`pwr-compat`; [models.md](models.md#families-and-adapters)
   lists the forms read). An unreadable call is answered with what was wrong;
   three in a row end the turn. A call written whole but without its closing
   tag is read; one cut off inside is not, and is retried smaller.
6. **Completion holds.** A `complete` is not carried out, and the model is told
   why, when it arrived in the same reply as other calls whose results it has
   not read (`COMPLETION_OVER_UNSEEN_RESULTS`), when the turn has written, run
   and read nothing in a workspace (asked once; `COMPLETION_WITH_NOTHING_DONE`),
   or when the turn created a program and ran nothing that runs it (asked once;
   the *never ran* hold of `completion_held`). The scripted loop holds the same
   three (plan W2.4, rows 5 and 6; decision D-2026-09-30-6).
7. **Execution.** Each call is checked against policy and run
   ([tools-and-sandbox.md](tools-and-sandbox.md)). A `write_file` onto an
   existing file becomes a whole-file replacement of the version the conversation
   last saw (`own_overwrite`); a file it has not read, or that changed since,
   is refused with an instruction to read it (see
   [tools-and-sandbox.md](tools-and-sandbox.md#edits)). Every call and result is recorded in
   the event log; results go back to the model with their call id.
8. **Detectors** (below) may end the turn with a stop reason.

The turn ends when the model answers in prose, `complete` is carried out, the
person presses Stop, or a detector or budget stops it. It returns a
`TurnReport` (actions, whether it edited, whether it completed or declined,
the stop reason) and the new message list.

**After the turn** — not inside it — the host's `run_chat_turn` hands the
repository's checks to `executor::close_turn` if the turn edited anything, and
the verdict is appended to the answer and to the history (see
[verification.md](verification.md#after-a-conversation-turn)).

### Limits of a turn

| Limit | Value | Where | What happens |
|---|---|---|---|
| Actions before checking in | 100 (was 26 until 2026-09-30); `actions_per_turn` in `.pwr/chat-config.json` | `DEFAULT_ACTIONS_PER_TURN` | The turn stops and says so; the next message continues. A goal's own limit (`goal_budget`) is used instead and is no longer capped at 26 |
| Compactions per turn | 2 | `COMPACTIONS_PER_TURN` | Stop as looping |
| Consecutive empty replies | 3 | `EMPTY_TURNS_BEFORE_GIVING_UP` | Stop as silent |
| Consecutive unreadable calls | 3 | `UNPARSEABLE_CALLS_BEFORE_GIVING_UP` | Stop as unparseable |
| Replies cut off inside a tool call, **in all** (not consecutive) | 3 | `CUT_OFF_REPLIES_PER_TURN` | The retry is capped at 8,192 tokens and the model is told to split the file (`replace_text`/`apply_patch` for the rest); the third stops the turn as unparseable |
| Consecutive backend faults | 3 | `BACKEND_FAULTS_BEFORE_GIVING_UP` | Stop as backend failing; edits so far are kept in the history |
| Reasoning finalization retries | 1 | `REASONING_FINALIZATION_RETRIES` | Stop as reasoning unfinished |
| Unstructured reply guard | 3,000 / 12,000 chars | `AGENT_UNSTRUCTURED_REPLY_GUARD` | A long prose reply after long thinking is retried once at 8,192 max tokens |
| Same refused action | 3 | `REPEATED_REFUSAL_LIMIT` | The model is told to stop proposing it |
| Echoed results | 3 in 10 | `ECHO_LIMIT`, `ECHO_WINDOW` | Flagged to the model |
| Same failed command | 2 | `REPEATED_FAILURE_LIMIT` | Not run a third time |
| Failed runs in a row after edits | 5 | `FAILED_RUN_LIMIT` | The model is told to hand over what is ready |
| No-progress windows | 3 of 6 actions | `NO_PROGRESS_LIMIT`, `NO_PROGRESS_WINDOW` | Stop as no progress |
| Reasoning kept per step | 16,000 chars | `REASONING_KEPT_CHARS` | Older reasoning is cut from the start |
| Replies that fell apart, in a row, on a prompt over 20,000 tokens | 2 | `DEGENERATE_REPLIES_BEFORE_RESET`, `DEGENERATE_RESETS_PER_TURN` (2), `DEGENERATE_RESET_ROOM` (8,192), `DEGENERATE_RESET_MIN_PROMPT_TOKENS` (20,000) | The history is compacted to that room and the turn goes on |
| A file written again and again | every 12th write of one path | `REWRITES_BEFORE_NOTE`, `stall.rs` | The write's result carries a note naming the count and the ways out; no limit |

Each limit has its own counter; there is no shared recovery budget (plan W2.6).

### Replies that fall apart

Three mechanisms act on a generation that loops or collapses, all added on
2026-10-01 from runs of Qwen3.5-9B and Qwen3-Coder-30B
([experiment-log.md](experiment-log.md)), and none of them measured for its
effect yet:

- the engine's own repetition stop and PWR's reader of the stream both end a
  reply that writes one passage again and again (`looping_reply`, or
  `runaway_reply` with a repetition detail);
- once a reply of the turn has looped — **by PWR's reader of the stream or by the engine's own repetition stop** (the commoner one; until 2026-10-01 only the first counted, so the penalty never fired in the app) — every later generation of the turn asks
  for a **presence penalty of 1.0 over the last 1,024 tokens** (the engine's
  own window is 20, which cannot see a repeated passage), unless a higher value
  is set — `presence_penalty`, `presence_context_size` in the request;
- two such replies in a row compact the history (above), **only above 20,000
  tokens**: the same model answers a fresh 30,000-token prompt correctly, so on a
  long history what is cut back is what it was lost in. On a short one it is
  not (Gemma 4 26B, 2026-10-01: six clean starts at 11,000–15,000 tokens and the
  replies went on repeating), so nothing is cut.

### Stop reasons

`StopReason`: `Interrupted`, `ContextFull`, `Looping`,
`Silent`, `ToolCallInReasoning`, `Unparseable`, `BudgetSpent`,
`BackendFailing`, `NoProgress`, `ReasoningUnfinished`. How they reach the app
is in [pwr-serve.md](pwr-serve.md#stop-reasons).

## Goal mode

A loop around the turn in the session executor
(`pwr_orchestrator::executor`, policy `Goal`), used when the app sends a prompt
with Goal on. The executor is driven through a `SessionHost` and returns a
`SessionEnd`; `serve.rs` is one host, and turns the ending into the ACP reply.
The same executor runs a conversation (policy `Conversation`): one turn, no
verification, no second turn.

1. Before the first turn, the full verification runs once; checks already
   failing are named in the request so the goal neither repairs them nor is
   held open by them.
2. After each turn:
   - a declined or stopped turn (other than *budget spent*) ends the goal;
   - a turn with no actions and no completion counts as idle; three idle
     rounds in a row pause the goal as *stalled*;
   - on `complete`, the full verification runs:
     - checks pass (technical checks alone count, from 2026-09-30), the goal
       edited something, and no review has run yet →
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
   - without `complete`, the goal continues with a nudge within the shared goal budget.
3. A checkpoint note is shown every 10 actions.

A goal budget starts **before baseline verification**. Before each generation,
verification, or review, the shared action/refusal/time limits are checked;
verification and review counters are checked before their respective operations.
The remaining action allowance is passed into the conversation and checked
before each call, including multiple calls in one reply. A deadline also bounds
asynchronous work in progress: on expiry the cancellation flag is set and the
operation's future is dropped. Synchronous filesystem work cannot be pre-empted
by Tokio; backend cancellation still depends on its existing safe points.

| Limit | Default | Scope |
|---|---|---|
| Actions | 208 | All turns, including the review continuation |
| Refused completions | 6 | Every failed completion verification, independent of failure names |
| Verification runs | 9 | Baseline plus completion verifications |
| Review rounds | 1 | Specification review and its continuation |
| Wall-clock | 3,600 seconds | From baseline through final result |
| Idle rounds | 3 | Existing stalled guard |
| Same failing set on completion | 3 | Existing blocked guard; W1.5 still open |
| Checkpoint note | every 10 actions | Progress indication |

Workspace overrides live under `goal_budget` in `.pwr/chat-config.json`:

```json
{
  "goal_budget": {
    "actions": 208,
    "refused_completions": 6,
    "verification_runs": 9,
    "review_rounds": 1,
    "wall": 3600
  }
}
```

Missing fields use defaults; unknown budget fields and invalid types are
configuration errors. Zero means no allowance for that operation. Review remains
at most once even if the configured cap is larger. A deliberate new prompt
starts a new budget; this is per-goal, not a session-wide quota.

Reached limits return terminal `budget`, the specific limit and spent/allowed
values in `_meta.pwr.budget`, and a human explanation in `goal.reason` (also shown
by the desktop). Every goal response records effective limits and counters in
`_meta.pwr.goalBudget`; errors record them in `error.data.goalBudget`. Successful,
blocked and stalled goals retain their existing terminal semantics. Ordinary
chat turns do not use this goal budget.

## The scripted run

`pwr run "<task>"` and every `pwr eval run` trial use
`run_action_loop_with_prompt_budget_and_context_tiers`
(`crates/pwr-orchestrator/src/lib.rs`). Unattended: grants only what
`--approve` names. It differs from the conversation in ways a campaign
measures and the app does not ship:

- a hard action budget (`DEFAULT_MAX_ACTIONS` 26, or
  `--max-actions`), with two provider turns per action (`TURNS_PER_ACTION`);
- a baseline of the checks before editing, checks on completion, and a
  recovery cycle (reproduce, classify, diagnose, retry within budget; stop on
  environment, policy or non-determinism);
- the same three completion holds as the conversation: unseen results, nothing
  done (asked once) and *never ran* — a program that was only syntax-checked
  must be run once (`completion_held`);
- `record_progress` (offered only with `--plan`) and `propose_verifier` (a
  check the person adopts) in its catalogue;
- ledger compaction, and optional context policies (`--context-policy
  current | recency-fill | evidence-state`);
- context-tier retry: a prompt the backend refuses is retried at a calibrated
  smaller window;
- `--plan` decomposition, `--provision` toolchain installs, `--session`
  continuation;
- completion with no usable verifier ends `verified: false,
  verifiable: false` .

The B0 (conventional loop) and B2 (fixed staged workflow) controls live in
`crates/pwr-orchestrator/src/baseline.rs` and share the tools, policy and log.

## Known defects

| Defect | Evidence | Plan |
|---|---|---|
| Post-turn verification happens after the turn ended, so a failing verdict does not send the model back | `executor::close_turn` | W7.1 decides |
| The app, the console and Goal mode share one executor; the scripted loop (`pwr run`, `eval run`) still has its own holds, compaction, recovery and catalogue | [plan/executor-parity.md](plan/executor-parity.md); the evaluator is not on the executor | W2.4 (open) |
| A dozen independent limits and no shared recovery budget | table above | W2.6 |
| A model that thrashes (rewrites one file seventy times) is told, not stopped; a goal's wall-clock is what ends it | `dev11-qwen3-coder`, 130 actions in 60 min | measure the note, then decide |
| The objective is kept whole, outside what compaction may shorten (W4.1) | — | done on `develop`; CI not run |
