# Frontend toolkit spike

**Historical decision record.** Tauri 2 + Angular (signals) in
`apps/desktop/` is the supported desktop client. The Slint prototype described
below was removed during production preparation after the Tauri app became the
selected client. Reasons and criteria are recorded in the roadmap's S2 entry.

## Tauri + Angular candidate — first cut, 2026-09-22

`apps/desktop/` runs with `npm run tauri dev` (a release core is expected at
`target/release/pwr`, or set `POORAI_CORE`; the workspace defaults to
`pwr-website/` in a checkout, or `POORAI_WORKSPACE`). Its native side,
`src-tauri/src/lib.rs`, is only a bridge: it starts `pwr serve --stdio` in
the chosen workspace, emits every protocol line as an `acp` event, writes the
interface's messages, and stops the core when the app exits. Everything else is
Angular with signals, zoneless, in `src/app/`:

- `core/agent.store.ts` — JSON-RPC over the bridge and the whole UI state:
  streamed reasoning and reply (from `agent_thought_chunk` and live
  `agent_message_chunk`), one card per tool call updated in place by id, diffs
  collected per file, permissions, check-ins, the goal outcome, sessions,
  models and the context window.
- `ui/` — sidebar (workspace, model, context, conversations), conversation
  (markdown sanitised with DOMPurify, collapsible reasoning, tool cards with a
  diff, auto-follow that stops while the person reads above, a working
  indicator that names what a silence can be), composer (several attachments,
  files and folders, Goal mode, ⌘↵, Stop), inspector (changes, evidence
  commands, core log), permission dialog.
- Motion in CSS only: entrance on every timeline entry, streaming caret,
  spinning tool state, aurora backdrop; all of it off under
  `prefers-reduced-motion`.
- `?demo` in a plain browser plays a scripted conversation for reviewing the
  design without a core.

Installed as `/Applications/PWR.app` (`npx tauri build --bundles app`,
10 MB, unsigned, still using the checkout's core and MLX interpreter). An app
opened from the Finder inherits launchd's minimal PATH, so the bridge gives the
core the login shell's PATH (`npm`, `node`, `cargo` for the checks). First
manual use found: opening another workspace showed "Error: the core stopped"
while the new core was healthy -- the old core's exit was taken for the new
one's; every start is now numbered and earlier exits are ignored. And every
conversation was titled "Ledger of 1 earlier run(s)…": the core named it after
its own injected context; titles now skip harness-written messages.

Second pass from the maintainer's review of a live run (2026-09-22): the
conversation was still hard to read. Consecutive actions now fold into one
group ("7 actions · 3 read · 2 edited"), one row each with `+/−` counts and a
status dot, open only while the turn works; diffs in the conversation are
closed until asked for; reasoning shows its last lines in a small window while
it streams and one line once done; replies are text, not cards. The composer
grows with its text, sends on ↵ (⇧↵ breaks the line), shows attachments as
typed cards and accepts files dropped on the window. Changes lists the most
recent file first and open, the others closed, each with `+/−`. The context
meter ("58K / 262K · 22%") reports the core's `_poorai/usage` after every
reply. Fixed: the model picker showed the first model while another was in
use (a `select` value bound before its options existed).

Measured on first launch (debug build, M2 Max): the app process 131 MB plus
WebKit's content, GPU and network processes ~195 MB. Not yet measured: the
long-conversation and 2,000-line-diff criteria, keyboard-only operation,
Windows, and bundling/signing.

**Status, 2026-09-21: frontend integration checkpoint ready; Slint candidate compiles and launches the
core over stdio. GPUI is not eligible for the current product requirement:
upstream 0.2.2 documents macOS/Linux only.**

The app is an evidence exercise, not a redesign contest. It must make one local
agent run legible and controllable without duplicating the harness. Both
candidates therefore consume `pwr serve --stdio` and implement the same
four regions: local models, conversation, action feed, and diff/permission
surface.

All planned pre-manual integration surfaces are now wired: model selection and
queued downloads, context window, approvals, sessions, evidence commands,
attachments and core state. The next activity is the single manual pass, not
another implementation loop.

## Current Slint candidate

`crates/pwr-app-slint/` is a standalone workspace to avoid changing the
dependency graph of the tested terminal. It launches `pwr serve --stdio`,
then can read and select an installed model through `_poorai/models`, create a
session, send a task and request cancellation. Its first screen reserves the
four target regions and shows protocol notifications in the action feed. A
permission request now appears with the protocol's `allow_once`, `allow_always`
and `reject_once` choices. The turn waits for a person to decide; this is still
not D.4 complete until it passes its usability check.

The action region also renders the latest `diff` content emitted by a completed
tool call. It is evidence that the native client can display the core-owned diff
without touching workspace files; file history/navigation and review remain D.5
work.

The conversation can queue a local file path and send it as a read-only
`resource_link` on the next prompt. The core still resolves and snapshots the
attachment inside its own sandbox; drag/drop and citation rendering remain.

The stdio bridge has a dedicated reader thread. During `session/prompt`, it can
therefore still write `session/cancel`; model/session commands received while a
turn runs are refused as busy rather than interleaved with the turn.

The model region also shows every declared HuggingFace artifact's local byte
state: missing, `.part` partial, or final file present but not rehashed. This is
only observation; refresh does not touch the network or claim verification.
The bridge starts the core with the selected workspace as its working directory,
so the registry and workspace-local configuration are resolved consistently even
when the app itself was launched from another directory. Packaging that registry
is now covered by the core's embedded fallback; a workspace-local registry still
takes precedence during development.

The lab now exposes per-artifact Download / resume and a sequential Download all
queue, showing byte progress as core download notifications. It can start with
only the selected workspace, so first-run download no longer requires a
model-backed session; a later catalog refresh recovers partial state from disk.

The lab also exposes the existing approval policy as two explicit workspace
controls: ask before every supported approval kind, or ask before none. The
fine-grained policy editor and window/sandbox settings remain later surfaces.

The context window is now adjusted through the core's `contextTokens` setting;
the app shows the granted value, binding ceiling, memory budget and rationale,
and provides smaller/larger controls. A richer calibration history is not yet
displayed.

Workspace sessions are now listed in the conversation column and can be resumed
through the core. A review of branch drift and interrupted-run reconciliation
still belongs to the later session surface.

The action area also exposes the core's Changes, Verify, Report and Diagnose
commands. Their results currently use the existing action feed; a dedicated
evidence/history view remains.

The header now has a core state line for connection, backend-unavailable,
cancellation-requested and request-error states. Empty catalog and recovery
copy still need a dedicated final layout.

It uses Slint 1.15 because that is compatible with the project's Rust 1.88.
Slint's compiler needs `unicode-width 0.2.2`, whereas the existing Ratatui 0.29
terminal exactly pins 0.2.0; a nested workspace isolates this temporary cost.
Slint offers GPLv3, royalty-free desktop, and commercial terms. S3 must make an
explicit license and attribution decision before distribution. The current
manual window is unbundled, so desktop automation cannot inventory it on macOS;
recorded visual/manual checks remain required.

## Fixed comparison criteria

1. Time from clean checkout to the same four-panel working screen.
2. Smoothness with a long conversation and a 2,000-line diff.
3. Clarity of one live tool/action feed and one permission decision to a new
   user.
4. Keyboard-only operation through model selection, prompt, approval and stop.
5. Startup time and resident memory after opening the same local session.
6. macOS and Windows build, bundle, signing and attribution/licensing path.

## GPUI eligibility result

GPUI 0.2.2 is Apache-2.0, but its own README calls it pre-1.0 and says callers
must be on macOS or Linux. The crate contains Windows-related dependency
entries, but that is not a supported Windows application path. This fails the
non-negotiable macOS-and-Windows criterion before a duplicate screen would add
useful evidence. No GPUI app was added.

**Provisional result:** Slint is the only eligible S2 candidate. This is not
yet the S2 exit: after the remaining frontend surfaces are integrated, run the
fixed manual interaction, keyboard, long-diff, startup and memory checks as one
batch. Reopen GPUI only once upstream documents and demonstrates supported
Windows use.

Sources: [GPUI README](https://github.com/zed-industries/zed/tree/main/crates/gpui)
and [Slint licensing](https://github.com/slint-ui/slint/blob/master/LICENSE.md).
