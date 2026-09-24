# External review, 2026-09-23 — what was checked and what follows

A senior engineer reviewed the repository read-only and delivered a report
(architecture, strengths, gaps, experiments, open-source readiness, a research
thesis and a prioritised roadmap). Every claim that could be checked against
the code or the artifacts was checked the same day, before anything was
adopted. This file records the verdicts; the actions are in
[`backlog.md`](backlog.md) (Part R) and the order in
[`roadmap.md`](roadmap.md).

Verdicts: **confirmed** (the code or an artifact says so), **partly** (true
with a qualification), **outdated** (true of an earlier state), **new** (found
while checking, not in the report).

## Findings

| # | Claim | Verdict | Evidence |
|---|---|---|---|
| 1 | The conversation grants every approval category in advance | **confirmed** | `chat_approvals` grants `all_approvals()` minus `ask_before`, whose default is only `HistoryRewrite` and `Publish`. So by default a conversation holds `DependencyChange`, `NetworkAccess`, `LocalService`, `ToolchainInstall` and `VerifierProposal`. **Consequence found here:** the guard added 2026-09-23 (installed dependencies read-only unless `DependencyChange`, backlog D.E2E-29) protects scripted runs and **nothing in the app's conversation**, where that approval is pre-granted. |
| 2 | `SandboxPolicy::Preferred` runs unconfined where no sandbox exists | **confirmed** | `prepare_command`: with no profile and `Preferred`, the command runs directly. Only macOS has an adapter. `will_sandbox` can report it; nothing makes it visible to the person. |
| 3 | The llama.cpp provider starts a server per generation | **confirmed** | `LlamaProvider::chat` starts `LlamaServer`, keeps it alive for one stream, drops it. `prepare_context` returns the requested size without asking the server. Not the live path on macOS (MLX is), but it is the path Windows depends on. |
| 4 | README status is stale and disagrees with the code | **confirmed** | README: "Status below describes code inspected on **2026-09-13**", "none of the above has been run against a live model", llama as "metadata inspection" — while the provider generates, and live runs are recorded daily since 2026-09-18. |
| 5 | The Angular app has no tests and is not in CI | **confirmed** | 0 `*.spec.ts`/`*.test.ts` under `apps/desktop/src`; the workflow runs only `cargo fmt`, `clippy`, `cargo test` and the milestone check. |
| 6 | 108 tracked `.poorai` records, some with home paths | **partly** | True of the local `redefinition` branch (64 models + 44 calibrations, 25 with the maintainer's home path). **The public `main` has 86, and none carries a personal path**: 22 were kept off it on 2026-09-23, and the 3 remaining `/Users/` strings are `/Users/runner/...` paths from Ollama's own CI builds. The maintainer then decided to take all of them off the public repository for now (backlog R.9); which to publish again, anonymised, is open. |
| 7 | No licence inventory of dependencies | **confirmed** | Nothing summarises or checks the Rust, npm and Python licences. |
| 8 | `.gitignore` has no rules for `.env*`, model weights, keys | **confirmed** | None present. No such file is tracked. |
| 9 | Two loops, not one runtime | **confirmed** | Already stated by `two_loops.rs` and the README; shared gate and perform, separate planning, compaction, completion and recovery. |
| 10 | No evidence for small models | **outdated** | True when read; the same day one Qwen3-14B run per arm (40K window, n=1) was recorded (backlog C.22). Still no 7–9B evidence and nothing at scale. |
| 11 | The effective-context ceiling is never passed | **confirmed** (not re-read in depth) | The window decision records `effective_context: not_set` in every inspection artifact read this week. |
| 12 | Test suite passes | **partly** | `cargo test` passes. **New: the public CI has been red since the 2026-09-23 push** — `cargo fmt --check` failed, so clippy and tests never ran there. The review ran `cargo test` only, as the maintenance loop here had. |

## Found while checking (new)

- **CI red on `main` since `39bf968e`.** Formatting failed; clippy with
  `-D warnings` had not been run on the branch at all (four lints, one of them
  in code older than this week). All fixed locally; the suite is run the way CI
  runs it. From now on a change is checked with `fmt --check`, `clippy -D
  warnings` and `cargo test`, judged by exit codes.
- **Finding 1 undoes part of D.E2E-29 in the product.** A fix measured in the
  scripted path is not a fix in the app until the app's approvals are checked.

## What is adopted

- **The research thesis**, as the framing for the small-model work the
  maintainer asked for: *can a local harness raise verified success on
  repository tasks for 7–14B models, at equal model, hardware and permissions,
  and by how much against a fixed tool-loop baseline?* It is sharper than
  "optimised for local models" and it is what C.22's experiment needs.
- **The headline metric**: absolute paired uplift in success rate, in
  percentage points, against a **fixed tool-loop baseline** (the model with the
  same tools and no harness management) — not against the bare model, which
  would credit the harness for the tools themselves. Failure reduction as a
  secondary figure only when the baseline fails often enough. No single
  "amplification factor".
- **The discipline of the complexity warning**: no multi-agent routing, general
  MCP/browser control or plugin framework before the paired benchmark shows a
  need. C.24 (fallback registry) and C.25 (vision) stay behind the safety and
  runtime items below, and each is admitted only with its own measurement.

## What is not adopted, or not yet

- **Moving `.poorai` evidence out of the repository** is the maintainer's
  decision: the public branch is already free of personal paths, and the
  evidence is tracked on purpose (see `.gitignore`). Recorded as open.
- **Unifying the two loops** is accepted as direction but not scheduled first:
  it is large, and the safety defaults and CI are cheaper and more urgent.
