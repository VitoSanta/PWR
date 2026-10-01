# Testing

**IMPLEMENTED suites, MEASURED in the 2026-10-01 F1 sampling cycle.** What the tests cover,
how CI runs them, what a green run does and does not mean, and how the manual
passes are recorded.

## Suites

**MEASURED F1 sampling cycle, 2026-10-01, parent `84b4654c` plus reviewed
sampling fixes and preexisting process-safety changes:** Rust exit 0, 1,405
reported passed / 0 failed / 5 ignored across 102 target summaries; one
Docker-dependent test skipped (included in reported successes). Desktop 107
tests in 13 files and production build, sidecar 41 tests, Clippy with denied
warnings, formatting and milestone agreement pass. Nineteen new sampling
regressions cover exclusions, mode selection, complete alternatives and pinned
cache identity/provenance; a loopback server exercises the real installed-model
fetch/cache wrapper. Existing npm dependencies were reused.
See the [sampling audit follow-up](reviews/2026-10-01-audit.md#f1-sampling-follow-up--after-context-correctness)
for red/green provenance and limits. Hosted CI, clean npm installation and the
Docker case remain unverified; no live-model measurement.

| Suite | Run with | Where |
|---|---|---|
| Rust (every crate: unit, integration, property tests; **1,405 reported passed, 5 ignored; one Docker skip** on 2026-10-01) | `cargo test --workspace` | `crates/*/src` (`#[cfg(test)]`), `crates/*/tests/` |
| Desktop unit tests (107 in 13 spec files, 2026-10-01) | `npm test -- --watch=false` in `apps/desktop` | `apps/desktop/src/app/**/*.spec.ts` |
| MLX sidecar (41, 2026-10-01) | `python -m unittest discover -s crates/pwr-mlx/sidecar` with the engine's interpreter | `crates/pwr-mlx/sidecar/test_pwr_mlx.py` |
| Protocol transcripts | part of the Rust suite | `crates/pwr-cli/tests/fixtures/acp/` |
| Conversation fixtures through a real `take_turn` | part of the Rust suite | `crates/pwr-cli/src/two_loops.rs` |
| Regression suites (tool calls, navigation, editing, verification) | `pwr eval suite suites/a1-tool-calls.json` … | `suites/` |
| Milestone table matches its manifest | `python3 scripts/milestones.py && git diff --exit-code docs/roadmap.md` | `scripts/milestones.py` |

## What they show, and what they do not

They show the harness's invariants: path policy, the sandbox profile,
tool-call parsing, edit conflicts, cancellation, compaction, completion
holds, action budgets, corpus accounting, protocol and session behaviour.

They do not show: that every model family's calls are read (that is a calibration, run per model on the machine: [models.md](models.md#quick-calibration)); that a 9B uses the tools reliably; that a specification
survives many compactions; competence on new repositories; the quality of a
generated UI; that any adaptive policy helps. Those are measured, not tested
([evaluation.md](evaluation.md)).

## CI

`.github/workflows/ci.yml`, on pushes to `develop`, `stage`, `main` and on pull
requests, three jobs:

- **`check`** (macOS 14, because the sandbox adapter is Seatbelt): `cargo fmt
  --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test
  --workspace --no-fail-fast` (failing tests named in annotations), and the
  milestone table check.
- **`desktop`** (macOS 14): `npm ci`, the unit tests, the production build.
- **`mlx-sidecar`** (macOS 15, Apple silicon): the pinned `mlx` and `mlx-lm`,
  then the sidecar's unit tests, offline.

No inference engine or live model runs in CI. **No push and no CI run has happened since 2026-09-30**: everything above is local evidence, and gate G1 needs the CI run.

The release workflow (`.github/workflows/release-macos.yml`) reuses the CI
workflow (`workflow_call`) and builds the DMG only after it passes (plan W9.1,
implemented; never run in anger: no tag has been pushed since).

## What "passed" means

A host-dependent test that finds its resource missing **skips through
`common::skip`** (`crates/pwr-tools/tests/common/mod.rs`): it prints
`PWR-SKIP <test> <reason>` and, when `PWR_SKIP_LOG` names a file, appends the
line to it (cargo hides the output of a passing test, so the file is the
record). CI sets the log and turns each line into a `test not exercised`
warning annotation, or says that every host-dependent test ran.

The tests that skip: Docker (no engine socket, or a socket with no daemon
behind it), .NET (two), a browser, an unconfined opt-out, a route to a remote
host, a non-loopback address, a home with none of the denied paths. To see
what a local run skipped:

```bash
PWR_SKIP_LOG=$TMPDIR/skips.log cargo test --workspace --no-fail-fast; cat $TMPDIR/skips.log
```

- **Environment is not failure.** The Docker test probes the daemon with
  Docker's `/_ping` before asserting. Docker Desktop leaves its socket file
  behind when it quits, and the test used to fail for that reason alone
  ([verification](reviews/2026-09-30-verification.md#test-suite-reproduced)).
- **Ignored tests** (`#[ignore]`, 6 in the workspace) need the network or a
  live model (`PWR_LIVE_MODEL`) and are run by hand.

## Test-only switches

| Variable | For |
|---|---|
| `PWR_LIVE_MODEL` | `pwr-models/tests/live_model.rs` against a real model |
| `PWR_LLAMA_SMOKE_MODEL` | a llama.cpp smoke test |
| `PWR_UPDATE_GOLDEN` | regenerate recorded transcripts |
| `PWR_DOWNLOAD_FREE_BYTES` | simulate free disk space |
| `CI=1` | reproduce hosted-runner conditions for the .NET and `look_at` tests |

## Manual passes

The app is walked by hand on a built bundle before a release and after
significant changes: engine setup, model download, chat, agent, goal mode,
permissions, the trace views, themes, narrow windows and full screen. Each
walk is recorded in `docs/release/` with what was checked and what was found
(the latest: [v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md)).
No end-to-end automation covers first launch, engine install, workspace change
or shutdown (plan W7.6).

## Rules

From [CONTRIBUTING](../CONTRIBUTING.md): a change carries a test that fails
without it; tests check behaviour, not source text; a host-dependent test says
when it skipped.
