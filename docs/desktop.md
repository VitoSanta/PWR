# The desktop app

**Checked against `develop` at `bff93062`, 2026-10-01.** The product surface:
what it shows, how it talks to the core, and what the plan changes (the layout is being reworked on a separate branch, `codex/agent-layout-lab`, not merged). The
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

- **Focus workspace**: restored to the layout before the editorial redesign.
  The conversation fills the window; the top bar opens conversations, model
  controls and tools. Tools can sit beside the conversation or fill a narrow
  window. Existing colour palettes are retained; text uses the system sans
  and code uses the system monospace font.
- **Conversation**: streamed answers and reasoning; while the engine reads a long prompt the working line says *Reading the conversation · 37 % (12,288 of 33,000 tokens)* (from `_pwr/model_progress`, 2026-10-01); one assistant turn with
  its actions grouped by phase; three trace views — *Compact* (what PWR is
  doing, by phase, and the result), *Detailed* (reasoning, each call, files,
  commands, checks, retries), *Raw Trace* (every event and the core log).
  A reopened conversation shows a reconstructed summary of its actions,
  labelled as such (the live trace is not saved).
- **Composer**: attachments (files, folders as read-only references, images
  for models that see), a queue for messages written during a turn, *Send
  now* to steer the running turn, Stop. Goal mode, permissions, the model
  picker and context usage sit beside the message field.
- **Run controls**: Goal mode on/off; the permission mode — Protected,
  Standard, Full access (shown in warning colours) — see
  [tools-and-sandbox.md](tools-and-sandbox.md#permission-modes).
- **Permission questions**: allow once, for the session, or reject, with the
  exact command.
- **Workbench cards**: Review, Terminal, Web preview, Files, Knowledge,
  Plan & checks, and Activity. Cards can be resized, moved and maximised.
  **Analyze terminal** and **Analyze page** append a diagnostic request to the
  composer without replacing its draft. Send the request to ask the model to
  use `read_terminal` (with permission), `check_page` (rendered text and console)
  or `look_at` (also a screenshot, for vision models). The browser check loads
  the URL separately from the embedded preview; it does not share its login
  state. On narrow windows these buttons return focus to the conversation.
- **Conversation controls**: Compact, Detailed and Raw Trace plus runtime
  metrics in the title row. Finished compact turns fold their work under
  an action summary; file-change notes open the Changes tab.
- **Model and context panels**: model switching, working window and reasoning
  effort; context composition, threshold, last compaction and *Compact now*.
- **Sampling** (in the Model Manager): each value shows where it comes from — your profile, the model card, `generation_config.json`, a PWR profile, or *PWR default (nothing declared)* — and a bare 0 reads *greedy* or *off*.
- **Model Manager** ([models.md](models.md#the-model-manager)).
- **Chat without a workspace**: talk to a model with only the attached files;
  nothing can be edited or run.
- **Settings**: Profile, Memory, Projects, Workspace, Appearance and Shortcuts.
  Appearance remembers the accent, serif headings, chat size and code font;
  alternate colour palettes remain available for each scheme.
- **First run**: the engine installer (about 1.2 GB), then workspace trust.

## What the review asks of it

The developer should see first: **what it is doing, which files changed, what
is left, which checks passed, why it stopped, which decision is needed**.
Token accounting, calibration confidence, context composition, backend
diagnostics and the graph are secondary (review §13).

| Change | Why | Plan | State |
|---|---|---|---|
| The post-turn note's mark follows the verdict | The mark is the first thing read | W2.2 | done on `develop` |
| The end of a turn leads with its outcome | The list above | W7.1 | partial: the typed outcome reaches the app |
| The 3D graph sits behind an *Experimental* switch; the Knowledge card opens on a searchable outline; the 3D libraries load only when switched on | No evidence it saves time; it costs bundle size and maintenance | W7.2 | done on `develop`; native walk pending |
| Token accounting, composition, compaction parameters, calibration and backend diagnostics, the reasoning stream as the "working" signal → Advanced | Secondary information in the primary view | W7.3 | open |
| Every turn that ran a command unconfined says so | Full access removes every protection | W7.4 | done on `develop` |
| Graphical defects from the manual walk, fixed with a screenshot before and after | Carried from the v0.3.0 plan | W7.5 | ongoing |
| "Engine busy: …" when a person waits behind a summary or a review | One generation at a time | W5.5 | open |
| A gated model says it needs a token | Today a generic failure | W7.7 | open |
| A warning before switching model in a long conversation (the new engine reads the whole prompt again: minutes) | Measured 2026-09-30, 32 minutes | — | open; only the progress line exists |
| Feedback while a model switch waits for the engine (it queues behind a stopped generation; the picker looks dead) | Measured 2026-10-01 | W5.5 | open |
| A setting for the per-turn action limit (`actions_per_turn`) | Today only in `.pwr/chat-config.json` | — | open |

## Tests

107 unit tests in 14 spec files (2026-10-02), run with
`npm test -- --watch=false` in `apps/desktop`, and the production build, both
in CI (`desktop` job). No end-to-end test covers first launch, engine install,
workspace change or shutdown (plan W7.6). Manual walks are recorded in
`docs/release/` (the latest: [v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md)).
