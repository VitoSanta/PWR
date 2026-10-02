# Stack matrix

PWR on real tasks across languages, frameworks, databases and tools, run the
way a person runs it: the desktop app's protocol (`pwr serve --stdio`), one
conversation per task in goal mode, the permission questions answered by a
stand-in person who grants what the task names and refuses everything else.

The verdict never comes from PWR. After each turn a clean copy of the
workspace -- build output, caches and installed toolchains left out, the
owner's test files restored as they were written, the task's hidden tests laid
over it -- is run in the task's official container image (or, for a task
about containers, by a host script). A task passes when that run passes.

## A task

    tasks/<id>/
      task.json    id, title, stacks, category, split (dev | heldout), the
                   approvals the person grants, turns, time budget, and how
                   it is verified
      brief.md     what the person asks, verbatim
      workspace/   the repository as the person hands it over, with the
                   owner's acceptance tests and `.pwr/checks.json`
      hidden/      tests PWR never sees, laid over the workspace to verify
      reference/   a solution, laid over the workspace

`runner/run.py reference` proves every task sound before it is run: the
workspace as handed over fails its verification, and with the reference
solution it passes.

## Running

    python3 runner/run.py list
    python3 runner/run.py reference
    python3 runner/run.py run --run <id> [--arm pwr|minimal] [--split dev] [TASK ...]

`runner/pin.sh` builds the release binary and pins it with the MLX sidecar
it reads (`~/Desktop/pwr-evidence/bin/pwr-<rev>` and `sidecar-<rev>/`), so a
rebuild or an edit during a campaign does not change what is measured.
`PWR_BIN` selects the pinned binary -- the runner uses the sidecar pinned
beside it and records its digest -- and `PWR_EVIDENCE_MODEL` the model.
Each task writes, under `~/Desktop/pwr-evidence/runs/<run>/<task>/`, the full
protocol transcript, the result, the diff against the seed and the verifier's
output.

Since 2026-10-02 this runner is PWR's product-path evaluator
([D-2026-10-02-1](../../docs/decisions.md#d-2026-10-02-1--the-stack-matrix-runner-is-the-product-path-evaluator)).

**Arms.** `--arm pwr` (the default) is the product in Goal mode. `--arm
minimal` is the W8.3 control behind the same `pwr serve`: the same engine,
sampling, tools, sandbox and budgets with PWR's harness off. One arm per run;
`result.json` records `arm`, and the runner stops a task whose reply names a
different harness than the one asked for. Compare runs with `analyze.py`.

**One engine at a time.** A campaign takes an exclusive lease on
`~/Desktop/pwr-evidence/engine.lock` (`PWR_EVIDENCE_LEASE`) and is refused
while another holds it. Before each task it looks for other inference engines
(a PWR MLX worker, `mlx_lm.server`, `llama-server`, an Ollama runner or a model
Ollama has loaded), waits up to a minute for the previous task's to exit, and
stops if one remains: two engines on this Mac overran its GPU working set on
2026-10-01 and both wrote text without meaning. `--allow-busy-machine` runs
anyway and is recorded; timings from such a run are not comparable.

**Provenance.** `result.json` carries `provenance`: the binary's and the
runner's digests, the model artifact (revision, quantization, weights size,
config and template digests), the engine's Python, MLX and mlx-lm versions,
the effective sampling with each value's source (`_pwr/model_sampling`), the
granted window and reasoning effort (`_pwr/models`), the machine, the load and
free memory at start and end, the engines seen at start, and the seed -- none:
the core sends none, so repeated trials differ by sampling.

`python3 -m unittest discover -s runner` tests the runner against a stand-in
core, without a model or Docker.

## Analysis

    python3 runner/analyze.py compare --arm pwr=<run> --arm other=<run> [--split heldout]
    python3 runner/analyze.py power --p10 0.25 --p01 0.05

`compare` pairs arms on the (task, attempt) results they share and reports,
apart, the **first cycle** (passed on the brief alone, unattended) and the
**final** outcome (after the runner's nudges, an external oracle's
intervention): pass@1 per task, Wilson 95% intervals with one trial per task,
between-trial variance with repeats, a task-resampled bootstrap interval for
the difference, exact McNemar (one trial per task) or a paired sign-flip test
over tasks (repeats are not new tasks), Holm-adjusted across comparisons, and
every provenance field on which the arms differ besides the arm itself.
`power` gives the paired tasks the exact McNemar test needs for a given effect
(p10: passes under A only; p01: under B only) -- for example 61 tasks for
0.25/0.05 at 80% power, 210 for 0.12/0.04 -- which is how a campaign is sized
before it runs.

Read on the development runs of 2026-10-01 (Qwen3.6-35B-A3B, 8 dev tasks,
one trial each), the "3/8 to 6/8" after the review-round change is 3/8 to 4/8
at the first cycle and 3/8 to 6/8 only after nudges; final difference -0.375,
bootstrap [-0.875, 0.125], exact McNemar p = 0.375, binaries `9385a010` and
`7afe9fdc`. A signal worth testing, not a measured effect.

## Tasks kept outside the repository

`PWR_EVIDENCE_TASKS` names further task folders (separated by `:`), laid out
like `tasks/`. It is how held-out tasks are written by someone other than
whoever tunes PWR, who then never reads them: `run.py reference` and
`run.py run` take them like any other task, and an id may not repeat.

## Splits

`dev` tasks are the ones PWR is improved against. `heldout` tasks are run only
to measure, after the harness is frozen for a campaign; a harness change made
after looking at a held-out failure moves that task to `dev`.

`splits.json` freezes every task's split and digest (first frozen 2026-10-02:
20 dev, 11 heldout). `run.py run` refuses a task that is missing from it, has
moved split, or has changed since; `--allow-unfrozen` runs it anyway and the
result says so. A revision listed below is followed by `run.py freeze --reason
"<why>"`, which keeps the previous freeze and the tasks it changed in the
manifest's history.

## Revisions

A task changed after it was run is listed here, with what changed and why.
Every result records the task's digest (`task_digest`), so a result can be
matched to the text it ran against; results from before a revision are not
compared with results after it.

- 2026-09-26 `bash-rotate`: the README names GNU and BSD userlands. A run
  used BSD-only `stat` flags that the Linux verifier does not have.
- 2026-09-26 `cpp-ini`: the README names GCC/libstdc++ and Clang/libc++ and
  asks for every header used to be included. A run built on macOS with a
  header reached only through libc++ (`<stdexcept>`) and failed on Linux.
- 2026-09-26 `c-ringbuf`: the README names GCC and Clang, for the same reason.
- 2026-09-26 `node-fastify-tasks`: the README states that an unknown `to` is
  a 400, which the hidden tests check and the README had left open.
- 2026-09-26 `cs-docker`: the brief says the `Dockerfile` and `.dockerignore`
  go at the root of the repository, where the verifier looks. A run put them
  in the project folder, which the brief had not ruled out.
- 2026-09-26 `elixir-stock` and `dart-cron` moved from `heldout` to `dev`.
  Their c1 failures were read and the harness changed because of them: a
  host install recipe for Elixir (Docker cannot satisfy a check that runs
  `mix` on the host), and a review round that runs a rule no check covers
  rather than reading the code for it.
- 2026-09-27 `ts-ledger`: the README says a line break is `\n` or `\r`. It
  said "a newline", and a hidden test quotes a field holding a bare `\r`; a
  c2 run quoted only `\n`, a reading the text allowed.
- 2026-09-27 `swift-lru` moved from `heldout` to `dev`: its c2 failure was
  read and PWR changed because of it -- `swift test` could not run inside
  PWR's sandbox at all (macOS's per-user temporary directory refused, and
  SwiftPM's own sandbox unable to nest), so the model could never check its
  work on the Mac.
- 2026-09-27 `spring-library` and `kotlin-rules` moved from `heldout` to
  `dev`: their c3 runs were read and PWR changed because of them -- a JVM in
  the sandbox took its home from the account (Maven's repository refused in
  the real home) and its temporary directory from the system (refused in
  `/var/folders`), whatever HOME and TMPDIR said.
