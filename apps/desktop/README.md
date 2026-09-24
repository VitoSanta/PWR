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
  when it ends, or delivered into it with "↳ now" (`_pwr/steer`).
- **Changes**, **Evidence** and **Core log** panels: per-file diffs with +/-
  counts, the latest file open.
- **Model and context in the top bar**: the model chip switches between the
  models the engine found, changes the working window and opens the **Model
  Manager**; the context indicator (`Context 42% · 54k / 128k`) opens the
  context panel -- what fills the window (estimated), the auto-compaction
  threshold, the last compaction and **Compact now**
  ([`docs/models-and-context.md`](../../docs/models-and-context.md)).
- **Model Manager**: this machine's memory, GPU, disk and engines; a search of
  Hugging Face for MLX or GGUF models, with further pages available through
  **Load more models**; each variant is rated for this machine
  with its explanation; verified, resumable downloads into the engine's
  models folder, with progress and cancel.
- **Goal** mode (keep working across check-ins until verified) and the
  **Auto-approve** switch (off = Ask, on = Auto, amber) with a warning when
  commands are not sandboxed.
- **Chat mode**: "Chat without a workspace" talks to the model with no
  project open; it reads only the files, folders and images attached, and
  cannot edit or run anything (C.26). "Open a workspace" goes back.
- **Images** for models that see: an image file dropped or attached reaches
  the model (the picker marks them "sees images"); with any other model the
  composer warns and the core refuses the message.
- Permission prompts (allow once, for the session, reject); conversations
  listed and resumed per workspace; the last workspace reopened.

## Layout

```text
src-tauri/src/lib.rs     the shell: finds and starts the core, relays ACP, remembers the workspace
src/app/core/            bridge.ts (Tauri IPC), agent.store.ts (state, signals), models.store.ts (Model Manager),
                         format.ts, model.ts, demo.ts
src/app/ui/              conversation, composer, sidebar, inspector, diff, markdown, permission,
                         context-meter (indicator and panel), model-picker, model-manager
src/styles.css           the theme
```

The shell looks for the core in `PWR_CORE`, then the checkout's
`target/release/pwr`, then `pwr` on `PATH`; for the engine's Python it
uses `PWR_MLX_PYTHON`, then the checkout's `.venv-mlx`. An app opened from
the Finder gets the PATH of a login shell, so the checks a workspace declares
(`npm`, `cargo`) are found.

## Build and run

On first launch PWR opens **Solo Chat**, which has no project workspace. To
enter Agent mode, choose a folder; the first open asks you to trust that exact
folder before starting the core. Trust is remembered per folder, and does not
change the Ask/Auto-approve setting. The desktop bundle includes the `pwr`
core executable.

```bash
cargo build --release -p pwr-cli          # from the repository root: the core
npm install
npx tauri build --bundles app                # src-tauri/target/release/bundle/macos/PWR.app
```

Copy `PWR.app` to `/Applications` to install it. `npx tauri dev` runs it
against the development server; `npx ng serve` alone serves the interface in a
browser, where `?demo` fills it with a recorded conversation (the core is not
reachable from a browser).

The bundled desktop icon is generated from `src-tauri/icons/pwr-source.png`.
After changing that source, run `npx tauri icon src-tauri/icons/pwr-source.png`
to regenerate the platform icons before building.

## Tests

`npx ng test --watch=false` runs the unit tests (Vitest): number formatting,
the store's handling of compaction and extension notifications, and the Model
Manager's download states (`src/app/core/*.spec.ts`). The rest of the
interface is still untested, and the app is not in CI (backlog R.8).
`npx ng build` is the minimum check before committing a change here.
