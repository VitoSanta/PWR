# Changelog

PWR is an open-source coding agent for local models, in continuous evolution.
Statuses such as *experimental* mean what they say: usable, still changing.

## v0.2.0-alpha — unreleased

The agent is faster, finishes more of what it starts, and works in far more
languages; the desktop app gains a workbench, project knowledge and a chat
mode. 125 commits since v0.1.2-alpha.

### Highlights

- **No more minutes of waiting after each message.** Background wiki
  summaries used the engine between your messages and evicted the
  conversation from its prompt cache, so every follow-up recomputed the whole
  conversation (up to four minutes at 70k tokens). They now run on a cache of
  their own; the prompt cache is kept across messages and copied only where
  needed.
- **Many more stacks work end to end on a Mac.** Toolchains a project needs
  are installed inside the workspace and put on `PATH` for the agent and the
  checks alike, with instructions for Go, Node, Java, Maven, Gradle, .NET,
  CMake, PHP, Composer, Dart, Flutter, Deno, Rust, Terraform and Elixir. Swift
  packages, the JVM (Maven, Gradle, Kotlin) and .NET now run inside the
  sandbox.
- **Goal mode checks its own work before finishing.** It never reports done
  while an acceptance check fails; when the checks pass, the work is read
  against the request rule by rule, and a second reader that has not seen the
  conversation marks the rules the code does not meet.
- **Measured.** On 31 real tasks across languages, frameworks, databases and
  tools, verified outside PWR in each task's official container with hidden
  tests, tasks completed rose from 19 to 22 between two campaigns of this
  cycle, one run each (see `evidence/stack-matrix/`).

### Agent

- A tool call the model writes badly is read when it has one reading
  (a parameter closed by its own name, a list missing its closing bracket)
  and otherwise re-asked with the exact format -- never taken as the answer.
- A command stopped at its time limit reports the end of what it printed, so
  a hanging test can be found; a dependency install that stalls offline asks
  for the network.
- A command line sent as the program is split into program and arguments.
- Replies that go round in circles are stopped and pointed at the
  documentation; an action that keeps giving the same result is named.
- `fetch_url` never returns a file as text, can download to a path, and saves
  nothing from an error answer.
- A command's output is capped at 16 KiB per stream (head and tail kept);
  files are not.
- Files the turn itself wrote or read can be deleted without resending their
  hash.
- `look_at` *(experimental)*: a model that reads images can take a screenshot
  of a local page or file it built.

### Permissions and sandbox

- A program outside the workspace's list, the network and Docker are
  questions for you, not refusals; each question shows the command, and the
  script a shell would run.
- What you allow for the session holds for the rest of a goal and for the
  checks it runs.
- Nothing the agent writes runs later outside the sandbox (`.git/config` is
  kept clean of commands git would run).
- JVM tools and Swift use the workspace for their home, caches and temporary
  files; the per-user temporary directory opens only to workspaces that run
  Apple's toolchain. See `SECURITY.md`.

### Desktop app

- **Workbench**: cards for files, the knowledge graph, a terminal and a local
  preview, following the theme.
- **Knowledge**: a wiki and graph per workspace, with background summaries,
  and projects recalled from any conversation -- found however you write
  their name.
- **Chat mode**: talk without a workspace, reading only what you attach.
- Conversations read as phases that open onto their steps; a message's
  actions sit under it. Copy, edit the queued message, rewind.
- The context window is followed live during a turn.
- Settings redesigned as pages; an outdated core is explained.
- Model Manager: parameter range, sorting across the whole Hub, believable
  sizes, pages that do not come back empty, fewer Hub requests.
- One grid, one button order, one motion system; six layout variants
  *(experimental, one to be chosen before release)*.
- Your name is used once in a conversation, not in every reply.

### Fixes

- Path escapes, rewind data loss and terminal leaks found by an audit.
- Temporary folders are no longer registered as projects.
- The engine installer is offered when the configured Python lacks `mlx-lm`.

### Evidence

- `evidence/stack-matrix/`: 37 tasks across languages, frameworks, databases
  and tools, run through the desktop app's protocol and verified
  independently; a static site renders each run with its conversation.

## v0.1.2-alpha — 2026-09-24

First public alpha: the macOS desktop app with PWR's own MLX engine, Hugging
Face model downloads into `~/.pwr/models`, the agent loop with goal mode,
the macOS sandbox and permission modes.
