# `pwr serve` — the protocol between the PWR app and its core

**Product decision, 2026-09-16.** PWR's user-facing product is **its own
desktop application, for macOS and Windows** -- a standalone app in the manner of
the ChatGPT and Claude desktop apps: a simple interface that makes clear what the
agent is doing. **It is the only front end PWR has.** PWR is not a plugin for
code editors or IDEs, and editor integration is not a goal. An earlier version of
this document treated compatibility with ACP editors as a first-class benefit and
made a manual pass in one of them an exit criterion; that was a misreading of the
product and is corrected throughout.

**Status: S1 IMPLEMENTED; increments to 2026-09-23.** Since the first
version: replies and reasoning **stream** as they are generated
(`agent_thought_chunk` for reasoning, `agent_message_chunk` with
`_meta.pwr.live: true` for the reply in progress, 2026-09-22); the window in
use is reported as a `_pwr/usage` notification (`used`, `window`) after
every generation; `_pwr/approvals` takes and returns the permission
`mode` (`ask` / `auto`), the kinds actually asked about (`asking`) and whether
commands are `sandboxed` (2026-09-23). **Also 2026-09-23** (details in
[`models-and-context.md`](models-and-context.md)): `_pwr/context`
(`sessionId`, optional `autoCompactPercent` 50–90) returns the window, the
engine's count or an estimate, an estimated composition, the auto-compaction
threshold and the last compaction; `_pwr/compact` (`sessionId`) compacts
between turns through the same function as automatic compaction and records
`context.compacted`; every compaction is announced as a `_pwr/compacted`
notification (`sessionId`, `trigger`, `note`); `_pwr/hardware` returns the
normalized host profile and each engine's readiness; `_pwr/catalog`
(`query`, `format` `mlx`/`gguf`, `filters`) searches the Hugging Face Hub and
returns rated variants; `_pwr/download` also takes a Hub variant
(`repository`, `revision`, `variant`, `format`) whose files the core re-reads
at that commit, and `_pwr/download_progress` now carries the download's
`state` (`preparing`, `downloading`, `verifying`, `completed`, `failed` with a
`kind`, `cancelled`); `_pwr/local_models` (`cwd`) lists the models on
this machine and `_pwr/model_delete` (`cwd`, `modelRef`, `format`)
deletes one, refusing the model the workspace uses. **2026-09-24** (details in
[`model-compatibility.md`](model-compatibility.md)): `_pwr/models` also
returns `compatibility` (the selected model's status -- `verified`,
`locally_calibrated`, `provisional`, `limited`, `incompatible` -- with
confidence, reasons, `features` {`chat`, `agent`}, calibration `checks`,
succinct `capabilities` and provenance) and `reasoning` (how Reasoning
Effort applies: `applies`, `control`, `capability`, `budgets`,
`budgetSource`); it takes `reasoningEffort` (`low` / `medium` / `high`, any
other value refused) and `acknowledgeProvisional: true` ("Use Conservative
Defaults"). `_pwr/quick_calibration` (`cwd`, `calibrationId`) runs Quick
Calibration on the workspace's model, reports `_pwr/calibration_progress`
(`calibrationId`, `step`, `total`, `name`) and returns `assessment` and
`reasoning`; the notification `_pwr/quick_calibration_cancel`
(`calibrationId`) stops it, including the generation in progress. One
calibration runs at a time. `_pwr/context` also returns `lastGeneration`
(the last reply's reasoning and answer tokens, their `tokenAccounting`, the
effort, the effective budget and whether it was lowered to fit). The Tauri app (`apps/desktop`) is the client of
record. What follows is the description of 2026-09-20 with those
additions marked where they change it.
`pwr serve --stdio` (`crates/pwr-cli/src/serve.rs`) speaks `initialize`;
`session/new`, `load`, `resume`, `list` and `close`; `session/prompt` with text,
attachments, tool calls, diffs and the answer; `session/request_permission`;
`session/cancel`; the console's commands as `available_commands_update`, as
`/name` prompts and as `_pwr/<name>` requests; `_pwr/steer`; and
`_pwr/models`, `_pwr/download` and `_pwr/approvals`. All of it runs the console's own turn,
resume, commands and attachment handling. Every message the server sends is
validated against the published ACP schema in the tests, and a golden transcript
fixes what a client sees for an edit, an approval granted and one refused. The
manual end-to-end pass belongs to the PWR app and moves to S3. Three permission options
are offered, since `reject_always` needs the denylist of open question 2.
`_pwr/models` is a read when called without `model`; with a non-empty
`model` it selects an artifact from the active backend's discovered catalog and
writes only that workspace's chat configuration. It cannot select an arbitrary
path or change engine. It also computes the working window. Its read response
now includes declared downloadable artifacts and their local final/`.part` byte
state; it never hashes files or starts the network merely to refresh status.
`_pwr/download` starts a declared artifact download for a workspace named by
`cwd`; a `sessionId` is optional. The caller supplies a `downloadId`, receives
byte progress as `_pwr/download_progress` notifications, and can send
`_pwr/download_cancel` at any time. Cancellation preserves the `.part` file
for a later retry, including across a client restart because status is derived
from disk. Nothing here is
evidence that a front end improves anything; that is R5's to measure.

## Why a protocol before the app

PWR's value is the harness: the session step both loops share
(`pwr_orchestrator::session`), its policy and sandbox, the audit, the
verification, the continuation record. An app that embedded the loop would be a
third copy of the state machine R1 just reduced to one boundary. So the app is a
client, and what it is a client *of* comes first: a local process, started by the
app, that speaks a documented protocol and runs the same session step the
terminal console runs.

That separation is what the protocol is for:

- **the app draws, the core decides.** Everything that makes a run safe and
  auditable -- policy, sandbox, approvals, verification, the log -- stays in the
  core, and the app cannot bypass it because it never holds it;
- **the core is testable without the app.** Every behaviour the app will show is
  already pinned by protocol tests, a schema check and a golden transcript,
  before a line of interface exists;
- **the terminal console is not a product surface.** It stays as the development
  and research tool the campaigns run through until the app covers what it does,
  and it may later become a client of the same protocol rather than a second
  front end.

## Decision: speak the Agent Client Protocol, extend it where PWR differs

The [Agent Client Protocol](https://agentclientprotocol.com) (ACP, v1) is
JSON-RPC 2.0 over stdio between a client and an agent running as its subprocess,
with capability negotiation at `initialize`. Its shape matches what PWR
already has:
sessions, prompt turns that stream updates, tool calls with statuses and diffs,
permission requests with allow/reject options, cancellation with a defined stop
reason, and session load/resume/list.

Adopting it rather than inventing a protocol means the app inherits a
specification instead of one we would have to write and keep: a versioned method
set, a published JSON schema every message is tested against, and defined
semantics for the hard cases -- cancellation, a permission question left open, a
session reloaded after a crash. It was chosen for that, not for the editors that
also speak it. That an editor could drive `pwr serve` is a side effect; it is
not supported, not tested, and not a reason to shape anything. Where PWR has
something ACP does not, it is carried in `_meta` or in `_pwr/*` extension
methods, never by bending an ACP method's meaning. (ACP reserves names that
begin with `_` for implementations, so the leading underscore is required.)

The spec was checked on 2026-09-15; the method and enum names below are from it.
It is versioned, and `initialize` negotiates the version, so a later revision is
an upgrade to plan rather than a break to absorb.

## Mapping

### Lifecycle

| ACP | PWR |
|---|---|
| `initialize` | Advertise `loadSession`, `sessionCapabilities` `list`, `resume` and `close`, and `embeddedContext`; image prompt content (accepted only for a model with a vision encoder, C.25), no audio; no MCP servers (declined until R6 admits one). |
| `initialize` → `_meta.pwr.chatHome` | Chat mode's folder (C.26). A session whose `cwd` is this folder has no workspace: it may only read what is attached -- files, folders (read-only, listed with `list_tree`), images -- and cannot write or run anything. Models and conversations are chosen there as for a workspace. |
| `session/new` (`cwd`) | A conversation rooted at `cwd`, with the workspace's chat config (backend, model, calibration, approvals). Refused with a readable error when the model is not prepared, as the console refuses. |
| `session/load` | `conversation::resume` (restore, reconcile, record `conversation.resumed`), shared with the console's `/resume`: what the person asked and the deployment answered replayed as `user_message_chunk` and `agent_message_chunk` (system prompt, ledgers, excerpts and tool results are not transcript), then the reconciliation note (files edited since, uncertain writes) as an agent message. The session id is the conversation id. |
| `session/resume` | The same resume without the replay; the note is still sent. |
| `session/list` | `conversation::list`: the workspace's conversations that completed a turn, most recently active first, titled by their first request, 50 a page with an offset cursor. `cwd` is required, since the log is per workspace; a directory with no log lists nothing and is not given one. |
| `session/close` | The session is dropped and a running turn stopped. `serve` loads no model itself (the backend holds it), so there is nothing to release. |

### A prompt turn

| ACP | PWR |
|---|---|
| `session/prompt` | One `converse::take_turn`. Text blocks are the request; `resource_link` (a `file:` URI) and embedded `resource` blocks are attachments, extracted, bounded and snapshotted by content hash exactly as the console's `/attach`, and appended to that turn only. A `resource_link` to a folder outside the workspace is not pasted: it is declared as a read-only reference folder (`reference_roots` in the workspace's `chat-config.json`, `..` for an ancestor), readable with `read_file` for the rest of the session and never writable, and the attachment lists its Markdown documents (2026-09-22). An attachment that cannot be read refuses the prompt rather than running a turn without it. `image` blocks, and a `resource_link` to a PNG, JPEG, WebP or GIF, are images: stored in `.pwr/images/<sha256>.<ext>` and shown to the model before the text, when the chosen model has a vision encoder and the engine has `mlx-vlm` (C.25, 2026-09-23); otherwise the prompt is refused with "image refused: ...". Audio is refused, as `initialize` said. |
| `session/update` `agent_message_chunk` | The turn's answer, and the post-turn check verdict. **Streamed since 2026-09-22:** chunks of a reply still being generated carry `_meta.pwr.live: true`; the final text follows without it. |
| `session/update` `agent_thought_chunk` | The model's reasoning as it is generated (2026-09-22), for models that reason. |
| `_pwr/usage` (notification) | After each generation: `used` (prompt plus generated tokens) and `window`, for a context meter (2026-09-22). |
| `session/update` `tool_call` | `pending`, when the turn proposes an action and before the policy is asked about it, with `kind` from the action (below) and its file in `locations`. |
| `session/update` `tool_call_update` | `in_progress` at the intent, `completed` or `failed` at the receipt; an edit carries `diff` content (`path`, `oldText`, `newText`) from the file before and after. An action refused at the gate goes straight to `failed`. |
| `session/request_permission` | `session::gate` reaching an approval the policy does not hold, carrying the `toolCallId` of the action just proposed. Options `allow_once`, `allow_always` (PWR's "for this session"), `reject_once`; `reject_always` waits on a session denylist PWR does not have. `cancelled` outcome is a refusal. |
| `session/cancel` | The turn's stop flag. The prompt answers with stop reason `cancelled`, as the spec requires. |
| `session/update` `available_commands_update` | `changes`, `verify`, `report`, `diagnose`, `doctor`, sent after a session is created, loaded or resumed. A prompt that is only `/name` runs the command and ends `end_turn`; any other slash is the deployment's to read. Session listing is `session/list`, not a command. |

Tool kinds: `read_file` → `read`; `search`, `find_definition`, `list_tree` →
`search`; `replace_text`, `apply_patch`, `apply_replace`, `write_file`,
`make_directory` → `edit`; `delete_path` → `delete`; `move_path` → `move`;
`run_command`, `start_service`, `stop_service` → `execute`; `fetch_url` →
`fetch`; everything else → `other`.

### Stop reasons

ACP defines `end_turn`, `max_tokens`, `max_turn_requests`, `refusal` and
`cancelled`. Every response also carries PWR's `TerminalClass` in
`_meta.pwr.terminal`, because several of PWR's endings have no ACP name and
a client should not have to parse prose to tell a backend fault from a stuck
deployment.

| PWR ending | ACP | `_meta.pwr.terminal` |
|---|---|---|
| answered | `end_turn` | — |
| declined | `refusal` | `declined` |
| `Interrupted` | `cancelled` | `interrupted` |
| `BudgetSpent` | `max_turn_requests` | `budget` |
| `ContextFull`, `Looping`, `NoProgress` | `max_tokens` | `recovery` |
| `Silent`, `Unparseable` | `end_turn` | `protocol` |
| `BackendFailing` | `end_turn` | `provider` |

### What stays PWR's, and why

- **File system and terminal stay in the agent.** ACP lets an agent read and
  write through the client (`fs/*`) and run commands in the client's terminal
  (`terminal/*`). PWR does not delegate either: the sandbox, the policy, the
  audit and the protected state are the harness, and an effect executed by the
  client is one PWR can neither confine nor record. Declared, not an
  omission. The app shows files and diffs; it never writes them.
- **Steering.** A message typed while a turn works is delivered at the next
  action boundary. ACP has no mid-turn prompt, so `_pwr/steer`
  (`sessionId`, `text`) carries it; between turns it is refused, as the text is
  then a prompt.
- **Extensions** (`_pwr/*`, each taking `sessionId`): `steer`; `changes`,
  `verify`, `report`, `diagnose` and `doctor`, answering `{text}` with the
  console's own summary; later `models` (backend, model, readiness, prepare)
  and `approvals` (the ask-before settings).

## Architecture

- **Crate.** `converse` lives in `pwr-orchestrator` (moved from
  `pwr-cli` on 2026-09-15), so the CLI and the server share one turn. Its
  `on_step` callback is the event sink: the console renders each `TurnStep`,
  and the server translates each into `session/update`. Steps are delivered
  synchronously, on the turn's own task: the first implementation queued them
  for another task to forward, and the golden transcript caught a permission
  request reaching the client before the action it asked about.
- **Process.** `pwr serve --stdio`: one process per app window or workspace,
  launched by the app, talking JSON-RPC on stdin/stdout, logging on stderr. No
  network listener; a remote client is out of scope.
- **Policy.** Identical to the console's: the workspace's derived allowlist,
  sandbox, protected state, and approvals asked through the client.
- **Concurrency.** One active turn per session; `session/prompt` on a busy
  session is refused with a JSON-RPC error rather than queued.
- **Audit.** Every session is a conversation in `.pwr/state.sqlite`, so
  `pwr report` and `--continue` read a served session exactly as a console
  one.

## Testing

- Implemented in S1.1–S1.2: protocol tests in `serve.rs` over an in-memory
  duplex with a scripted runner -- initialize, a refused session, an edit
  streamed with its diff, a declined request, a conversation across turns,
  permission granted for the session and refused, cancellation of an open
  question, a busy session, unknown methods and malformed input, load with
  replay and reconciliation then a continued turn, resume, unloadable sessions,
  paged listing, close, steering, commands as extensions and slash prompts, and
  what a replay omits. `conversation::list` and `resume` are tested against a
  real log in `pwr-orchestrator/tests/conversation_list.rs`.
- Protocol fixtures with the scripted providers `two_loops` uses: a golden
  JSON-RPC transcript per case (answer, edit with diff, permission granted and
  refused, cancellation, load with reconciliation), compared byte for byte
  after normalising ids and times.
- Implemented: every message the server sends is validated against the
  published ACP schema (`crates/pwr-cli/tests/fixtures/acp/schema-v1.json`,
  vendored at a recorded commit) by type -- a notification or request by its
  method, a response by the method it answers. A negative test checks the
  check: an invented tool kind, an unknown update type and a stop reason ACP
  does not define are all rejected.
- Implemented: a golden transcript
  (`tests/fixtures/acp/transcripts/edit-permission-granted-and-refused.jsonl`)
  of `converse::take_turn` driven by a scripted deployment in a real workspace
  -- an edit, a dependency change approved, and the same change refused in the
  next turn -- compared byte for byte with the session id and workspace path
  replaced. `PWR_UPDATE_GOLDEN=1` rewrites it, and the diff is the review.
- **In S3:** the manual end-to-end pass is done with the PWR app itself, not
  with a third-party client. The checklist is below and is part of S3's exit.

## The manual pass with the app (S3)

After the frontend integration work is complete, run the app against a workspace
whose model is chosen and prepared -- chosen and prepared from inside the app --
and record the complete pass here. Do not repeat this manual checklist after
each protocol increment; automated tests cover those increments while the
surface is still moving.

Launch the supported Tauri app in development mode from `apps/desktop`:

```sh
cd apps/desktop
npm install
npx tauri dev
```

1. a question answered, with the answer shown as it arrives;
2. a read, then an edit, with the diff shown clearly enough to judge;
3. an approval prompt for a dependency change: granted once, then refused;
4. a stop pressed mid-turn, and the turn ending as stopped;
5. a file attached by dragging or picking it, and the answer showing it was read;
6. quitting the app and reopening the conversation, with the history back and
   the note about what changed while it was closed;
7. running the repository's checks from the app.

## The PWR app (S2–S3)

**Current implementation:** Tauri 2 with Angular in `apps/desktop`. The Slint
spike was retired and its prototype removed after the Tauri app became the
supported desktop client; the historical comparison below records that decision.

**What it is.** A standalone desktop application for **macOS and Windows**, and
PWR's only front end. The model is the ChatGPT and Claude desktop apps: the
conversation is the centre, and everything else exists to make the agent's work
legible to the person relying on it. Linux is not a target.

**What it must make clear**, because this is where a local agent is harder to
trust than a hosted one:

- the conversation, with the answer as it arrives;
- what the agent is doing right now, as a readable feed of actions -- reading,
  searching, editing, running the checks -- not as log lines;
- every change it makes, as a diff a person can judge before relying on it;
- when it needs permission, what exactly it is asking and why, with a clear
  allow-once, allow-for-this-conversation and refuse;
- why a turn ended -- finished, stopped, out of budget, the model not
  responding -- in plain language, from the terminal class the core already
  reports;
- the conversations in a workspace, reopened with what changed while closed;
- which model is running, whether it is ready, and preparing one.

**How it is built.** A client of `pwr serve` that it starts itself, from a
Rust process that shares the core's protocol types. *Revised 2026-09-22:* the
interface is Tauri 2 with an Angular frontend in the system webview; the
earlier "no web UI" rule is withdrawn (reasons in the roadmap's S2 entry).
Electron stays excluded for its memory beside a local model.

**What Windows requires of the core, not only of the app.** The app starts the
core on the same machine, so shipping on Windows means the core runs there too --
backends, tools, verification and, above all, execution isolation. Today the
sandbox is macOS's `sandbox-exec` (Seatbelt) and nothing confines a command
elsewhere. The harness does not ship on a platform where it cannot confine what
the agent runs, so Windows isolation -- a job object with a restricted token or an
AppContainer, with the same write-confinement and protected-state guarantees the
Seatbelt profile's tests pin -- is a prerequisite of the Windows build, with tests
of its own, and not a detail of the app.

**Choosing the toolkit (S2).** Two candidates, chosen by a timeboxed spike that
builds the same screen in both -- conversation, action feed, a diff of one edit,
and a permission prompt:

| | GPUI (Zed's framework) | Slint |
|---|---|---|
| Strength | Text, long lists and diffs, GPU-rendered | Declarative, native look, mature on macOS and Windows |
| Risk | Built for a code editor rather than a conversation app; API still moving, thin documentation; Windows support newer | A diff view to build; licence terms to confirm |
| Fits | A code-centred interface | A conversation-centred app with panels |

The clarified product shifts the weight of that table: PWR is
conversation-first and must ship on Windows as well as macOS, which is where
GPUI is weakest and Slint strongest. The spike still decides, because diff
rendering and long conversations are where Slint has more to build.

Spike criteria, fixed before it starts: time to the four-panel screen; a long
conversation and a diff of a 2,000-line file rendered without lag; clarity to
someone who has not used PWR, judged on the permission prompt and the action
feed; keyboard operation; startup time and memory idle; build, signing and
packaging on macOS and Windows; licence terms. egui is a prototyping fallback,
not a product candidate.

## Phases

| | Scope | Exit |
|---|---|---|
| **S1** protocol | Move `converse` to a library; `pwr serve --stdio` with lifecycle, prompt turns, tool calls, permissions, cancellation, load/resume/list, attachments and `_pwr/*` extensions. **Done 2026-09-16.** | Protocol tests, schema validation and the golden transcript pass; the terminal console's behaviour is unchanged. |
| **S2** toolkit spike | GPUI and Slint, same four-panel screen, as `serve` clients, on macOS and Windows | A written choice against the fixed criteria. |
| **S3** the PWR app | The chosen toolkit, for macOS and Windows, covering everything the terminal console does -- including choosing and preparing a model | The manual pass above completed and recorded; every capability admitted so far is drivable from the app; signed installable builds for both platforms. It is part of R5's product evaluation, not a substitute for it. |

S1 is backend and can proceed now without touching frozen measurement binaries.
B10 begins S2 with this narrow model-selection seam, then a toolkit spike can
test the actual conversation/action/diff/permission surface without taking a
position on an unfinished download workflow. S3 lands before R5's held-out
evaluation.

## Open questions

1. Token-level streaming: worth changing `collect_reply` for, or reply-sized
   chunks until a client shows they matter?
2. `reject_always` needs a session denylist PWR does not have. Add it, or
   offer only the three options PWR can honour?
3. Should `session/set_mode` map to a read-only "ask" mode (every mutating
   capability refused at the gate) and a normal "code" mode?
4. Model download and expensive preparation take minutes. `_pwr/models` now
   selects an already discovered model, computes its window and reports the
   non-verifying local state of declared download files. It still needs download
   progress notifications, cancellation and resumption after the app quits.
