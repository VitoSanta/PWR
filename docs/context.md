# Context

**IMPLEMENTED, reviewed in the 2026-10-01 F1 context cycle.** What reaches the
model, how it is counted, how it is compacted, and where repository knowledge
comes from.

## What changed since the baseline (`4ae7c5f1`)

- **The objective is kept whole.** Full human objectives and steering revisions
  live in the persisted checkpoint, independent of the compressible history,
  and every compaction carries them verbatim; if they cannot fit the input
  allowance, generation ends as *context full* (plan W4.1).
- **The compaction threshold is 75 % of the granted window**, with no ceiling
  in tokens unless the workspace sets one (`compact_ceiling_tokens`); the
  default ceiling of 32,768 tokens was withdrawn on 2026-10-02
  ([D-2026-10-02-3](decisions.md)). A **clean-start compaction** runs after
  two replies in a row that fell apart on a prompt over 20,000 tokens ([agent-loop.md](agent-loop.md#replies-that-fall-apart)).
- **Embedding** startup and each reply have a 30-second deadline; a timeout kills
  the sidecar, disables semantic requests for that ranker and falls back to the
  lexical candidates, recorded in the events and shown to the person (plan W1.8,
  W4.6 in part). Scripted `ContextCompiled` carries `retrieval_fallback`.
- **Observed windows require acknowledgment.** Metadata/preparation errors are
  explicit; Settings and context-tier recovery do not turn a requested window
  into a granted one. A backend acknowledgment still does not prove physical
  or effective model capacity.
- **The policy trigger differs from physical fit.** An objective above the
  compaction trigger may still fit. If nothing can be folded, the generation
  envelope decides whether prompt plus safety margin leaves answer room.
  When the turn cannot get under the trigger -- the verbatim objective alone
  is above it, or a compaction left the prompt above it -- it says so and from
  then on compacts only when the window leaves no answer room. Before this,
  such a turn spent its two compactions (the first grew the prompt, the
  objective being kept whole) and stopped after three of five reads with the
  16,384-token window mostly empty (`two_loops.rs` regressions, 2026-10-02).
- **Token costs are still estimates**; an exact, template- and tool-aware
  preflight (W4.2) is not implemented.

## What a conversation turn sends

The first message is the **system prompt**, rebuilt each turn
(`compose_chat_turn`, `crates/pwr-cli/src/main.rs`) from:

- PWR's instructions and harness rules (`chat_system_prompt_for`), including that a new project goes directly in the repository root and not in a new subfolder, unless asked (added 2026-10-01: every model scaffolded `ng new <name>` into a subfolder), and that, while the workspace holds no project, the first folder that becomes one is taken as the root by PWR ([D-2026-10-02-4](decisions.md), `crates/pwr-orchestrator/src/scaffold.rs`), including the workspace's reference folders;
- the deployment's own suffix from its profile (a required section: a model
  switched mid-conversation gets its own suffix);
- the personal block (`personal::prompt_block`, bounded to 8,000 bytes):
  the person's profile, workspace and global memories, the names of known
  projects, and the project's instructions (`.pwr/instructions.md`, else
  `AGENTS.md`, else `PWR.md`). Repository instructions reserve room first;
  optional entries cannot evict them, and omitted entries are disclosed.
  Instructions beyond the 6,000-byte excerpt allowance carry a notice to read
  the full file. Project README descriptions reach only recall tool results,
  not the system block.

When a new user request arrives, it is composed with typed sections
(`context::compile`, `crates/pwr-orchestrator/src/context.rs`):

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
whose required part exceeds the budget** from the composer's side: it reports
`over_budget_by`, and the callers reject oversized required input (plan W4.3,
partial until the exact preflight of W4.2).

After that, the history grows with the model's replies (answer, tool calls,
and up to 16,000 characters of its reasoning per step) and the tool results.

## Counting

| Figure | How | Exact? |
|---|---|---|
| Prompt and generated tokens of the last request (MLX) | the engine, with the model's tokenizer | exact, one request stale |
| Same, llama.cpp | the server's usage block | exact for its tokenizer |
| Composition, section costs | `len() / 4` (`CHARS_PER_TOKEN`) | estimate, labelled |
| Compaction trigger | last engine count + `len()/4` of what was appended (`prompt_tokens_now`) | estimate |
| Generation envelope | last engine count + `len()/3` of what was appended (`conservative_prompt_tokens`) | estimate, denser on purpose |
| Tool schemas and template overhead | learned as a constant offset from the engine's counts, capped at 8,192 | learned |

Two defects: tool-call **arguments** appended since the last count are not
counted by either running estimate — a large `write_file` is nearly free until
the next request is measured (verification N1) — and there is no exact count
of the real rendered prompt before a generation. Plan W4.2 fixes both.

## Compaction

`compaction::compact` (`crates/pwr-orchestrator/src/compaction.rs`) is the
only conversation compaction. The turn calls it when the prompt reaches the
threshold (75 % of the granted window by default; 50–90 % per workspace, saved
as `compact_at_percent` in `.pwr/chat-config.json`, and an optional ceiling in
tokens, `compact_ceiling_tokens`, set by hand), and, with a smaller room, after two
replies in a row that fell apart; **Compact now**
(`_pwr/compact`) calls the same function between turns.

It is mechanical — no model writes it. The folded history is replaced by one
record, headed *"Earlier in this conversation, summarised because it no longer
fits. Re-read anything you need rather than relying on this."*, keeping:

- **the objective and every revision of it, verbatim**, from the checkpoint
  (when they cannot fit, the turn stops as *context full* instead); the first
  request is also kept (cut to 800 characters), and up to **12 later requests,
  each cut to a 200-character line**, as the readable history;
- up to 24 actions with their paths or commands, and up to 40 paths;
- up to 6 statements of the model, the last being where it left off;
- every file the conversation changed with its current hash (from the
  checkpoint, not the messages);
- up to 8 failures not followed by a success of the same action;
- what the checks said last.

The system prompt is never folded; the tail kept verbatim is whatever recent
conversation fits in half the room, and at least two messages. Tool output,
superseded ledgers and retrieved passages are dropped and counted. A compaction
that cannot fold anything preserves the original messages, including reasoning.
Successful automatic compaction invalidates the previous whole-history count;
the learned schema/template offset remains in estimates. Both prompt estimates
include serialized tool-call names and arguments, plus reasoning. These remain
heuristics, not exact counts. A later
compaction merges the earlier record. Each compaction records
`context.compacted` in the event log; a manual one also writes a snapshot.
The existing limit of two automatic compactions per turn remains. Exhaustion
is `CompactionBudget` (terminal class `budget`), not a model-loop diagnosis.
Degenerate-reply resets keep their separate existing bound.

**What compaction still loses:** tool output, superseded ledgers and retrieved
passages (counted, not kept), and the exact wording of the model's earlier
reasoning. Whether this record beats recent history plus reads on request is
unmeasured (plan W4.7, W8).

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
only", which the code does not do — and, from `develop`, a **size bound is checked before the read** (plan W1.7).

### Retrieval

For each new request, `retrieved_context` ranks passages lexically: exact
symbol match 40, partial 12, path match 8, content occurrence 2 (at most 8
counted), headings weighted 3× in Markdown; excerpts are windows of ±12 lines,
up to 90 lines per Markdown section, at most 5 excerpts, within the budget
left. Each excerpt carries its file hash and why it was chosen. The block says
it is lexical and a starting point. Files one import away from the three strongest candidates get a smaller
bonus (`graph_neighbours`, `IMPORT_PROXIMITY` 6). A retrieval error from the embedding encoder falls back to the lexical set and says so (plan W4.6, partial: other retrieval paths can still fail silently).

Semantic fusion with a local embedding model exists and is **experimental,
opt-in** (`PWR_SEMANTIC_RETRIEVAL=1`, `crates/pwr-cli/src/semantic.rs`); its
cache under `.pwr/embeddings/` has no eviction (plan W6.5); the encoder's reads now have a 30-second deadline.

`pwr repo rank "<request>"` prints what a turn would be given, with scores.

### Framework guidance

For Angular workspaces only (`angular.json` or `@angular/core`),
`workspace_topology` describes the bootstrap and routing layout, and a
guidance section gives Angular-specific rules. **The guidance says no tool
can render or screenshot the page**, which is false for vision models offered
`look_at` (`context.rs`; plan W4.5).

The MLX request carries the selected logical window (`context_tokens`). After
rendering the complete chat template, tools and images, the sidecar checks the
actual token vector before model prefill and limits generation to remaining
space, including a forced reasoning close. An oversized input returns typed
`PromptTooLarge`: full mode compacts if possible; minimal mode drops complete
old exchanges; irreducible input ends as ContextFull. It does not reload at a
smaller context tier, which cannot fit more input. Rust estimates remain useful
for planning and display; they are not proof that the final prompt fits.

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
Save; memories never grant a permission. A proposal records its originating
workspace, so accepting after switching folders still saves to that origin.
The missing-profile and empty-JSON defaults both enable confirmed memories.

The **graph** has nodes for the project, folders, files, symbols (up to
5,000), packages, work entries and decisions; each edge says how it is known:
*fact* (contains, defines, changed in), *resolved* (relative JS/TS imports,
Rust `crate::`/`super::`, Python dotted modules), *named* (a package import),
*guess* (a test by naming convention; a decision naming a file). It is not a
call graph and not semantic understanding.

**Summaries** are written after every non-chat turn while no session is busy,
up to 8 per idle period, 400 tokens each, no tools, no reasoning — **only when
the person turns them on** (`background_summaries`, off by default since plan W5.2;
an incoming prompt pre-empts one in flight). They occupy the only engine and
are unverified text. A query rechecks hashes in the active workspace; recall
of another project's saved wiki states that source freshness is unknown and
identifies the graph snapshot time.

`remember`, `recall_project` and `wiki_query` are offered to conversations
only, never to scripted runs.

## What the app shows

`_pwr/context` returns the window, the used tokens (engine count or estimate,
labelled), the composition by kind (estimate), the compaction threshold --
computed by the turn's own rule (`Continuity::compaction_room`), so a workspace
ceiling shows; until 2026-10-02 it showed 75 % of the window while a default
ceiling compacted at 32,768 tokens -- and the last compaction; the percentage
is set from the context meter's popover (*Auto-compact at*, 50–90 %); the app's context indicator and panel show them
([desktop.md](desktop.md)).

## Known defects

| Defect | Plan | State |
|---|---|---|
| Rust room planning still estimates tokens; MLX now checks the actual schema/template/image-expanded token vector before prefill | W4.2 | MLX preflight implemented; other-backend parity and effective-context evidence pending |
| Required context can exceed an estimate; MLX refuses actual overflow before prefill, while optional personalization cannot evict the instruction excerpt | W4.3 | partial; full long instruction files still require explicit reads |
| Large tool output is dropped past its bound, not retrievable | W4.4 | open |
| Angular guidance contradicts `look_at` | W4.5 | open |
| Retrieval errors outside the embedding path are silent | W4.6 | partial |
| The embedding cache has no eviction | W6.5 | open |
| No evidence yet that this context system beats recent history plus reads on request | W4.7, W8 | open |
| Whether a 4-bit model stays coherent up to 75 % of a large window (196,608 of 262,144 tokens), and the clean-start compaction, are unmeasured | [D-2026-10-02-3](decisions.md) | unmeasured |
