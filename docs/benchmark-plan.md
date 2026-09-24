> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Benchmark Plan

The corpora as they exist. The suite they are meant to become — twenty-six
capability areas, a difficulty ladder, a failure taxonomy and the harness
torture tests — is in [benchmark-design.md](benchmark-design.md), together with
the parts of that design this project is not following and why.

Build a frozen, licensed corpus of small bugfixes, multi-file changes, repository questions, refactors, test failures, and tool-policy attacks. Each task has a base commit, statement, allowed files, hidden/visible verifier, time budget, and contamination/provenance note.

Phase A benchmarks calibration: latency, throughput, memory and failure rate across context ladder. Phase B benchmarks agent quality: verified resolution, regressions, cost proxy, tool actions, and intervention. Compare Qwen3.8 primary versus Ornith challenger first; include all installed models as controls only after capability probe success. Run repeated seeded trials; retain raw traces/redacted artifacts and publish a Markdown plus JSON report. Change one independent variable per experiment.

## Corpora, as they exist

| Suite | What it exercises |
|---|---|
| `m5-frozen-v1` | Eight tasks across all six original kinds, on files of tens of lines |
| `generation-v1` | Building an HTTP API from a specification, scored by a verifier that starts the server |
| `realistic-v1` | A line buried in a 200-function file, a 40-file repository that never names the file to change, a two-part change in a large repository, and an injection buried in `docs/` |
| `vague-v1` | The same defects as bug reports rather than specifications, one with the symptom in a different file from the cause |
| `longhaul-v1` | A rename reaching twelve call sites, sized past the point where the history fits in one context |

`m5-frozen-v1` was written before partial editing and retrieval existed, and every task in it fits in a file small enough that whole-file rewriting works and a repository small enough to list. That is why six campaigns against it never surfaced either limit, and why the later suites exist.

Every hidden verifier is validated in both directions before any model runs it: a correct implementation passes and the untouched workspace fails. A verifier that passes both is not a verifier, and one has already been found that way.

## What a task must carry before it is admitted

A reference solution, held outside the corpus and never materialised into a
run's workspace, and proved by `crates/pwr-eval/tests/corpus_is_sound.rs`:
the hidden verifier must fail at the baseline, the statement's claim about the
visible verifier must be true, the reference must pass both, and it must stay
inside the scope the statement declares.

That gate exists because two corpus defects reached campaigns before it did,
both the same shape — a task scored against a rule its statement never gave. It
has since caught a task whose defect could not reach its own visible test, and a
generation task that had never been validated at all.
