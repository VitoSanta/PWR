# State and persistence

**Checked against `develop` at `0776ff4f`, 2026-09-30.** Every place PWR keeps
state, what kind of state it is, and what may be deleted. The review found the
stores overlapping without a clear hierarchy (§3.4); this map is the first
step of plan W6.2, which then enforces it.

## Kinds of state

- **Authoritative** — decides behaviour; losing it changes what PWR does.
- **Evidence** — records what happened; read to report, resume and audit.
- **Projection** — derived from the workspace or from evidence; can be deleted
  and is rebuilt on demand.
- **Cache** — speeds something up; safe to delete.
- **Configuration** — written by the person or the project.

## In a workspace: `<workspace>/.pwr/`

The sandbox denies commands writes to `.pwr/` and reads of its files (except
in Full access mode); the file tools refuse writes to it.

| Path | Kind | Written by | What it is |
|---|---|---|---|
| `state.sqlite` | Evidence (and authoritative for resume) | the core | The event log: every run, turn, tool attempt, generation, compaction, checkpoint and snapshot, hash-chained |
| `checks.json` | Configuration, authoritative | the owner | Declared checks; `"kind": "acceptance"` marks acceptance checks ([verification.md](verification.md)) |
| `protected.json` | Configuration, authoritative | the owner | Paths the agent may read but not change |
| `chat-config.json` | Configuration, authoritative | the app / console | Model, window setting, reasoning effort, permission mode, asked-before list, compaction threshold, reference folders, acknowledged provisional models |
| `instructions.md` | Configuration | the project | Project instructions (else `AGENTS.md`, else `PWR.md` at the root) |
| `memory.json` | Configuration | the person (a model only proposes) | Workspace memories |
| `indexes/` | Cache | the core | Repository index (SQLite, keyed on mtime and size) |
| `wiki/overview.md`, `wiki/graph.json` | Projection | the core | Computed overview and knowledge graph |
| `wiki/log.md` | Projection of evidence | the core | Work log, one entry per turn that changed or finished something |
| `wiki/summaries.json` | Projection (unverified model text) | the model, in the background | Module summaries, each with the hash of what it describes |
| `embeddings/<model>.json` | Cache | the core (opt-in) | Section embeddings for semantic retrieval; no eviction |
| `images/` | Evidence | the core | Images attached or captured (`look_at`) |
| `chat-attachments/<hash>.txt` | Evidence | the core | Text snapshots of attached files |
| `calibrations/`, `execution-profiles/` | Evidence (research) | `pwr calibrate`, `pwr run` | Context-ladder calibrations and execution profiles of the scripted loop |
| `evaluations/` | Evidence (research) | `pwr eval run` | Campaign reports (git-ignored in this repository) |

Next to `.pwr/`, `.pwr-scratch/` is the commands' `HOME` and `TMPDIR`, and
`.toolchains/` holds toolchains a task installed.

## Personal: `~/.pwr/` (or `PWR_HOME`)

| Path | Kind | What it is |
|---|---|---|
| `models/<owner>/<name>/` | Data | Downloaded models (`PWR_MLX_MODELS` moves it) |
| `model-evidence/<hash>.json` | Evidence | Local Quick Calibrations (`PWR_EVIDENCE_DIR` moves it) |
| `profile.json` | Configuration | The person's profile |
| `memory.json` | Configuration | Global memories |
| `projects.json` | Projection | Registry of workspaces with a wiki |
| `chat/` | Evidence and configuration | Chat without a workspace: its own `.pwr`-like home (`PWR_CHAT_HOME` moves it) |
| `calibrations/` | Evidence (research) | Calibration profiles |

The engine's Python environment lives in
`~/Library/Application Support/ai.pwr.desktop/engine` (the app) or `.venv-mlx`
(a checkout, `scripts/setup-mlx.sh`).

## The event log

`crates/pwr-store`: one SQLite table, `events` (`id`, `run_id`, `event_type`,
`payload`, `at`, `previous_hash`, `run_previous_hash`, `event_hash`), plus
`schema_migrations` (two migrations). Each event's hash covers its id, run,
type, payload, time and the previous event's hash; a second link chains each
run's events on their own. `verify_run_chain` recomputes a run's chain;
`pwr report --format jsonl <run>` says whether it holds.

A conversation writes, among others: `conversation.checkpoint` (changed files
and their hashes, the objective's revision number), `conversation.snapshot`
(**the whole message list, every turn**), steering, generation and reasoning
events (never the reasoning text), tool attempts, `context.compacted`, check
verdicts, reverts.

**It is a verifiable log, not a tamper-proof one.** It detects an altered or
corrupted event against the chain it holds; it does not stop someone who
controls the file from rewriting the chain or cutting its end, and no external
anchor is planned.

### Known defects

| Defect | Evidence | Plan |
|---|---|---|
| `append` reads the last hash and inserts without a transaction; two writers can produce inconsistent links | `crates/pwr-store/src/lib.rs:188-230` | W6.1 |
| A failure to read the previous hash silently starts a new chain (`.ok()`) | `lib.rs:193-212` | W6.1 |
| A per-run link stored as `NULL` verifies as *unlinked*, not broken | `lib.rs:142-150` | W6.1 |
| No WAL, busy timeout or indexes for per-run queries | no `PRAGMA` or `CREATE INDEX` in the crate | W6.1 |
| A full snapshot of the conversation is appended every turn | `record_snapshot`, `crates/pwr-orchestrator/src/conversation.rs:107` | W6.3 (measure first) |

## Overlaps to resolve

The same session is described by several records: the checkpoint (changed
files), snapshots (messages), the scripted loop's ledger and task state, the
compaction record inside the messages, the wiki's work log, and the in-memory
`FileEdit` list used for rewind (lost when the session ends). Plan W6.2 names
one authoritative record per fact — the event log for what happened, the
checkpoint for file versions and the objective (W4.1), the workspace itself
for content — and makes every other one a projection that a test deletes and
rebuilds. No new form of memory is added before that holds.

## Resume

`pwr chat --continue` and the app's session list restore a conversation from
its last snapshot, and tell the model (and the person) what changed on disk
since — files edited outside, and actions an interrupted turn took after the
snapshot. An action recorded as started without a result has an unknown
effect; making that explicit is plan W6.4. Rewind restores files only within
the current session.
