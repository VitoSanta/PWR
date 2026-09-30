# The desktop app

**Checked against `develop` at `0776ff4f`, 2026-09-30.** The product surface:
what it shows, how it talks to the core, and what the plan changes. The
developer guide (build, layout of the source, design system) is
[apps/desktop/README.md](../apps/desktop/README.md).

## Shape

A Tauri 2 shell with an Angular interface (zoneless, signals), in
`apps/desktop`. **It is a client of the core and nothing else**: it starts
`pwr serve --stdio`, speaks the Agent Client Protocol with PWR's `_pwr/*`
extensions ([pwr-serve.md](pwr-serve.md)), and never writes to a workspace
itself — a revert asks the core.

- **Native side** (`src-tauri/src`): finds and starts the core (`core_start`,
  `core_send`, `core_stop`), terminal tabs (`term_open`, `term_write`,
  `term_resize`, `term_close`), the engine installer (`engine_status`,
  `engine_install`, `engine_cancel`), workspace trust (`workspace_is_trusted`,
  `trust_workspace`), `default_workspace`, `chat_home_path`, `open_external`
  (http(s) only), `debug_export_chat`.
- **Bridge** (`src/app/core/bridge.ts`): the only place the interface reaches
  the native side.
- **Content Security Policy**: scripts only from the app, network only to
  Tauri's IPC, no remote images; Markdown is sanitised.

## What a person sees

- **Focus**, the only layout: the conversation in the centre, a floating tool
  bar, and a grid of workbench cards beside it.
- **Conversation**: streamed answers and reasoning; one assistant turn with
  its actions grouped by phase; three trace views — *Compact* (what PWR is
  doing, by phase, and the result), *Detailed* (reasoning, each call, files,
  commands, checks, retries), *Raw Trace* (every event and the core log).
  A reopened conversation shows a reconstructed summary of its actions,
  labelled as such (the live trace is not saved).
- **Composer**: attachments (files, folders as read-only references, images
  for models that see), a queue for messages written during a turn, *Send
  now* to steer the running turn, Stop.
- **Run controls**: Goal mode on/off; the permission mode — Protected,
  Standard, Full access (shown in warning colours) — see
  [tools-and-sandbox.md](tools-and-sandbox.md#permission-modes).
- **Permission questions**: allow once, for the session, or reject, with the
  exact command.
- **Workbench cards**: *Review* (per-file diffs, Revert one or all), *Terminal*
  (several shells as tabs), *Web preview* (the app this machine serves on
  localhost), *Files*, *Knowledge* (the project graph in 3D, with what was
  done), *Plan & checks* (Verify, Report, Diagnose), *Activity* (background
  work and the core log).
- **Top bar**: the model chip (switch model, working window, Model Manager);
  the context indicator (`Context 42% · 54k / 128k`) and its panel
  (composition, threshold, last compaction, *Compact now*); runtime metrics;
  reasoning effort.
- **Model Manager** ([models.md](models.md#the-model-manager)).
- **Chat without a workspace**: talk to a model with only the attached files;
  nothing can be edited or run.
- **Settings**: profile, memories, known projects, permissions.
- **First run**: the engine installer (about 1.2 GB), then workspace trust.

## What the review asks of it

The developer should see first: **what it is doing, which files changed, what
is left, which checks passed, why it stopped, which decision is needed**.
Token accounting, calibration confidence, context composition, backend
diagnostics and the graph are secondary (review §13).

| Change | Why | Plan |
|---|---|---|
| The post-turn note's mark follows the verdict (today `✓` even for a failure) | The mark is the first thing read | W2.2 |
| The end of a turn leads with its outcome | The list above | W7.1 |
| The 3D graph moves behind an *Experimental* switch; the Knowledge card opens on a searchable outline; the 3D libraries (`3d-force-graph`, `three-spritetext`) load only when switched on | No evidence it saves time; it costs bundle size and maintenance | W7.2 |
| Token accounting, composition, compaction parameters, calibration and backend diagnostics, the reasoning stream as the "working" signal → Advanced | Secondary information in the primary view | W7.3 |
| Every turn that ran a command unconfined says so | Full access removes every protection | W7.4 |
| Graphical defects from the manual walk, fixed with a screenshot before and after | Carried from the v0.3.0 plan | W7.5 |
| "Engine busy: …" when a person waits behind a summary or a review | One generation at a time | W5.5 |
| A gated model says it needs a token | Today a generic failure | W7.7 |

## Tests

94 unit tests in 12 spec files (the review's count, 2026-09-30), run with
`npm test -- --watch=false` in `apps/desktop`, and the production build, both
in CI (`desktop` job). No end-to-end test covers first launch, engine install,
workspace change or shutdown (plan W7.6). Manual walks are recorded in
`docs/release/` (the latest: [v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md)).
