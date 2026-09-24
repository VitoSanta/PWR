> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Tool Runtime

Tools are typed capabilities. Requests carry root-relative paths and limits; results carry status, bounded output, duration, artifact references and redaction flags. Tool output is untrusted input to the model and is redacted before it reaches a prompt.

## The surface, as implemented

| Capability | What it does | Guard |
|---|---|---|
| `ReadFile` | Reads a file, or a line window of one | Refuses binaries and anything resolving outside the root |
| `Search` | Literal string search across workspace text files | Bounded matches, redacted excerpts |
| `ListTree` | Bounded file listing | Skips symlinks and policy-excluded directories |
| `ReplaceText` | Replaces one exact, unique occurrence | Hash guard; refuses an ambiguous or absent match |
| `WriteFile` | Creates a file | Refuses to overwrite; approval gate on manifests |
| `ApplyReplace` | Rewrites a whole file | Hash guard; refuses binaries and oversized content |
| `RunCommand` | Runs one allowlisted command | Sandbox, timeout, output cap, no network without a grant |
| `FetchUrl` | Fetches one http or https URL as text | Requires the network grant; refuses other schemes and redirects |
| `Complete` | Declares the task done | Accepted only if deterministic verification then passes |

`Search` is literal and repository-scoped. There is no structural or semantic search.

`FetchUrl` is a **fetch, not a search**: there is no index and no query, so a caller must already know the address. Naming it search would promise something it does not do. Redirects are refused rather than followed, because a redirect can change scheme or host after the scheme check and would make that check advisory rather than binding. A fetched page is untrusted input in the strongest sense — a remote party wrote it — so it is bounded, redacted and hashed exactly like a file read, and it grants nothing: a page instructing the agent to run a command is prose, and the command still has to pass policy.

## Reading and editing at scale

`ReadFile` takes `first_line` and `max_lines` and reports the file's total line count, so a file larger than the output bound can be read in windows rather than being cut with nothing to say where. The `artifact_hash` it returns covers the **whole** file, not the window, so an edit guarded by it stays sound after a partial read.

Creation, partial edit and whole-file rewrite are three tools rather than one. An edit carries the hash of what it replaces; a create has nothing to hash. A single tool doing both would put a blind overwrite one missing argument away.

`ReplaceText` refuses a `find` that matches more than once. Two occurrences mean the caller may not have meant the one that would change, and choosing between them is the silent wrong edit the hash guard exists to prevent.

## Execution

`RunCommand` runs under an allowlist, in a sandbox where the platform provides one, with `env_clear` and only `PATH` restored. `HOME` and `TMPDIR` point inside the workspace, so package managers keep caches and config within the boundary instead of the boundary being widened to reach them; that also makes a run hermetic, since nothing downloaded persists into the next one and nothing in the real home is read.

Every attempt is audited before its result propagates, allowed or denied.

## Hashes and refusals

An edit is guarded by the hash of the file as it is on disk. Every result that carries such a hash reports it twice: under its own name (`new_hash` after an edit, `artifact_hash` after a read) and under `expected_hash`, which is the name of the parameter the next call must pass it as. One value under two names is redundant; one value under two names where only one of them matches the parameter is a mapping the caller has to infer, and a measured run never made that inference — it re-sent the pre-edit hash four times across two intervening re-reads.

Refusals carry what the refusal already knew. A stale hash names the hash the file now has. An edit whose `find` text is absent *and* whose `replace` text is present is reported as already applied, because "not found" is true but sends the caller round the loop again on work that is done.

### What counts as binary — 2026-09-06

The read, patch, edit and search tools all refused a file carrying a NUL byte in
its first 4096 bytes, and nothing else. A PDF has none: an ASCII header, an
object graph, and a payload that is deflate- or ASCII85-encoded. Measured on a
real 160 KiB CV in the campaign of 2026-09-06 — `read_file` returned it as text,
so the deployment received the header, some link annotations and an encoded
image stream in place of the document, took them for the file's contents, paid
for them twice over a compaction, and searched them for `/URI` because the
header looked searchable. Eight of its actions and both its compactions went to
variations of one impossible read, and it never learned the file was not text.

Detection is now by leading-byte signature as well as by NUL scan, and the
refusal names the format: *"CV.pdf is a PDF document, not text, so it cannot be
read as text. No tool in this run extracts its text; report the missing
prerequisite rather than trying another command or another window of the same
file."* Two things there are deliberate. Naming the format ends a family of
attempts rather than the single attempt that was made — the run's own failure
mode was retrying with a different executable and a different window, each time
paying an action to learn nothing. And saying that no tool in the run will
extract it turns a dead end into a prerequisite the caller can report, which is
the outcome the harness can act on.

The signature table supplements the NUL scan and does not replace it: an
unrecognised binary is still refused as "binary data". The read decides from the
first 4096 bytes before streaming, so a large binary is refused from its header
rather than hashed in full to produce a refusal that was already decidable.

### Importing a document — 2026-09-06

The refusal above closes half the problem. The document was the task, so
`extract_document` closes the other half: it recovers a PDF's text into
`<path>.txt` beside it and reports the links the document declares. The refusal
names it, so the dead end now has an exit.

A file rather than a tool result, because a CV is several thousand tokens and
the run that needed one survived two compactions: a file is read in windows,
searched, and still there afterwards. Its header carries the source path, the
source hash, the extraction method and the declared links, so a claim the run
makes about the document can be checked against the document rather than against
the model's memory of it. Extracting twice is stable and cheap; an existing
extraction that differs from a fresh one is refused rather than overwritten.

What it does not do is a named refusal in each case, never a degraded result: a
scanned page is reported as needing OCR, a CID-keyed font as encoding glyph ids
rather than characters, an unreadable file as not being a PDF. Text that reads
like text and is not is the failure this exists to prevent —
[ADR-012](adr/ADR-012-document-import.md) records the decision and what it costs.
Measured against the campaign's own CV, the output is identical to `pypdf`'s,
character for character once whitespace is normalised, and additionally recovers
seven link targets the text alone does not carry.

## Installed dependencies — 2026-09-23

`search` takes `in_dependencies: true` and searches the packages the project
declares, where they are installed on this machine: the names in
`package.json` under `node_modules`, the packages in a virtual environment's
`site-packages`, and the versions `Cargo.lock` pins, unpacked in the cargo
registry. Declared, not merely present — a package left in a cache is not
evidence about this project.

Paths come back absolute and those roots are readable, so a passage found this
way can be read with `read_file`; nothing there can be written, and the
sandbox is opened to one subpath per ecosystem rather than to three hundred
package directories. The search is bounded exactly as the workspace search is:
the same match caps, output limit and deadline, with one file scan shared
between them. Measured on this repository, 285 declared packages: 17 ms to
discover them and 4.6 s for a literal search across all of them.

The reason it exists (backlog C.12): what a local model does not know about a
library is usually already on disk, in the exact version the project builds
against. It is offline, free, and not a third-party page that might carry
instructions — so installed dependencies come first and the web later.

## The command allowlist

The allowlist is derived from the repository — the executables named by an explicit `.poorai/checks.json`, by CI configuration, or by the build systems whose markers are present — never a fixed list. Common aliases travel with what a repository declares: `python3` admits `python` and the reverse, `pytest` and `poetry` admit the interpreter they run under, `npm` admits `node` and `npx`, `flutter` admits `dart`. A project whose declared check runs `python3` denying `python` refuses the interpreter it already permits, and did cost a measured run an action.

### Several reads in one turn — 2026-09-07

One action per turn was the rule, and a turn carrying several tool calls was
rejected whole. That is right for edits and wrong for reads, and the Angular run
of 2026-09-07 measured what it costs. Twelve of that run's fourteen malformed
turns were the deployment asking for two to four files at once on a 26-file
project:

```
read_file(app.ts), read_file(app.html), read_file(app.css), read_file(app.spec.ts)
```

Every one was thrown away. The deployment fell back to one file per turn at
twenty to sixty seconds of generation each, fifty of its seventy-six actions went
to reading, thirty-six of those reads were of files the harness itself reported
as already read and unchanged, and the run died in the no-progress guard having
written no components, with 134 of 150 actions and 74 of 90 minutes unspent.

A turn may now carry up to six independent read-only actions -- `read_file`,
`search`, `list_tree`, `vcs_status`, `vcs_diff` -- and they are executed,
audited and charged individually, then answered together. What is saved is the
turn, which is where the time goes; nothing about the reads themselves changes,
and they were already confined, bounded and counted.

A turn that mixes a read with an edit, or carries two edits, is refused exactly
as before, with the same message. That refusal was right for a reason worth
keeping: taking the first of `replace_text, replace_text` performs one edit,
drops the other, and leaves the deployment believing both landed. The batch is
allowed only when every call in it is read-only, and a batch that would run past
the action budget performs what fits and names the rest rather than dropping
them silently.

### What the campaigns of 2026-09-06/07 changed here

Three runs of the same task — `qwen3.8:27b-mlx` once, `ornith-1.5:35b` twice —
built a working portfolio from a PDF and then damaged it. The repairs below come
from what the event logs actually recorded, and each one names its measurement.

**A tool the schema offers must be a tool the decoder accepts.** `apply_patch`
had been advertised since the action channel existed while the typed action's
tag said `apply_patch_hunks`, so a deployment calling the tool exactly as
offered got `unknown variant` back. Three of those in a row end a run for a
broken action channel, and that is how the first ornith run died at 1,218
seconds, with a finished and working site in its workspace. The variant now
renames to `apply_patch` and keeps `apply_patch_hunks` as a deserialisation
alias so existing logs stay readable. Nothing in the suite compared the two
lists; a test now walks every offered name through the decoder with a row of
valid arguments, so a capability added to one and not the other fails there.

**A refusal that names only the wall costs actions.** The allowlist is derived
from what a repository declares, so a workspace that is not yet a project has an
empty one and every command is refused — correctly, and with nothing to do about
it. Measured: five of fifty actions spent on `python3`, `python`, `python3.11`,
`node` and `which`, to run a verification script the deployment had written
itself, with `verifier-proposal` granted and `propose_verifier` never called.
The refusal now says why the allowlist is empty and, when the grant is present,
that `propose_verifier` is how a command joins it; without the grant it says
plainly that nothing in the run can widen it. This is the shape that already
works — the refusal naming `extract_document` sent both deployments straight to
it on their first try.

**Emptying a file is not editing it.** The fiftieth and last action of the
second ornith run replaced 11 KiB of working JavaScript with `""`. The budget
ran out immediately after, the delivered page went from every block visible to
none, and nothing downstream noticed because the file still existed.
`apply_replace` now refuses a whole-file replacement with empty content and
names `delete_path`, which is what removal is for and is refusable, recorded and
visible in a way that was not.

## Bounds that hold while output is being produced

A limit applied after the fact bounds the report, not the process. Command output, HTTP bodies and file reads were each materialised whole and truncated afterwards, so a command printing without end was bounded only by the machine. stdout and stderr are now drained incrementally with bounded retention, and the result still carries the hash of the **whole** output and a flag saying it was truncated — the caller learns that something was cut, and the hash still identifies what was produced.

A timeout kills the process group rather than the process. A child that outlives the tool it was spawned by is a process still writing to the workspace after the run stopped watching, which the hash guard on the next edit would then blame on the workspace being stale.

`FetchUrl` streams under a byte cap rather than reading a body to completion, and the provider's NDJSON reader decodes UTF-8 across chunk boundaries instead of per chunk, with a cap on a single line. Decoding per chunk corrupted any code point that spanned two of them.

## What the surface still does not have

One walker serves the index and the tools as of 2026-09-03, so `.gitignore` excludes a file from a tool result exactly as it excludes it from retrieval, and the listing is sorted rather than in directory order.

`MakeDirectory`, `DeletePath`, `MovePath`, `VcsStatus` and `VcsDiff` closed the reorganisation gap on 2026-09-03. A delete carries the hash of what it removes, as an edit does; a directory has to be named recursively and reports how much went; a move refuses an existing destination and will not follow a symlink out of the workspace. The two version-control tools are read-only by construction — no argument they take reaches a mutating subcommand.

What is still missing is a multi-hunk patch: a change touching several places in one file is several whole-file rewrites, each carrying the whole file.

The sandbox confines writing and denies the network; it does not confine **reading**. Nine known credential paths are denied and everything else on the host is legible to a sandboxed command. Under `--provision`, which grants an arbitrary executable and a network together, that is the shape of an exfiltration and the flag's help says so.

`git clean` now needs the same approval as `reset --hard`: both discard uncommitted work, and only one of them was checked.

A tool outcome is one of five — `allowed_success`, `allowed_failure`, `policy_denial`, `timeout`, `protocol_failure` — rather than allowed or not. A command that ran and exited non-zero used to be recorded exactly like one that worked, which is what made the evaluation's failure rate meaningless.

`ToolCapability` is an enum left over from an earlier design and no longer corresponds to the typed actions above. It is two vocabularies for one concept, and one of them is wrong.

Evaluation used to be the exception to all of this. Corpus materialisation and external verifiers shelled out directly, with no sandbox, timeout or output cap; they now run through `run_command` under their own bounded policy. See `security-sandboxing.md`.

## Services — 2026-09-03

`StartService` and `StopService` stand a long-running process up and take it down. `run_command` cannot: it waits for the process to exit, which a server does not do.

Ready means accepting a connection, not existing. A port is reserved from the operating system unless one is named. A start that never answers is stopped rather than left running, and every service is killed when the run ends by any route -- the supervisor's `Drop` is the mechanism, and a mutant removing it fails the fixture.
