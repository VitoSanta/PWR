# Profile, memory and the workspace wiki

Written 2026-09-25. What PWR knows about the person and about the projects it
has worked on, where each piece lives, who may write it, and how a model reads
it.

## Three kinds of knowledge, three hands

| What | Where | Written by | Read by the model |
|---|---|---|---|
| Profile (name, role, language, style, about) | `~/.pwr/profile.json` | the person, in Settings → You | always, in the system prompt |
| Memories, global | `~/.pwr/memory.json` | the person; a model only *proposes* (`remember`) | always, in the system prompt |
| Memories, workspace | `<workspace>/.pwr/memory.json` | the same | in that workspace |
| Project instructions | `.pwr/instructions.md`, else `AGENTS.md`, else `PWR.md` | the project | in that workspace |
| Wiki overview | `<workspace>/.pwr/wiki/overview.md` | computed from the files | on request (`recall_project`) |
| Work log | `<workspace>/.pwr/wiki/log.md` | PWR, after each turn that changed or finished something | on request |
| Knowledge graph | `<workspace>/.pwr/wiki/graph.json` | computed from the index, the log and the memories | on request (`wiki_query`) |
| Module summaries | `<workspace>/.pwr/wiki/summaries.json` | the model, in the background, marked unverified | with the graph |
| Project registry | `~/.pwr/projects.json` | PWR | its names, in the system prompt |

`PWR_HOME` replaces `~/.pwr`.

## Why a model may not save a memory

A memory is read on every request after it is saved. A model that saved its own
could be led by any file it reads into saving a standing instruction, and a
small local model misremembers more often than a hosted one. So `remember`
shows the fact to the person above the composer, and nothing is written until
they press Save (in the scope they choose). The memories block tells the model
that memories never grant a permission or override a refusal.

## The prompt block

`personal::prompt_block` adds, in order: the profile (with an instruction to
greet the person by name and follow their language and style), the memories
(workspace first, then global, newest first), the names of known projects, and
the project's instructions. It is bounded at 8,000 characters (about 2,000
tokens); what does not fit is left out and the block says so. It is re-read on
every turn, so a change in Settings reaches the next reply, and the Context
panel counts it as *Profile and memory*.

## The knowledge graph

Nodes: the project, folders, files, symbols (up to 5,000), packages (imports
that are not files here), work entries and decisions (workspace memories).
Edges, each with how it is known:

| Edge | Certainty |
|---|---|
| folder *contains* file, file *defines* symbol | fact |
| file *imports* file | resolved by the language's path rule (JS/TS relative paths, Rust `crate::`/`super::`, Python dotted modules) |
| file *imports* package | by name, not resolved |
| test *tests* file | a guess from naming convention |
| file *was changed in* work entry | fact, from the log |
| decision *is about* file | a guess: the decision names the file |

Built from `pwr_repo`'s incremental index after every turn, so an unchanged
workspace is not re-read. A model asks it with `wiki_query` ("what imports
`price.ts`?", "what changed `cart.ts`?"); the Inspector's Wiki tab asks the same
graph. Nothing a model writes becomes an edge.

## Summaries

After a turn, while no session is working, PWR asks the model for two or three
sentences about each source folder whose files changed since its last summary
(up to eight per idle period, no tools, no reasoning, 400 tokens). The input is
read by the harness: the folder's files, what each defines, and the opening of
the three largest. Each summary records the hash of the files it describes; when
they change it is shown as written before its files changed until it is
rewritten. Summaries are always labelled as written by a model and unverified.

## Recalling a project from anywhere

Every workspace with a wiki is in `~/.pwr/projects.json`. The system prompt
lists their names; `recall_project` returns one's overview and newest work
entries, and `wiki_query` with `project` asks its graph. Both read the wiki
only, never the other workspace's files: remembering a project does not widen
what a conversation may read. Settings → Memory lists the projects; forgetting
one removes it from the list and leaves its folder and wiki alone.

## Not in scripted runs

`remember`, `recall_project` and `wiki_query` are offered to conversations
only. The scripted loop's catalogue is part of what a campaign measures, and a
scripted run has no person to confirm a memory.
