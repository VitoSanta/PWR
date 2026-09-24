> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Benchmark and certification

Certification is evidence, not a model label: Unsupported (no usable path),
Experimental (generic/limited evidence), Compatible (contract tests pass),
Certified (versioned suite and gates pass). It is scoped to model digest,
quantization, backend version, adapter version, hardware class and harness
revision; regression demotes rather than silently retaining a badge.

Suites measure instruction following, repository discovery/comprehension,
tools and arguments, structured output, edits/diff quality, tests/verification,
planning/replanning, malformed-output recovery, context recall/pollution,
long-horizon completion, latency, throughput, token/action efficiency and
unnecessary edits. Use frozen workspaces, visible and hidden deterministic
verifiers, fixed seeds where supported, repeated trials and raw artifacts.
Report counts/intervals rather than a magic score; a weighted task score is
allowed only with published weights and per-category gates. Normalize quality
within the same backend/hardware; report speed separately.

Existing `pwr-eval`, corpora, capability probes and `EvaluationRun` are
PARTIAL foundations. Add a certification registry only after their artifacts
have stable versioned semantics.

## What is implemented (AR-013)

`strategies/certifications.json` holds `CertificationRecord`s.
`pwr models certification MODEL` reports the level in force for the live
scope; `pwr models certify MODEL --level ... --rationale ...` files one.

The scope is taken from live observation rather than from arguments: model
digest and deployment fingerprint from the backend, backend version from the
backend itself, adapter revision from the family adapter that would read this
deployment's replies, hardware compatibility key from the probe, and the
harness revision from the build. A record therefore cannot be filed against a
backend, adapter or host other than the one that was measured.

Records are appended, never replaced. A later record supersedes an earlier one
by being later, which is what makes a regression a demotion rather than an edit
that erases the evidence it contradicts; superseded records stay visible in the
report. `Compatible` and `Certified` are refused without evaluation runs and
raw artifact hashes. A deployment with no matching record is `Experimental`,
never `Compatible`, and the selector then requires an explicit opt-in.

A backend that publishes no version cannot be scoped, and certification is
refused rather than attributed to an unknown build.

The suites themselves are still unbuilt: what exists is the registry and its
gates, not the measurements that would justify a `Certified` record.

## Running without a calibration (measured limitation)

The rule was "a run needs a calibration". On a backend that does not accept a
context window that rule can never be satisfied -- no ladder can be measured --
so it silently became "this backend can never run anything", which is a worse
answer than running on declared settings that say they are declared.

So `run` requires `--profile` on a backend that serves a requested window, and
on one that does not it builds a `ConservativeBootstrap` execution profile
instead: no calibration id, a context capped at
`BOOTSTRAP_CONTEXT_CEILING` (32768) rather than the window in force, and no
measured tiers for context recovery to retreat to. The run result says
`capacity_evidence: "declared, not measured"` at the top level, not three keys
deep inside the profile, because that is the difference between a result that
means something about this machine and one that means the run completed.

A bootstrap run is not evidence for certification. `Compatible` and `Certified`
still require evaluation runs and raw artifacts.
