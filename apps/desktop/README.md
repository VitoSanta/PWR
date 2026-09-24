# PWR desktop app

The product surface of PWR: a Tauri 2 shell with an Angular (zoneless,
signals) interface. It is a client of the core and nothing else -- it starts
`pwr serve --stdio` and speaks the Agent Client Protocol with PWR's
`_pwr/*` extensions ([`docs/pwr-serve.md`](../../docs/pwr-serve.md)).
Files, commands and diffs stay inside the core's policy, sandbox and audit;
the app never writes to a workspace itself. It is the supported desktop client.

## What it does

- A **streamed conversation**: the reply and the model's reasoning as they are
  written, one assistant turn with its actions grouped under it, markdown
  rendering, attachment cards for files and folders (drag and drop), and
  folders attached as read-only references.
- A **message queue**: what you write while a turn runs is queued and sent
  when it ends, or delivered into it with "Send now" (`_pwr/steer`).
- **Changes**, **Evidence** and **Core log** panels: per-file diffs with +/-
  counts, the latest file open.
- **Model and context in the top bar**: the model chip switches between the
  models the engine found, changes the working window and opens the **Model
  Manager**; the context indicator (`Context 42% · 54k / 128k`) opens the
  context panel -- what fills the window (estimated), the auto-compaction
  threshold, the last compaction and **Compact now**
  ([`docs/models-and-context.md`](../../docs/models-and-context.md)).
- **Model Manager**: this machine's memory, GPU, disk and engine; a search of
  Hugging Face for MLX models, with further pages available through
  **Load more models**; each variant is rated for this machine
  with its explanation; verified, resumable downloads into `~/.pwr/models`
  (or `PWR_MLX_MODELS`), with progress and cancel; the models on this Mac,
  with delete. GGUF search appears only when the core runs llama.cpp, which a
  release build on macOS never does.
- **Goal** mode (keep working across check-ins until verified) and the
  **Auto-approve** switch (off = Ask, on = Auto, amber) with a warning when
  commands are not sandboxed.
- **Chat mode**: "Chat without a workspace" talks to the model with no
  project open; it reads only the files, folders and images attached, and
  cannot edit or run anything (C.26). "Open a workspace" goes back.
- **Images** for models that see: an image file dropped or attached reaches
  the model (the picker marks them "sees images"); with any other model the
  composer warns and the core refuses the message.
- **Revert** a changed file, or all of them, from the Changes panel. The
  core does it (`_pwr/revert`): it refuses when the file no longer holds what
  the model wrote, so a later manual edit is never overwritten, and it records
  the revert in the conversation's log.
- Permission prompts (allow once, for the session, reject); conversations
  listed and resumed per workspace; the last workspace reopened.

## Layout

```text
src-tauri/src/lib.rs     the shell: finds and starts the core, relays ACP, remembers the workspace
src/app/core/            bridge.ts (Tauri IPC), agent.store.ts (state, signals), models.store.ts (Model Manager),
                         format.ts, model.ts, demo.ts; theme.ts (System/Light/Dark), layout.ts (panel
                         docking and sizes), ui.ts (shortcuts, dialog stack, toasts, confirmations)
src/app/ui/              conversation, composer, sidebar (and rail), inspector, diff, markdown, permission,
                         context-meter (indicator and panel), model-picker, model-manager, settings,
                         command-palette
src/app/ui/kit/          the shared primitives: icon, dialog, popover, select, tooltip, resize-handle,
                         toasts and the confirmation dialog
src/styles/              the design system, in layers: tokens, base, primitives, shell, conversation, panels
```

### Content Security Policy

`tauri.conf.json` sets one: scripts only from the app (Tauri adds the hash of
the inline theme script in `index.html`), styles from the app and inline
(Angular injects component styles at run time, so Tauri is told not to add
hashes to `style-src`, which would disable `'unsafe-inline'`), images from
the app, `data:` and `blob:`, and network access only to Tauri's IPC. Angular's
inlined critical CSS is off (`angular.json`), because it loads the stylesheet
through an inline `onload` handler the policy would block. A development build
uses a looser `devCsp` that also allows the dev server's websocket.

### Design system

Every colour, radius, spacing step, type size, shadow and duration is a
token in `src/styles/tokens.css`, defined once for the dark theme and once
for the light one; components use the semantic names (`--surface-primary`,
`--text-muted`, `--radius-control`…) and never literal values. Geometry is
strict: controls (buttons, inputs, selects, menu items) share
`--radius-control`, cards and popovers `--radius-card`, the composer
`--radius-panel`, dialogs `--radius-modal`, and pills are only for badges,
status and chips. One icon family (`pa-icon`), one dialog (`pa-dialog`: focus
kept inside, Escape for the innermost only, focus restored), one popover, one
select (a themed ARIA listbox in place of the native `<select>`), one
tooltip, one toast and one confirmation dialog (`ConfirmService`, instead of
`window.confirm`).

**Appearance** is System (the default, following the OS live), Light or Dark,
in Settings (⌘, / Ctrl+,) or the command palette; `index.html` applies it
before first paint and the native window follows it.

**Panels** dock while the conversation keeps at least 560 px: as the window
narrows the inspector collapses first, then the navigation becomes a rail.
Either can then be opened over the conversation. Drag or arrow-key the
panel edges to resize them (double-click resets); sizes and open/closed are
remembered.

**Keyboard**: ⌘K / Ctrl+K opens the command palette, ⌘N a new conversation,
⌘B the sidebar, ⌥⌘B the inspector, Esc closes the innermost dialog, popover
or floating panel.

The shell looks for the core in `PWR_CORE`, then the bundled one, then the
checkout's `target/release/pwr`, then `pwr` on `PATH`. It hands the core the
engine's scripts bundled with the app (`sidecar/pwr_mlx.py`,
`sidecar/pwr_embed.py`, as `PWR_MLX_SIDECAR` / `PWR_EMBED_SIDECAR` unless
those are already set); a build without them falls back to the checkout's.
A release build always runs the MLX engine; `PWR_BACKEND` is honoured only by
a development build. For the engine's
Python it uses `PWR_MLX_PYTHON`, then the environment the app installed
itself (below); a development build (`npx tauri dev`) also takes the
checkout's `.venv-mlx`, a release build never does, so it behaves as it will
for anyone who downloads it. An app opened from the Finder gets the PATH of a
login shell, so the checks a workspace declares (`npm`, `cargo`) are found.

### First run: the engine

On an Apple-silicon Mac with no engine, PWR opens a setup screen instead of
the app. **Install engine** creates a private environment in
`~/Library/Application Support/ai.pwr.desktop/engine` with the `uv` bundled
in the app: a standalone Python 3.11 (not the system's, no Xcode tools
needed), the pinned `mlx`, `mlx-lm`, `mlx-embeddings` and `mlx-vlm` of
`scripts/setup-mlx.sh`, and the search encoder in the Hugging Face cache --
about 1.2 GB, with progress, cancel and retry. It is marked ready only once a
final import check passes. Then the app starts as usual and points to the
Model Manager for a first model. Deleting that folder brings the setup back.
`?setup` in a browser shows the screen with a simulated install.

## Build and run

On first launch PWR opens **Chat**, which has no project workspace. To
enter Agent mode, choose a folder; the first open asks you to trust that exact
folder before starting the core. Trust is remembered per folder, and does not
change the Ask/Auto-approve setting. The desktop bundle includes the `pwr`
core executable, the MLX engine's two Python scripts, and `uv`, which
`scripts/bundle-uv.sh` copies from `PATH` at build time (`brew install uv`;
the binary is not committed). The bundle is ad-hoc signed
(`signingIdentity: "-"`), not notarized.

```bash
cargo build --release -p pwr-cli          # from the repository root: the core
npm install
npx tauri build --bundles app                # src-tauri/target/release/bundle/macos/PWR.app
```

Copy `PWR.app` to `/Applications` to install it. `npx tauri build` (all
targets) also produces `bundle/dmg/PWR_0.1.0_aarch64.dmg`. `npx tauri dev` runs it
against the development server; `npx ng serve` alone serves the interface in a
browser, where `?demo` fills it with a recorded conversation (the core is not
reachable from a browser).

The bundled desktop icon is generated from `src-tauri/icons/pwr-source.png`.
After changing that source, run `npx tauri icon src-tauri/icons/pwr-source.png`
to regenerate the platform icons before building.

## Tests

`npx ng test --watch=false` runs the unit tests (Vitest): number and date
formatting, the store's handling of compaction and extension notifications,
the Model Manager's download states, theme resolution, the panel layout at
each window width (`src/app/core/*.spec.ts`), and the keyboard, ARIA and focus
behaviour of the select and the dialog (`src/app/ui/kit/kit.spec.ts`). The
rest of the interface is reviewed in a browser with `?demo`; there is no
screenshot testing, and the app is not in CI (backlog R.8).
`npx ng build` is the minimum check before committing a change here.
