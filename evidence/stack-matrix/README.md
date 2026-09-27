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
    python3 runner/run.py run --run <id> [--split dev] [TASK ...]

`runner/pin.sh` builds the release binary and pins it with the MLX sidecar
it reads (`~/Desktop/pwr-evidence/bin/pwr-<rev>` and `sidecar-<rev>/`), so a
rebuild or an edit during a campaign does not change what is measured.
`PWR_BIN` selects the pinned binary -- the runner uses the sidecar pinned
beside it and records its digest -- and `PWR_EVIDENCE_MODEL` the model.
Each task writes, under `~/Desktop/pwr-evidence/runs/<run>/<task>/`, the full
protocol transcript, the result, the diff against the seed and the verifier's
output.

## Splits

`dev` tasks are the ones PWR is improved against. `heldout` tasks are run only
to measure, after the harness is frozen for a campaign; a harness change made
after looking at a held-out failure moves that task to `dev`.

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
