# Changelog

PWR is an open-source coding agent for local models, in continuous evolution.
Statuses such as *experimental* mean what they say: usable, still changing.

## Unreleased (`develop`)

Development changes through 2026-10-07. These changes are not in the
published v0.2.0-alpha DMG. Dated checks are recorded in
[Mac verification](docs/release/v0.2.x-mac-verification.md) and the
[experiment log](docs/experiment-log.md); they are not evidence for a new
candidate. See [next-release readiness](docs/release/next-release-readiness.md).

### Direction and documentation

- A technical review of 2026-09-30 was checked claim by claim against the
  code and adopted: PWR is positioned as a dependable local coding agent for
  bounded repository changes, and the claim that its harness makes small
  models better is to be tested once, on the app's own path, before more is
  built on it ([MASTER_SPEC](MASTER_SPEC.md), [plan](docs/plan/implementation-plan.md)).
- The documentation was rewritten from the code. Earlier documents are kept
  in [Git history](https://github.com/VitoSanta/PWR/tree/309266d5/docs/archive). Windows moves after the decisive comparison.

### Agent

- A whole-file rewrite (`write_file` onto an existing file) is checked against
  the version the conversation last read or wrote, never against the file as it
  is at write time. A file it has not read, or that a person or a command
  changed since, is refused with an instruction to read it, and the file is
  left as it was (plan W1.1).
- File writes are atomic: a temporary file, then a move, with the target
  re-checked just before it. A crash leaves the original whole, and a change
  made while an edit was being prepared is refused instead of overwritten
  (plan W1.2).
- Gemma 4 and gpt-oss calibrate and run in agent mode (tool-call adapters,
  engine diagnostics kept off the protocol, a stop at the model's closing
  marker; calibration subsequently revised to suite 6).
- For every model: unresolved imports answered with the install to run; edit
  results show the changed lines; `complete` over unread results, or before
  anything was done, is held once; a command that failed the same way twice is
  not run again; five failed runs after edits → hand over what is ready;
  more one-reading repairs of malformed calls.
- Ornith-1.5-9B uses its vendor's temperature.

### Effects and verification (unreleased)

- **Commands cannot change what the edit tools refuse**: frozen acceptance
  files, installed dependencies and their parents are denied inside the
  sandbox, renames and symlink aliases included; Full access remains
  unconfined and says so in every turn's outcome (plan W1.3, W1.9, W7.4).
- **A goal is bounded on every path** — actions, refused completions,
  verifications, review rounds and a wall-clock deadline (plan W1.4) — and its
  progress is judged by the *identity of a failure*, not the name of a check
  (plan W1.5). Zero-test runs are named for cargo, pytest, Jest, Vitest, Go and
  .NET.
- **Acceptance artifacts are frozen**: the files an acceptance check runs are
  hashed before the goal starts, and a changed one stops it even in Full access;
  you can authorize a single file (plan W3.1).
- **One structured outcome** per turn (delivery, checks, baseline, acceptance,
  confinement, budgets) reaches the app; the ✓ follows what the checks said.
- The app, the terminal console and Goal mode run **one session executor**
  (plan W2.3). The review round — the work read against the request, rule by
  rule — now also runs when the checks pass and no acceptance check is
  declared, the usual case (the goal then still ends *not verified*).
- The objective and its revisions are never shortened by compaction; if they do
  not fit, the turn stops and says so (plan W4.1). PDF decompression, the
  repository walk and the embedding sidecar are bounded; the event log is
  transactional.

### Models and the engine

- **Two more families read and two fixed**, so models that were wrongly refused as
  "Limited" now work: Mistral/Devstral and Liquid LFM2 (new adapters), Qwen3-Coder
  (its calls arrive without their opening tag, and with Python-style lists) and
  Granite 4.1.
  Gemma 4's template got the call ids it names results by. All 22 models
  installed on the maintainer's Mac pass Quick Calibration (`quick-calibration-6`),
  which now asks whether a model can *use* tools, not which one it reaches for
  first; a failure measured through an adapter that later changed is void.
- **Sampling is never greedy by default**: a model with no declared sampling
  uses 0.6 / 0.95 / 20. Model-card recommendations are scoped conservatively:
  an unknown active mode does not select an assumed thinking/coding recipe.
  Each effective value says where it came from.
- **Long conversations**: compaction at 75% of the granted context window, with
  no default token ceiling (the temporary 32k ceiling was withdrawn); a clean
  start after two replies in a row that fall apart; a
  presence penalty that can actually see a repeated passage after a reply
  looped; a note when one file has been rewritten twelve times; the action limit
  per turn is 100 (it was 26) and `actions_per_turn` changes it.
- **MLX 0.32.3** (was 0.32.0): attention and quantized-matmul fixes; Gemma 4
  prefill about 8–10 % faster in the recorded local check, not a universal
  speed claim. The managed-engine marker is checked against current package
  pins; obsolete managed installs offer reinstallation.
- **mlx-vlm 0.7.2** (was 0.6.17): support for loading the documented 1-bit
  artifact path. The MLX free-buffer cache defaults to 2 GiB, configurable by
  `PWR_MLX_CACHE_GB`, and is cleared after each completed generation. This is
  not a total-memory or KV-cache cap; long-run effectiveness is unmeasured.
- The MLX sidecar applies the macOS GPU context-switch timeout workaround
  before importing MLX (`AGX_RELAX_CDM_CTXSTORE_TIMEOUT=1`, explicit overrides
  preserved). Non-fused prefill targets one second and can shrink below the
  former 256-token floor. GPU errors include pre-operation memory/phase
  diagnostics; optional tracing records per-chunk timings. This mitigates
  `Impacting Interactivity`, rather than treating it as proof of exhausted RAM.
- The app shows how far the engine has read the prompt ("Reading the
  conversation · 37 %") instead of looking stuck.

### Goal support (experimental)

- Verified proposals run as an executor phase, using ordinary edit policy and
  restoring proposals that do not improve the verification verdict. Enabled
  by profile only for Ornith 1.5 9B; recorded results are mixed across models.
- Optional model-work budget (`goal_budget.work`) and off-by-default
  `goal_aids` switches: failure pointers, ten-tool catalogue, SEARCH/REPLACE
  proposals, persistent plan and paced reasoning. Benefits remain unmeasured.
- A repeated-repair notice quotes the actual failing lines after three
  file-change/failed-command cycles; advisory, not a proven loop recovery.

### Sandbox and permissions

- Three permission modes: Protected, Standard and **Full access** (no
  sandbox). A sandboxed command can ask to run once outside the sandbox.
- .NET installed per user, MSBuild's node sockets, and `look_at` on GitHub's
  runners work in the sandbox.

### Desktop

- Focus tools laid out as a grid; the Terminal holds several shells as tabs.
- Scrolling holds its place while an answer streams and across trace views;
  top-bar panels close when the window loses focus.
- A reopened conversation shows a reconstructed summary of its actions and
  each turn's own model.

- Terminal analysis can read recent user-terminal output with a dedicated
  permission. Local page checks return rendered text/console and, for vision
  models, an image. Page tools support up to 12 click/type/press steps, option
  selection and scrolling; their screenshot is shown in the conversation.
  These are local DOM interactions with framing/trusted-event limitations.
- Actions use plain descriptions, long messages fold, and turn timestamps
  are visible. Failed commands are distinguished from policy refusals.

### Evaluation

- `pwr eval run --reasoning-effort` bounds reasoning as the app does (Medium by
  default; `off` is the old behaviour and must be declared as the treatment).
- `pwr eval compare` is strict by default.
- The product-path runner records provenance, holds an exclusive engine lease,
  freezes task splits and supports a same-engine minimal control and paired
  analysis. No confirmatory superiority result is implied.
- Check discovery reaches nested projects; additional zero-test and
  pass-without-tests signatures keep absent evidence explicit.
- New corpus `corpus/small-apps-v1.json`.
- The stack-matrix runner drives the app's own protocol with hidden tests, and
  `runner/watch.py` follows a run token by token.

## v0.2.0-alpha — 2026-09-28

The agent has improvements to speed, completion handling and toolchain coverage;
the desktop app gains a workbench and project knowledge. Chat without a
workspace was already present in v0.1.2-alpha and has been extended.

### Highlights

- **Prompt cache survives background work.** Background wiki
  summaries used the engine between your messages and evicted the
  conversation from its prompt cache, so every follow-up recomputed the whole
  conversation (up to four minutes at 70k tokens). They now run on a cache of
  their own; the prompt cache is kept across messages and copied only where
  needed.
- **Broader toolchain support on a Mac.** Toolchains a project needs
  are installed inside the workspace and put on `PATH` for the agent and the
  checks alike, with instructions for Go, Node, Java, Maven, Gradle, .NET,
  CMake, PHP, Composer, Dart, Flutter, Deno, Rust, Terraform and Elixir. Swift
  packages, the JVM (Maven, Gradle, Kotlin) and .NET now run inside the
  sandbox.
- **Goal mode checks its own work before finishing.** It never reports done
  while an acceptance check fails; when the checks pass, the work is read
  against the request rule by rule, and a second reader that has not seen the
  conversation marks the rules the code does not meet.
- **Measured within this development cycle.** On 31 tasks across languages,
  frameworks, databases and tools, verified outside PWR with hidden tests,
  tasks completed rose from 19 to 22 between two campaigns, one run per task.
  A later, reduced c4 run passed 7 of 9 tasks, including 5 of 6 held-out
  tasks not used to tune PWR. These are single runs on the pinned `d313acd9`
  binary, not a paired v0.1.2-alpha comparison or a result for all 37 tasks
  in the corpus (see `evidence/stack-matrix/`).

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
- A run can be shown **Compact**, **Detailed** or as a **Raw Trace** (every
  event with its payload, and the core's log for the run). The choice is
  remembered and changes only the view, never the model's work.
- The context window is followed live during a turn.
- Settings redesigned as pages; an outdated core is explained.
- Model Manager: parameter range, sorting across the whole Hub, believable
  sizes, pages that do not come back empty, fewer Hub requests. Its model
  profile also shows the MLX sampling parameters and their sources, with
  editable user overrides.
- **Focus** is the sole desktop layout, chosen after the six-layout experiment.
  It has a centred conversation, a compact composer and resizable tool cards.
  Colour themes can change without switching layouts. On macOS, the full-screen
  title bar draws over the app.
- Your name is used once in a conversation, not in every reply.

### Fixes

- Path escapes, rewind data loss and terminal leaks found by an audit.
- Temporary folders are no longer registered as projects.
- The engine installer is offered when the configured Python lacks `mlx-lm`.

### Evidence

- `evidence/stack-matrix/`: a 37-task corpus and runner using the desktop
  app's protocol, independent verification and a static result viewer.
  The reduced c4 run has nine recorded task verdicts; the full corpus was
  not run in c4.

## v0.1.2-alpha — 2026-09-24

First public alpha: the macOS desktop app with PWR's own MLX engine, Hugging
Face model downloads into `~/.pwr/models`, the agent loop with goal mode,
the macOS sandbox and permission modes.
