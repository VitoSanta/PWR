# Context

**Checked against `develop` at `0776ff4f`, 2026-09-30.** What reaches the
model, how it is counted, how it is compacted, and where repository knowledge
comes from.

## What a conversation turn sends

The first message is the **system prompt**, rebuilt each turn
(`compose_chat_turn`, `crates/pwr-cli/src/main.rs:4214`) from:

- PWR's instructions and harness rules (`chat_system_prompt_for`,
  `main.rs:1296`), including the workspace's reference folders;
- the deployment's own suffix from its profile (a required section: a model
  switched mid-conversation gets its own suffix);
- the personal block (`personal::prompt_block`, bounded to 8,000 characters):
  the person's profile, workspace and global memories, the names of known
  projects, and the project's instructions (`.pwr/instructions.md`, else
  `AGENTS.md`, else `PWR.md`).

When a new user request arrives, it is composed with typed sections
(`context::compile`, `crates/pwr-orchestrator/src/context.rs:521`):

| Section | Required | Evicted |
|---|---|---|
| System, model suffix | yes | never |
| Workspace topology (Angular only, `workspace_topology`) | yes | never |
| Task (the request) | yes | never |
| Framework guidance (Angular) | no | third |
| Session ledger | no | second |
| Repository excerpts (retrieval) | no | first |

The budget is the window minus a 25 % output reserve (`OUTPUT_RESERVE_SHARE`).
Optional sections are dropped in eviction order until the rest fits; the last
one considered is truncated instead if at least 2,000 characters of it would
survive. **Required sections are never cut, and nothing refuses a prompt
whose required part exceeds the budget** (plan W4.3).

After that, the history grows with the model's replies (answer, tool calls,
and up to 16,000 characters of its reasoning per step) and the tool results.

## Counting

| Figure | How | Exact? |
|---|---|---|
| Prompt and generated tokens of the last request (MLX) | the engine, with the model's tokenizer | exact, one request stale |
| Same, llama.cpp | the server's usage block | exact for its tokenizer |
| Composition, section costs | `len() / 4` (`CHARS_PER_TOKEN`, `context.rs:129`) | estimate, labelled |
| Compaction trigger | last engine count + `len()/4` of what was appended (`prompt_tokens_now`, `converse.rs:2537`) | estimate |
| Generation envelope | last engine count + `len()/3` of what was appended (`conservative_prompt_tokens`, `converse.rs:2448`) | estimate, denser on purpose |
| Tool schemas and template overhead | learned as a constant offset from the engine's counts, capped at 8,192 (`context.rs:131-150`) | learned |

Two defects: tool-call **arguments** appended since the last count are not
counted by either running estimate — a large `write_file` is nearly free until
the next request is measured (verification N1) — and there is no exact count
of the real rendered prompt before a generation. Plan W4.2 fixes both.

## Compaction

`compaction::compact` (`crates/pwr-orchestrator/src/compaction.rs`) is the
only conversation compaction. The turn calls it when the prompt reaches the
threshold (default 75 % of the window; 50–90 % per workspace, saved as
`compact_at_percent` in `.pwr/chat-config.json`); **Compact now**
(`_pwr/compact`) calls the same function between turns.

It is mechanical — no model writes it. The folded history is replaced by one
record, headed *"Earlier in this conversation, summarised because it no longer
fits. Re-read anything you need rather than relying on this."*, keeping:

- the **first request, cut to 800 characters**, and up to **12 later requests,
  each cut to a 200-character line**;
- up to 24 actions with their paths or commands, and up to 40 paths;
- up to 6 statements of the model, the last being where it left off;
- every file the conversation changed with its current hash (from the
  checkpoint, not the messages);
- up to 8 failures not followed by a success of the same action;
- what the checks said last.

The system prompt is never folded; the tail kept verbatim is whatever recent
conversation fits in half the room, and at least two messages. Tool output,
superseded ledgers and retrieved passages are dropped and counted. A later
compaction merges the earlier record. Each compaction records
`context.compacted` in the event log; a manual one also writes a snapshot.

**The defect:** a long specification loses the constraints that decide
success, by construction. Plan W4.1 keeps the objective and its revisions
verbatim, outside what compaction may shorten, and stops the turn instead of
abbreviating them.

The scripted loop has its own ledger compaction and optional policies
(`recency-fill`, `evidence-state`, `crates/pwr-orchestrator/src/evidence.rs`);
see [agent-loop.md](agent-loop.md#the-scripted-run).

## Repository knowledge

### The index

`pwr_repo::index_incremental` (`crates/pwr-repo/src/lib.rs`) walks the
workspace with `.gitignore` semantics (whether or not it is a Git checkout),
excluding `.git`, `target`, `node_modules`, `.venv`, `.pwr`, `.pwr-scratch`,
and stores per file: content hash, size, shallow symbols (declaration
keywords), imports, test subject by naming convention, and terms. It is cached
in SQLite under `.pwr/indexes/` keyed on **mtime and size** — fast, and not a
proof that content is unchanged.

Files over 1 MB (`MAX_INDEXED_BYTES`) and binary files are left out of the
index entirely — the constant's comment says they are "inventory entries
only", which the code does not do — and **they are read whole before the size
is checked** (plan W1.7).

### Retrieval

For each new request, `retrieved_context` ranks passages lexically: exact
symbol match 40, partial 12, path match 8, content occurrence 2 (at most 8
counted), headings weighted 3× in Markdown; excerpts are windows of ±12 lines,
up to 90 lines per Markdown section, at most 5 excerpts, within the budget
left. Each excerpt carries its file hash and why it was chosen. The block says
it is lexical and a starting point. Files one import away from the three strongest candidates get a smaller
bonus (`graph_neighbours`, `IMPORT_PROXIMITY` 6). **A retrieval error becomes an empty section silently** (plan W4.6).

Semantic fusion with a local embedding model exists and is **experimental,
opt-in** (`PWR_SEMANTIC_RETRIEVAL=1`, `crates/pwr-cli/src/semantic.rs`); its
cache under `.pwr/embeddings/` has no eviction, and the encoder's read has no
timeout (plan W1.8, W6.5).

`pwr repo rank "<request>"` prints what a turn would be given, with scores.

### Framework guidance

For Angular workspaces only (`angular.json` or `@angular/core`),
`workspace_topology` describes the bootstrap and routing layout, and a
guidance section gives Angular-specific rules. **The guidance says no tool
can render or screenshot the page**, which is false for vision models offered
`look_at` (`context.rs:384`; plan W4.5).

## The project wiki and memory

| What | Where | Written by | Reaches the model |
|---|---|---|---|
| Profile | `~/.pwr/profile.json` | the person (Settings) | always, system prompt |
| Global memories | `~/.pwr/memory.json` | the person; a model only proposes (`remember`) | always |
| Workspace memories | `.pwr/memory.json` | same | in that workspace |
| Project instructions | `.pwr/instructions.md` / `AGENTS.md` / `PWR.md` | the project | in that workspace |
| Overview | `.pwr/wiki/overview.md` | computed | on request (`recall_project`) |
| Work log | `.pwr/wiki/log.md` | PWR, after a turn that changed or finished something | on request |
| Graph | `.pwr/wiki/graph.json` | computed from the index, the log and memories | on request (`wiki_query`) |
| Module summaries | `.pwr/wiki/summaries.json` | the model, in the background, labelled unverified | with the graph |
| Project registry | `~/.pwr/projects.json` | PWR | project names, system prompt |

`PWR_HOME` replaces `~/.pwr`. A memory is written only when the person presses
Save; memories never grant a permission.

The **graph** has nodes for the project, folders, files, symbols (up to
5,000), packages, work entries and decisions; each edge says how it is known:
*fact* (contains, defines, changed in), *resolved* (relative JS/TS imports,
Rust `crate::`/`super::`, Python dotted modules), *named* (a package import),
*guess* (a test by naming convention; a decision naming a file). It is not a
call graph and not semantic understanding.

**Summaries** are written after every non-chat turn while no session is busy,
up to 8 per idle period, 400 tokens each, no tools, no reasoning. They occupy
the only engine and are unverified text; plan W5.2 turns them off by default.

`remember`, `recall_project` and `wiki_query` are offered to conversations
only, never to scripted runs.

## What the app shows

`_pwr/context` returns the window, the used tokens (engine count or estimate,
labelled), the composition by kind (estimate), the compaction threshold and
the last compaction; the app's context indicator and panel show them
([desktop.md](desktop.md)).

## Known defects

| Defect | Plan |
|---|---|
| The objective is compressed to 800/200 characters | W4.1 |
| Tool-call arguments uncounted; no exact preflight | W4.2 |
| Required sections can exceed the budget with no signal | W4.3 |
| Large tool output is dropped past its bound, not retrievable | W4.4 |
| Angular guidance contradicts `look_at` | W4.5 |
| Retrieval errors are silent | W4.6 |
| Oversized files read whole before the bound; dropped from the index though documented as inventory entries | W1.7 |
| No evidence yet that this context system beats recent history plus reads on request | W4.7, W8 |
