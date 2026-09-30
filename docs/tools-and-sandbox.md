# Tools, policy and the sandbox

**Checked against `develop` at `0776ff4f`, 2026-09-30.** What the agent can do,
what bounds it, and where the bounds end. The operational summary for users is
[SECURITY.md](../SECURITY.md); this page is the engineering account.

## Working-tree protection update — 2026-09-30

Commands and file tools share frozen acceptance paths and installed-dependency
protection. Parent deletion/renaming and workspace symlink aliases are guarded;
on macOS, ancestor unlink restrictions prevent moving a protected subtree while
allowing unrelated sibling writes. Verifiers use the same protection policy.
Unsupported command confinement is reported as partially enforced; Full access
is explicitly unconfined. Acceptance hashes detect changed evidence even there,
both before and after checks. This is not a claim of cross-platform sandbox parity.

PDF decoding is limited to 8 MiB per stream and 64 MiB per document. Exceeding
those limits fails extraction instead of retrying another decoder unboundedly.

## The tool catalogue

Defined once, in `action_tool_catalog` (`crates/pwr-orchestrator/src/lib.rs:4560`).
The scripted loop offers all of it; a conversation removes two and adds four
(`chat_tool_catalog`, `crates/pwr-orchestrator/src/converse.rs:622`).

| Tool | What it does | Conversation | Scripted |
|---|---|---|---|
| `read_file` | Read a text file, or a window (`first_line`, `max_lines`); a long Markdown file read whole returns its outline | ✓ | ✓ |
| `list_tree` | List files, optionally under one folder | ✓ | ✓ |
| `search` | Search text files; with `in_dependencies`, the installed dependencies (`node_modules`, `site-packages`, Cargo sources) | ✓ | ✓ |
| `find_definition` | Where an identifier is declared | ✓ | ✓ |
| `extract_document` | Recover a PDF's text into `<path>.txt` | ✓ | ✓ |
| `replace_text` | Replace one exact, unique occurrence | ✓ | ✓ |
| `apply_patch` | Several replacements in one file under one hash guard | ✓ | ✓ |
| `apply_replace` | Replace a whole file, given the hash from a prior read | ✓ | ✓ |
| `write_file` | Create a file. In a conversation, onto an existing file it becomes a whole replacement (`own_overwrite`, see [Edits](#edits)) | ✓ | ✓ (refuses an existing file) |
| `delete_path`, `move_path`, `make_directory` | File-system changes inside the workspace; delete needs the file's hash | ✓ | ✓ |
| `restore_file` | Put a file back as it was when first read or changed in this run | ✓ | ✓ |
| `run_command` | Run one program with an argument list; no shell; `outside_sandbox` asks to run it unconfined | ✓ | ✓ |
| `start_service`, `stop_service` | A supervised long-running process, ready when it accepts a connection | ✓ | ✓ |
| `fetch_url` | Fetch one http(s) URL, or save it to a workspace path | ✓ | ✓ |
| `vcs_status`, `vcs_diff` | What Git says changed | ✓ | ✓ |
| `complete` | Declare the task done | ✓ | ✓ |
| `decline` | Refuse the task, saying why | ✓ | ✓ |
| `record_progress` | Record a finished plan step | — | ✓ |
| `propose_verifier` | Offer a check for a workspace that declares none | — | ✓ |
| `remember` | Propose a memory; the person saves it | ✓ | — |
| `recall_project` | Another project's wiki overview and log | ✓ | — |
| `wiki_query` | Ask the project graph | ✓ | — |
| `look_at` | Screenshot a local page or workspace HTML file (**experimental**, vision models only) | ✓ | — |

Chat without a workspace offers only `read_file`, `list_tree`, `remember`,
`recall_project` and `wiki_query`, on attached files.

`complete`'s description tells the model *"Accepted only if deterministic
verification then passes"*, which is true of Goal mode with an acceptance
check and not of a conversation turn or a scripted run without a verifier
(plan W3.4).

## Execution

- **No shell.** `run_command` takes an executable and an argument list; no
  shell ever interprets them. A whole command line sent as the executable with
  no arguments, in plain words, is split into program and arguments while the
  call is decoded (`crates/pwr-orchestrator/src/lib.rs:5470-5500`); one that
  contains anything a shell would read differently (quotes, `$`, pipes,
  globs, redirections) is refused with the correct form
  (`crates/pwr-tools/src/lib.rs:6524`).
- **Environment.** Cleared; `HOME` and `TMPDIR` point inside the workspace
  (`.pwr-scratch`); JVMs get `JAVA_TOOL_OPTIONS` (`user.home`,
  `java.io.tmpdir`), Gradle `GRADLE_USER_HOME`.
- **Bounds.** Output bounded per result (64 KiB by default,
  `ToolPolicy::output_limit`), a timeout, process groups killed on stop.
  Output past the bound is not kept (plan W4.4).
- **Toolchains** a task needs are installed under `.toolchains/<name>/`, first
  on `PATH` for commands and checks alike; the host is not modified.
- **Services** are owned by a supervisor and killed when it goes.
- **Programs.** A program the workspace does not declare is asked about
  (`toolchain_install`) rather than refused.
- **Audit.** Every attempt — allowed or denied — is recorded in the event log.

## Edits

Every edit is guarded by the file's content hash: `apply_replace`,
`apply_patch`, `replace_text` and `delete_path` refuse a file whose hash is
not the expected one, and the refusal names the current hash. Writes into
installed dependencies are refused without `dependency_change`; paths in
`.pwr/protected.json` are refused always (`refuse_if_protected`,
`crates/pwr-tools/src/lib.rs:1984`), as are `.pwr/` and `.git/hooks`,
`.git/config`.

**A rewrite is of the version the model read.** In a conversation, `write_file`
onto an existing file becomes `apply_replace` (`own_overwrite`,
`crates/pwr-orchestrator/src/converse.rs`) with the hash the conversation *last
saw* for that path — from a `read_file` (any window), or from the edit or
creation that left the file as it is — kept across turns
(`Continuity::known`) and, after a restart, seeded from the checkpoint's
changed files. It is refused, without running, when:

- the file exists and the conversation has not read it ("read it with
  `read_file` first, or change part of it with `replace_text`/`apply_patch`");
- the file no longer holds what the conversation last saw — the person edited
  it, or a command changed it ("read it again").

Neither refusal names the file's current hash, which would let a caller send
it back without reading anything. The refusal is audited as a denied
`write_file`. A file that does not exist is created as before. Paths are
compared after normalising `./` and `//`. (Before 2026-09-30 the hash was read
from the file at write time, so the check compared the file with itself.)
`apply_replace` called directly still refuses a stale hash and names the
current one, as it always has.

**Writes are whole or absent.** Every file-writing tool — `write_file`,
`apply_patch`, `replace_text`, `apply_replace`, `restore_file`, the text
`extract_document` writes, and the core's Revert and rewind — goes through
`pwr_tools::atomic::write_atomic` (`crates/pwr-tools/src/atomic.rs`): the new
bytes are written to a temporary file beside the target (`.pwr-tmp-<pid>-<n>-<name>`),
flushed, given the target's permissions, and moved over it with `rename`, then
the folder is flushed. Immediately before the move the target is read again and
must still hash to what the tool checked; otherwise nothing is written and the
model is told to read it again. A new file is linked into place, which fails if
the name was taken meanwhile. A crash or a disk error leaves the original
intact; a read-only file is still refused; an executable stays executable.
Temporary names are never indexed, listed or searched.

What remains: the comparison and the `rename` are two system calls, so a
change landing between them (microseconds, not the seconds between a tool's
read and its write) is still lost; and no rollback of a completed edit is
promised.

## Permission modes

Per workspace, chosen in the app's Run controls (`permission_mode` in
`.pwr/chat-config.json`; `PermissionMode`, `crates/pwr-cli/src/main.rs:1096`):

| Mode | Stored as | Sandbox | Asks before |
|---|---|---|---|
| **Protected** (default) | `ask` | yes; network closed unless granted | dependency changes, network, programs the workspace does not list, rewriting history, publishing — plus, always, the container engine, running outside the sandbox and reaching outside the workspace |
| **Standard** | `auto` | yes | publishing, rewriting history, the container engine, running outside the sandbox, reaching outside the workspace |
| **Full access** | `full` | **no** — the person's own `HOME`, caches and `TMPDIR` | nothing |

In Protected mode, local services and adopting a proposed check are granted.
Each question is answered *once*, *for the session* or *no*; session answers
also apply to the checks that close a turn or a goal.

Scripted runs (`pwr run`) grant only what `--approve` names:
`dependency-change`, `history-rewrite`, `publish`, `network-access`,
`local-service`, `toolchain-install`, `container-engine`,
`verifier-proposal`, `outside-sandbox`, `outside-workspace`.

## The sandbox (macOS Seatbelt)

`sandbox_profile` (`crates/pwr-tools/src/lib.rs:2092-2375`) builds a profile per
command:

- **Writes** confined to the workspace, except `.pwr/` and `.git/hooks`
  (denied), plus the few host paths toolchains need: `/private/tmp/.dotnet`,
  `/private/tmp/pwr-look` (the `look_at` browser), MSBuild's node sockets
  (`/private/tmp/MSBuild<pid>`), and for Apple toolchains the per-user
  temporary folder.
- **Reads** denied outside the workspace except system paths a process needs
  and the toolchain directories the project names; `.pwr/` files are
  unreadable (their directories stay listable).
- **Never readable**, whatever is granted: `~/.ssh`, `~/.aws`, `~/.gnupg`,
  `~/.config/gh`, `~/.config/gcloud`, `~/.kube`, `~/.docker/config.json`,
  `~/.netrc`, `~/Library/Keychains`.
- **Network** denied unless granted; with `local_service` only, binding and
  connecting to `localhost` — which Seatbelt cannot narrow to loopback, so it
  covers every local address.
- **Git configuration**: writable, and anything a command adds that Git would
  execute is removed afterwards (`GitConfigGuard`).

A command whose profile cannot be built (no Seatbelt, a root with a quote in
it) is **refused**, unless `PWR_ALLOW_UNCONFINED=1`; then it runs unconfined
and is recorded `sandboxed: false`.

### Where the boundary ends

| Limit | Why | Plan |
|---|---|---|
| **Protected paths and dependency trees are not in the profile.** A command (a script, an interpreter, a build) can change a file the edit tools refuse, and so can the checks | `refuse_if_protected` is a tool check; the profile knows only `.pwr` and hooks | W1.3 |
| **Full access has no sandbox at all.** `.pwr/`, hooks, protected paths, dependencies and the host are writable by any command; checks run unconfined | `sandbox_for`, `main.rs:1119` | W1.3, W1.9, W7.4 |
| **The container engine acts outside the sandbox.** The grant opens only the engine's socket, but the daemon then runs containers with the network and the folders it shares (all of `HOME` on Docker Desktop) | By design of the grant | W1.9 (disclosure) |
| **`localhost` is every local address** | Seatbelt accepts only `*` or `localhost` | W1.9 |
| **A network grant is not per destination** | No egress filtering exists | W1.9 |
| **argv is not a code boundary** | Python, Node, build scripts and package lifecycle scripts run arbitrary code within the sandbox | W1.9 |
| **`--provision` grants any program and the network together** | Installing a toolchain needs both; it can still read system and toolchain paths | Documented in the flag's help |
| **Unbounded PDF decompression** | `inflate`, `document.rs:232` | W1.6 |
| **macOS only** | No adapter elsewhere; commands are refused there | Later |

## Known defects

| Defect | Plan |
|---|---|
| Overwrite bound to the current hash, not the read version | W1.1 |
| Protections missing from the command sandbox; none in Full access | W1.3 |
| `complete` promises verification the loop does not always perform | W3.4 |
| Unbounded PDF inflation | W1.6 |
| Grants do not say what they open, at the point of decision | W1.9 |
