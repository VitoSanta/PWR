# Security

PWR runs a language model's output as commands against a repository on your
machine. That is its purpose and its risk, so this document says plainly where
the boundary is and where it ends. **Checked against `develop` at `0776ff4f`,
2026-09-30.** The engineering detail is in
[docs/tools-and-sandbox.md](docs/tools-and-sandbox.md).

**It is an alpha. Use it on repositories you can recover (committed, or
backed up), watch what it does, and do not run it unattended on code you did
not write.**

## Permission modes

Per workspace, in the app's Run controls (`permission_mode` in
`.pwr/chat-config.json`):

| Mode | Sandbox | Asks before |
|---|---|---|
| **Protected** (default) | yes; network closed unless you allow it | changing dependencies, reaching the network, running a program the workspace does not list, rewriting Git history, publishing; and always before using the container engine, running a command outside the sandbox, or reaching a folder outside the workspace |
| **Standard** | yes | publishing, rewriting history, the container engine, running outside the sandbox, reaching outside the workspace |
| **Full access** | **none** | nothing |

Each question shows the exact command and is answered *once*, *for the
session* or *no*. What you allow for the session also applies to the checks
that close a turn or a goal. Scripted runs (`pwr run`) grant only what
`--approve` names.

**Full access removes every protection below**: commands run as they would in
your terminal, with your home folder, credentials, caches and network, and the
checks run the same way. Choose it only for work you are watching.

## What the sandbox enforces (Protected and Standard)

On macOS, every command runs under a Seatbelt profile:

- **Writes** are confined to the workspace, except PWR's own state (`.pwr/`)
  and Git hooks, which are denied, and a few host paths toolchains need
  (`/private/tmp/.dotnet`, `/private/tmp/pwr-look` for `look_at`'s browser,
  MSBuild's node sockets, and for Swift/Xcode projects the per-user temporary
  folder).
- **Reads** are denied outside the workspace except the system paths a
  process needs and the toolchain folders the project names.
- **Never readable**, whatever you allow: `~/.ssh`, `~/.aws`, `~/.gnupg`,
  `~/.config/gh`, `~/.config/gcloud`, `~/.kube`, `~/.docker/config.json`,
  `~/.netrc`, `~/Library/Keychains`.
- **Network** denied unless you allow it.
- **`HOME` and `TMPDIR`** point inside the workspace; toolchains a task needs
  are installed under `.toolchains/` in it; the host is not modified.
- **Git configuration** stays writable, and anything a command adds that Git
  would execute is removed afterwards.

A command that cannot be confined (no sandbox on the platform, or a workspace
path the profile cannot express) is **refused**, unless you set
`PWR_ALLOW_UNCONFINED=1`; then it runs with your rights and is recorded
`sandboxed: false`.

The file tools additionally refuse: writes into installed dependencies
(`node_modules`, `site-packages`, `vendor`) unless you allow a dependency
change; paths listed in `.pwr/protected.json`; `.pwr/`, `.git/hooks`,
`.git/config`; and any edit whose expected file hash does not match.

In the app: a Content Security Policy (scripts only from the app, no remote
images, network only to the core's IPC); links open in your browser, http(s)
only; the app never writes to a workspace itself — Revert goes through the
core, which refuses a file you changed since. Every tool attempt, allowed or
denied, is recorded in `.pwr/state.sqlite`; `pwr report --format jsonl <run>`
verifies its hash chain.

## Where the boundary ends

These are known limits of the current design. Each has a plan item; until it
lands, **do not rely on the protection it names**.

- **Protected paths and installed dependencies are protected from the edit
  tools, not from commands.** A script, an interpreter or a build the agent
  runs — and the checks — can change a file the edit tools would refuse
  ([plan W1.3](docs/plan/implementation-plan.md#w13-the-same-protections-for-commands-as-for-file-tools)).
- **A change landing in the instant between a tool's last check and its
  write is still lost.** File writes are atomic (a temporary file, then a move)
  and the target is re-checked just before the move, so a crash leaves the
  original whole and a change made while the edit was being prepared is
  refused; the two system calls at the end cannot be made one.
- **"Verified" can follow a weakened test.** Goal mode freezes
  `.pwr/checks.json`, not the tests it runs
  ([W3.1](docs/plan/implementation-plan.md#w31-freeze-what-decides-acceptance)).
- **The container engine is outside the sandbox.** Allowing it opens only the
  engine's socket, but the daemon then runs containers with the network and
  with the folders it shares — on Docker Desktop, your whole home folder.
- **"Local services" means every local address.** macOS's sandbox can name
  only `localhost`, which covers your LAN interfaces too.
- **Network access is not per destination.** Once allowed, a command can
  reach any host.
- **Code runs.** No shell is used, but Python, Node, build scripts and package
  install scripts run arbitrary code inside the sandbox.
- **`--provision`** (command line) grants any program and the network
  together, to install toolchains.
- **A repository is untrusted input.** A file that tells the agent to fetch
  and run something is text; the command still has to pass policy. The test
  corpus contains such a file deliberately.
- **PDF extraction** decompresses without a size bound
  ([W1.6](docs/plan/implementation-plan.md#w16-bound-pdf-decompression)).
- **macOS only.** There is no sandbox adapter elsewhere; commands are refused
  there unless `PWR_ALLOW_UNCONFINED=1`.
- **The log is verifiable, not tamper-proof**: it detects an altered event,
  not a rewritten chain or a removed ending.

## Models and privacy

The model runs on this machine. The MLX engine is a child process of the core
with no network endpoint; the llama.cpp engine (command line only) listens on
`127.0.0.1` only. Model folders are data: templates render in a sandboxed Jinja
environment, `trust_remote_code` is off, the Hub is offline for the engine, and
PWR never runs `.py` files from a model repository. Downloads require a size
and a checksum for every file and never overwrite an existing different file.
The network is used when you browse or download models, once to install the
engine, and by the agent only when you allow it.

## Reporting something

Open an issue with what you observed and how to reproduce it. If a tool can
write, read or reach outside the boundary described above, say so in the
title so it is looked at first. There is no embargo process on an alpha.
