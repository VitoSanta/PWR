# `pwr serve` — the protocol between the app and the core

**Checked against `develop` at `bff93062`, 2026-10-01** (`crates/pwr-cli/src/serve.rs`). The design history of this protocol, including its
phases and the decision to adopt ACP, is in the
[archived version](archive/pwr-serve.md).

## Transport

`pwr serve --stdio`: one process per app window, launched by the app,
JSON-RPC 2.0 over stdin/stdout, logs on stderr, no network listener. It speaks
the [Agent Client Protocol](https://agentclientprotocol.com) (v1) and adds
what ACP has no name for as `_pwr/*` methods and `_meta.pwr` fields, never by
changing an ACP method's meaning. Editors that speak ACP could connect; that
is neither supported nor tested — the app is the only client.

File system and terminal stay in the core: PWR does not use ACP's `fs/*` or
`terminal/*` delegation, because an effect executed by the client could be
neither confined nor recorded.

## ACP methods

| Method | What PWR does |
|---|---|
| `initialize` | Advertises `loadSession`, session list/resume/close, prompt capabilities (image yes, audio no, embedded context yes); `_meta.pwr.chatHome` is chat mode's folder |
| `session/new` (`cwd`) | A conversation rooted at `cwd`, with that workspace's configuration; refused with a readable error if the model is not ready. A session whose `cwd` is the chat home has no workspace |
| `session/load` | Restores a conversation, replays what was asked and answered, then a note on what changed on disk since |
| `session/resume` | The same, without the replay |
| `session/list` | The workspace's conversations, most recent first, 50 a page |
| `session/close` | Drops the session and stops a running turn |
| `session/prompt` | One turn (or a goal, with `goalMode: true`). Text is the request; `resource_link` and `resource` blocks are attachments (a folder outside the workspace becomes a read-only reference); images only for a vision model. One active turn per session: a second prompt is refused. `harness: "minimal"` (not with `goalMode`) runs the W8.3 control the stack-matrix runner compares PWR against: one turn under the goal's action and time budgets with PWR's harness off, and the reply's `_meta.pwr.harness` says `minimal`; the app never sends it |
| `session/cancel` (notification) | Stops the turn; the prompt answers `cancelled` |
| `session/update` (to the client) | `agent_message_chunk` (streamed with `_meta.pwr.live: true`, then final), `agent_thought_chunk` (reasoning), `tool_call` / `tool_call_update` (with `kind`, locations, diffs), `user_message_chunk` on replay, `available_commands_update` |
| `session/request_permission` (to the client) | An approval the policy does not hold; options `allow_once`, `allow_always` (for this session), `reject_once`. There is no `reject_always` |

Tool kinds: `read_file` → `read`; `search`, `find_definition`, `list_tree` →
`search`; edits → `edit`; `delete_path` → `delete`; `move_path` → `move`;
commands and services → `execute`; `fetch_url` → `fetch`; else `other`
(`tool_kind`).

## PWR extensions

Requests (each takes the session or workspace it applies to):

| Method | Purpose |
|---|---|
| `_pwr/steer` | Deliver a message into the running turn at the next action boundary; refused between turns |
| `_pwr/models`, `_pwr/local_models`, `_pwr/catalog`, `_pwr/hardware` | Model readiness and selection; models on disk; the Hub catalogue with fit ratings; this machine |
| `_pwr/download`, `_pwr/download_cancel` | Download a model variant (progress as notifications); cancel it |
| `_pwr/model_delete` | Delete a model's files, with the refusals in [models.md](models.md#downloads) |
| `_pwr/model_sampling` | A model's sampling settings |
| `_pwr/quick_calibration`, `_pwr/quick_calibration_cancel` | Run or cancel Quick Calibration |
| `_pwr/context`, `_pwr/compact` | Window, usage and composition; compact now |
| `_pwr/approvals` | The permission mode and the asked-before list |
| `_pwr/revert` | Put a changed file back, refused if it no longer holds what the model wrote |
| `_pwr/rewind` | Rewind the conversation to one of the person's messages, restoring files this session changed |
| `_pwr/files`, `_pwr/file` | List and read workspace files for the app |
| `_pwr/profile`, `_pwr/memory`, `_pwr/projects` | The person's profile, memories, known projects |
| `_pwr/wiki`, `_pwr/wiki_summarise`, `_pwr/wiki_settings` | The project wiki; start background summaries; turn them on or off (off by default) |
| `_pwr/acceptance_authorize` | The person authorizes one changed acceptance artifact, by path, for this session (journaled before it takes effect; the model has no such tool) |
| `_pwr/session_delete` | Delete a conversation |
| `_pwr/changes`, `_pwr/verify`, `_pwr/report`, `_pwr/diagnose`, `_pwr/doctor` | The commands also offered as `/changes`… in a prompt: files changed and their diff, the repository's checks, the session's recorded report, loops and stalls in its events, whether the backend serves the model |

Notifications to the client: `_pwr/turn_started`, `_pwr/turn_event`,
`_pwr/usage` (used tokens and window after each generation),
`_pwr/compacted`, `_pwr/memory_proposed`, `_pwr/wiki_summarising`,
`_pwr/wiki_updated`, `_pwr/download_progress`, `_pwr/model_progress`,
`_pwr/calibration_progress`. `_pwr/model_progress` is a sign of life while
the engine reads the prompt; from 2026-09-30 it carries
`prefill: {processed, total}` tokens when the engine reports them, and the app
shows "Reading the conversation · 37%".

## Stop reasons

Every prompt response carries ACP's stop reason and, in `_meta.pwr.terminal`,
PWR's own class (`stop_reason`):

| PWR ending | ACP `stopReason` | `_meta.pwr.terminal` |
|---|---|---|
| answered | `end_turn` | — |
| declined | `refusal` | `declined` |
| `Interrupted` | `cancelled` | `interrupted` |
| `BudgetSpent` | `max_turn_requests` | `budget` |
| `ContextFull`, `Looping` | `max_tokens` | `recovery` |
| `NoProgress` | `end_turn` | `recovery` |
| `Silent`, `ToolCallInReasoning`, `Unparseable`, `ReasoningUnfinished` | `end_turn` | `protocol` |
| `BackendFailing` | `end_turn` | `provider` |
| Goal paused at its action budget | `max_turn_requests` | `budget` |
| Goal paused, idle | `end_turn` | `stalled` |
| Goal stopped, same failure | `end_turn` | `blocked` |

A goal's response also carries `_meta.pwr.goal` (`enabled`, `completed`,
`verified`, and whether a guard was reached) and the action totals.

Goal outcomes also carry `_meta.pwr.goalBudget` (effective limits and spent
counters). A budget stop names the limit in `_meta.pwr.budget` and explains it
in `goal.reason`; see [agent-loop.md](agent-loop.md#goal-mode).

Every prompt response also carries the typed **`TurnOutcome`** in
`_meta.pwr.outcome` (delivery, checks, baseline, acceptance, confinement,
budgets, the terminal class); the app reads its status from it (plan W2.1/W7.1,
partial: the scripted runner's own JSON is not migrated).

## Tests

Protocol tests in `serve.rs` drive the server with a fake runner and assert
the messages: lifecycle, streaming, permissions, cancellation, goal endings,
steering. Recorded transcripts are under `crates/pwr-cli/tests/fixtures/acp/`.
