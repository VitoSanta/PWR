> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Manual testing on real repositories — 2026-09-06

## Current desktop goal-mode pass — 2026-09-21

The retained protocol below describes the earlier `run` path. The current
manual product pass is the desktop app, launched by running `pwr` from the
root of the repository to test. On macOS it uses MLX by default; use
`POORAI_BACKEND=llama PWR` only when deliberately testing a GGUF through
llama.cpp.

The installed launcher resolves PWR's own Cargo workspace before it checks
for a rebuild, while preserving the shell's current directory as the target
workspace. Therefore `pwr` is valid from a non-Rust project such as an
Angular application; `PWR --cli ...` addresses the direct terminal core.

For a substantial task such as an Angular site, select the assistant, enable
**Goal mode** in the compose bar, then send one complete objective. Goal mode
automatically continues across the ordinary 26-action conversation checkpoint.
The model's `complete` action is not proof. The workspace's **full discovered
check suite** always runs, but PWR only reports a verified goal after a
passing, explicitly-declared product acceptance check. A failed build, test or
lint check is returned to the model as evidence for another iteration. When
technical checks pass but no product acceptance check exists, PWR stops with
"technical checks passed; goal acceptance not verified" rather than inventing
a success. Stop remains available throughout; the 208-action aggregate guard
pauses for a human decision and never represents a successful completion.

Before testing, make the acceptance evidence real. `package.json` and CI help
PWR discover technical checks, but a goal needs a command with
`"kind": "acceptance"` in `.poorai/checks.json` **before opening the Goal
mode session**. The declaration is snapshotted and must remain unchanged: the
model may improve tests while it works, but it must not manufacture or relax
the evidence that certifies its own completion. Start a new session after human
review when the acceptance contract genuinely needs to change. It is intentionally generic:
web projects can run browser/e2e route checks; a backend can exercise an API
contract or smoke environment; a CLI can execute its user workflow; a desktop
application can run a deterministic smoke test; migrations can check schema
and data invariants. The command must be a real repository-owned test, not a
sentence from the model.

```json
{
  "checks": [
    { "executable": "npm", "args": ["run", "build"], "kind": "technical" },
    { "executable": "npm", "args": ["test", "--", "--watch=false"], "kind": "technical" },
    { "executable": "npm", "args": ["run", "test:e2e"], "kind": "acceptance" }
  ]
}
```

The full verifier executes all declared checks. A green, pre-existing acceptance command is
the extra evidence that permits the product-facing **Goal verified** state;
it does not replace review of anything that cannot be expressed deterministically.
Record both the final full-check result and what those checks actually cover.

Record for this first goal-mode pass: the exact task, selected model, workspace
root shown in the header, elapsed time, total action count, each automatic
checkpoint, whether `complete` was reached, the final full-check summary, and
the diff. A failed verification, an inappropriate continuation, a false
completion, or a pause at the aggregate guard is a finding, not a reason to
silently retry under different conditions.

### Attaching supporting files

Use the `+` button in the desktop composer to choose a text file or PDF from
Finder. The file may live outside the workspace: PWR reads only the selected
path once, extracts bounded text (for PDFs when extractable), and writes a
content-addressed read-only snapshot under
`<workspace>/.poorai/chat-attachments/`. It does not grant the model general
access to the folder that contains the selected file. The same picker can
attach a folder as a bounded read-only text snapshot; generated and private
workspace directories such as `.git`, `.poorai`, `node_modules`, `target` and
`dist` are excluded.

### Desktop end-to-end evidence — 2026-09-21

An isolated, committed JavaScript fixture was opened through the desktop
launcher at `/private/tmp/pwr-e2e-ui.ojbmkY`. The app displayed that exact
workspace and the selected Nemotron MLX assistant. The same desktop core path
was then driven through its ACP boundary with the task: change `greeting()` to
`"hello PWR"`, update its test, run `npm test`, and complete only after it
passes. Nemotron performed 9 actions: read the implementation, test and
manifest; changed implementation and test; ran `npm test`; reread both files;
then called `complete`. Goal verification discovered and ran `npm test --silent`
and reported 1/1 checks passing. The recorded diff and a second independent
`npm test` both confirm the requested change.

This proves the desktop launch/configuration plus the live core path on a small
real model task. It does not replace the planned Angular manual pass, full-screen
and resize interaction checks, or automated GUI control: the current raw Slint
binary is not exposed as a controllable macOS application surface to the active
computer-use provider. Those remain explicit product-test work, not inferred
success.

### Session logs and support evidence — 2026-09-21

The desktop app persists each conversation in
`<workspace>/.poorai/state.sqlite`; its authoritative listing is ACP
`session/list`, which includes the conversation id, workspace, title, time and
message count. The app's Review controls can request a human-readable report
or diagnosis for the active session, but it does not yet expose a complete
export bundle. The older CLI `session list` is a separate legacy surface and
does not currently enumerate ACP-created conversation ids. Until the desktop
history/export view exists, do not ask a user to copy SQLite data: collect the
visible transcript, selected model, workspace path, final verification output,
relevant diff and the session id from ACP support tooling instead.

For working the agent by hand on your own repositories, and for recording what
happens in a form the roadmap can use. Everything below was run on this host
before it was written; the numbers are observations, not expectations.

## Where the project actually stands

Ready for hands-on use on repositories you control, with a clean git tree, while
you watch. Not ready to be left alone with anything.

What the campaigns of 2026-09-06 measured, they measured through `eval`, which
hands the agent its verifier from the corpus. The `run` command discovers the
repository's own checks, escalates to the full suite at completion, and applies
its own timeouts — and **that path has almost no measurement behind it**. One
end-to-end run exists: a one-line arithmetic defect in a 34-file Rust crate,
`gpt-oss:20b`, 47 seconds, the correct line changed and the repository's own
tests green. That is the entire evidence for the product path. Which is exactly
why testing it by hand is worth your time: you will be exercising the surface
the campaigns do not reach.

Known and not worth rediscovering:

- One action per turn. Independent reads cost a turn each.
- The editing path still cannot pause to ask a question. In `pwr`, compose
  the clarified task directly in the TUI and press Enter to hand it to the
  fully-authorized agent.
- No image input, so nothing whose requirement is visual.
- A killed run does not resume. Its state is replayable and the loop cannot yet
  start from it.
- The action budget is a `u8`: no single run exceeds 255 actions.
- The sandbox is macOS-only.
- Cost is now recorded but no threshold judges it, and the cost ranking of
  deployments **inverts between repair and diagnosis** — do not carry an
  impression of "which model is cheaper" from one kind of task to another.

## Setup, once per repository

The agent's state lives in the workspace it runs in, so each repository needs
its own setup. It writes only inside the workspace.

```bash
pwr doctor                                   # host and backend facts
pwr models inspect gpt-oss:20b --probe       # required; about 2 minutes
```

The probe is not optional and its absence is the first thing you will hit:
without it a run refuses with `no active capability evidence`. It measures how
the deployment forms tool calls on this host and is stored under `.poorai/`.

A run also needs a calibration profile, which authorises how much context it may
use. The three MVP deployments already have v5 profiles on this machine:

| Deployment | `--profile` |
|---|---|
| `qwen3.8:27b-mlx` | `.poorai/calibrations/01a075b4-07a5-7b53-b111-950f934c1a67.json` |
| `ornith-1.5:35b` | `.poorai/calibrations/01a075b5-e692-7dc0-8ddd-6e767bcb2729.json` |
| `gpt-oss:20b` | `.poorai/calibrations/01a075b7-57a4-7672-8845-c2ae4ff58f42.json` |

Pass the absolute path. For any other deployment, calibrate it first —
`pwr calibrate <model> --ladder 8192,16384,32768` — and keep every profile:
a v4 profile cannot authorise a v5 run, and a profile is bound to this host.

Before the first run in a repository, **commit or stash everything**. The agent
edits files in place. Work on a branch you are willing to throw away, and if the
repository has no `.gitignore` for its build directory, add one — otherwise the
diff you read afterwards is mostly object files.

## The command

```bash
pwr run "<the task, in your own words>" \
  --model gpt-oss:20b \
  --profile /absolute/path/to/calibration.json
```

Run it from the root of the repository under test. Useful flags:

- `--dry-run` — what it would do, without a model turn.
- `--json` — machine-readable result; the human-readable form is the default.
- `--max-actions N` — a shorter leash than the profile's budget.
- `--turn-timeout-secs` — default 900. A turn that generates a subtle regular
  expression has been measured at 240 seconds where its neighbours took 3.
- `--plan` — decompose before acting. Off by default and **unmeasured**; if you
  use it, say so in the record, because no campaign has run with it on.
- `--session NAME` — carry what earlier runs established into this one.
- `--approve <effect>` — nothing beyond the workspace is granted by default.
- `--provision` — grants network *and* arbitrary executables together, to
  install a toolchain. Use it only for work you are willing to watch.

Checks are discovered rather than assumed, in this order: `.poorai/checks.json`
if the repository declares one, then CI configuration, then `package.json`'s
test script, then a build-system marker. If discovery picks the wrong command,
declare the right one:

```json
{ "checks": [ { "executable": "cargo", "args": ["test", "--workspace"] } ] }
```

## The plan

Four batches, in this order, because each one only makes sense if the one before
it worked. Stop and record when something fails; a failure is the result.

### Batch 1 — does it work on your repositories at all

Three tasks, each a defect you already understand, in three repositories with a
test command that works. Say what is wrong and where the symptom is; do not say
where the fix goes.

> The test `<name>` fails: `<symptom in one sentence>`. Find the root cause and
> fix it. Do not change the test.

Record per run: resolved or not, wall-clock seconds, the diff, and whether the
check it discovered is the one you would have run. The last is the point of the
batch — check discovery is the product path's least-tested step.

### Batch 2 — the product path's own machinery

- A repository with **no** CI config and no declared checks. Does discovery find
  a command at all, and is it the right one?
- A repository whose suite takes minutes. Does the run hold, or does a timeout
  end work that was correct?
- A repository whose suite is **already red** for an unrelated reason. The
  contract says a pre-existing failure may stay red only if the harness recorded
  it as known-failing; check that it neither "fixes" the unrelated failure nor
  refuses to complete because of it.

### Batch 3 — the limits we already know, confirmed on your code

- An **underspecified** task — one where a competent engineer would ask you a
  question. Record what it does instead. This is the strongest evidence for the
  missing capability, and your repositories will make a better case than a
  corpus task.
- A task needing more than a handful of files changed. Where does it stop, and
  does it stop honestly or claim completion?
- The same task twice at the same settings. Identical inputs do **not** reproduce
  a run here; two runs of the same task have been measured resolving and failing.

### Batch 4 — size, which is the whole point

A repository large enough that its file tree is not a map — several hundred
files, a defect whose module name does not announce it. Everything measured so
far tops out at 34 files or one 1,621-line file, and the two synthetic attempts
to make navigation hard both failed because a tree listing or a retrieved
excerpt gave the answer away. Your real repositories are the first honest test
of this, and this batch is the one most likely to produce a finding.

Record the peak prompt against the authorised context, the number of reads, and
whether it ever re-read something it had already seen.

## What to record, and how to get it

Every run prints a run id. Afterwards:

```bash
pwr report <run-id> --format md      # what happened, as a document
pwr session list                     # sessions in this workspace
pwr session show <name>              # what a session established, re-checked
PWR verify <run-id>                  # the event log's hash chain
```

For each run, keep: the task text verbatim, the deployment and profile, the run
id, resolved or not, seconds, the diff, and one sentence on what went wrong if
something did. The task text matters more than it looks — a wording difference
is a different experiment, and several of this project's findings turned out to
be about the instruction rather than the code.

Worth reporting back, in rough order of value: a check discovered wrongly; a
completion declared over a red check; a task that needed a question; a large
repository where retrieval sent it to the wrong place; and any run whose cost
looks nothing like the numbers in [experiment-log.md](experiment-log.md).

## What this is not

Not a benchmark. These runs are not preregistered, their seeds are not
controlled, and their tasks are not held out, so no number from them belongs in
the roadmap as a rate. They are for finding what breaks, which is a different
job and, right now, the more useful one.
