# Testing

**Checked against `develop` at `0776ff4f`, 2026-09-30.** What the tests cover,
how CI runs them, what a green run does and does not mean, and how the manual
passes are recorded.

## Suites

| Suite | Run with | Where |
|---|---|---|
| Rust (every crate: unit, integration, property tests) | `cargo test --workspace` | `crates/*/src` (`#[cfg(test)]`), `crates/*/tests/` |
| Desktop unit tests (94 in 12 spec files, review count) | `npm test -- --watch=false` in `apps/desktop` | `apps/desktop/src/app/**/*.spec.ts` |
| MLX sidecar (37, review count) | `python -m unittest discover -s crates/pwr-mlx/sidecar` with the engine's interpreter | `crates/pwr-mlx/sidecar/test_pwr_mlx.py` |
| Protocol transcripts | part of the Rust suite | `crates/pwr-cli/tests/fixtures/acp/` |
| Conversation fixtures through a real `take_turn` | part of the Rust suite | `crates/pwr-cli/src/two_loops.rs` |
| Regression suites (tool calls, navigation, editing, verification) | `pwr eval suite suites/a1-tool-calls.json` … | `suites/` |
| Milestone table matches its manifest | `python3 scripts/milestones.py && git diff --exit-code docs/roadmap.md` | `scripts/milestones.py` |

## What they show, and what they do not

They show the harness's invariants: path policy, the sandbox profile,
tool-call parsing, edit conflicts, cancellation, compaction, completion
holds, action budgets, corpus accounting, protocol and session behaviour.

They do not show: that a 9B uses the tools reliably; that a specification
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

No inference engine or live model runs in CI.

The release workflow (`.github/workflows/release-macos.yml`) builds the DMG
from a tag and **runs none of these** (plan W9.1).

## What "passed" means today

A passing run can include tests that did not exercise anything:

- **Early returns count as passes.** Tests that need something on the host
  return when it is missing: Docker (`sandbox_and_approvals.rs:1518`), .NET
  (`1321`, `1357`), a browser (`1570`), an unconfined opt-out (`1265`), a route
  to a remote host or a non-loopback address (`local_service.rs:114`, `131`),
  a home or denied paths (`provisioning.rs:157`, `166`), a live model
  (`pwr-models/tests/live_model.rs:31`).
- **Environment failures look like test failures.** On a Mac where Docker
  Desktop is stopped but its socket file remains, the Docker test runs and
  fails (the only failure on 2026-09-30,
  [verification](reviews/2026-09-30-verification.md#test-suite-reproduced)).
- **Ignored tests** (`#[ignore]`, 6 in the workspace) need the network or a
  model and are run by hand.

Plan W0.2 makes skips explicit (`PWR-SKIP <test> <reason>`), probes the
Docker daemon before asserting, and has CI list what it did not exercise.

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
