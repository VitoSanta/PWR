# PWR: product and research contract

**Adopted 2026-10-01**, approved by the owner after F0 review. Replaces the
[2026-09-30 contract](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/MASTER_SPEC-2026-09-30.md) through
[D-2026-10-01-2](docs/decisions.md#d-2026-10-01-2--mission-expansion-and-windows-exit-gate).
This is the mission and evidence contract; it does not claim that its
PLANNED destination has been implemented or measured.

## Mission and product boundary

Build the reference local agent harness for open-weight models on consumer
hardware, approaching the experience and objectively verified results of
frontier harnesses. This is the owner's destination, **not a capability claim**.
Claude Code, Codex CLI and Cursor are comparison references, not runtime
dependencies. PWR remains local, with no cloud inference unless the owner
explicitly changes that boundary.

Coding is the core: bugs, features, refactors, new projects, long sessions and
resumption in real repositories. Broader capabilities enter through a stable
extension interface, with capability-specific permissions and objective task
verifiers. Research, documents, data, browser, vision, memory, subagents,
background work and computer use are **PLANNED directions**, not promises that
the current product implements them. Load optional tool descriptions on demand;
measure their token cost and do not hardwire each integration into the core.

Start with Apple silicon and the owner's M2 Max with 64 GB unified memory.
Keep engine and sandbox interfaces portable. Build Windows only after the
exit criterion below passes, or after an explicit, evidenced revision approved
by the owner. No claim of Windows command confinement before implementation
and real hardware tests.

## Claims, design and effects

Use the [evidence vocabulary](#evidence-vocabulary) for every system-state
claim: IMPLEMENTED, EXPERIMENTAL, PLANNED, HYPOTHESIS, MEASURED, or `unknown`.
Recheck earlier documents, tests and reports against the current code and
artifacts. A reachable implementation, a green test and a model calibration
are distinct from measured agent competence.

Every design choice cites an opened primary source or a repository measure.
Without one, record a falsifiable hypothesis with a control, metric, threshold
and rejection rule. Upstream behavior belongs in declarative family adapters
and profiles; avoid model-specific core branches. Source code reuse requires
an Apache-2.0-compatible license review and attribution in NOTICE. Observable
ideas from proprietary products do not authorize copying their code.

The model proposes; the harness owns file versions, permissions, checks and
budgets. Tool output, files, web pages and MCP responses are untrusted data,
never authority to change instructions or grant effects. Enforce the same
effect boundary for file tools and commands; state each unenforceable limit.
Keep objective revisions intact, writes atomic and based on the version seen,
and acceptance artifacts frozen. Missing checks, zero tests and unconfined
runs do not become verified results. Report errors explicitly, including
context preparation failures; no I/O or model-output error becomes success.

## Model tiers

Tier assignment uses resident memory, KV requirements and active parameters
on this machine, not total parameter count alone. The examples below are
**HYPOTHESIS** classifications until the installed artifacts are measured.

| Tier | Candidate scope | Intended use, to be measured |
|---|---|---|
| S | Dense up to about 8B, including 4B/8B | Small reliable tasks with strong harness support |
| M | Dense 9–16B | Daily use on bounded tasks |
| L | MoE 20–50B total with few active B; dense 20–32B | Candidate daily-use sweet spot |
| XL | Dense about 70B; Q4 weights roughly 40 GB before KV/overhead | Correct, predictable work even when slower |

Select at least two current families per tier after verifying availability
and actual fit. Do not infer memory fit from the approximate XL weight size.
Measure effective usable context, not just the advertised maximum. Any tier
adaptation of prompt, offered tools, edit format, context or reasoning budget
is declarative and must earn its cost in a controlled comparison.

## Phases and execution

| Phase | Work and completion evidence |
|---|---|
| F0 | Establish the source/check baseline and proposed contract; maintainer review before adoption; record public decisions and milestone state |
| F1 | Audit engines, context, loops, tools, effects, checks, persistence, UI, evaluation and code; record promise, actual source, behavioral evidence, severity and gaps; obtain initial performance and dev capability baseline |
| F2 | In parallel with F1, verify current competitors and primary literature; record revision/date/license, local support limits and hypotheses in the detailed plan |
| F3 | Eval uses the app executor with parity tests; minimal same-engine control; competitor endpoints verified; frozen dev/heldout split and provenance; prospective power analysis |
| F4 | Improve coding across all tiers in controlled dev cycles; keep, revise or remove each mechanism by its measured benefit and cost |
| F5 | Stable extension/permission interface first, then capabilities in measured value order; each has objective verifiers and a threat model |
| F6 | Freeze binary, sidecar, deployments and preregistration; heldout once per configuration; PWR vs simple loop and at least two competitors per tier; publish raw artifacts |
| F7 | After the exit gate: persistent llama.cpp engine, CUDA/Vulkan, cancellation/cache/template parity, Windows isolation, CI and real GPU tests |

F1 and F2 overlap. Correct clear correctness/security defects with a failing
behavioral test before the minimal fix. Do not change model-behavior defaults
before the initial dev/performance baseline, or claim improvement before F3
can measure the shipped path. Existing defaults remain hypotheses, including
compaction ceiling, actions per turn, sampling floor, same-model review and
collapsed-reply recovery. Their current values must come from code, not this
contract. No heldout inspection or tuning; never weaken a verifier or change
a task to improve a score.

Each cycle records the problem/evidence, hypothesis, design with simpler
alternative, failing test, minimal implementation, local CI-equivalent
checks, controlled measure and keep/revise/remove decision. Update the
appropriate existing technical documents and milestone state in the same
change; record the outcome and its limits for contributors. No visible
feature removal without owner approval. A mechanism failing its controlled
comparison is removed under that approval rule, with a negative result logged.

Keep only modules reachable from product/evaluation with meaningful behavioral
coverage. Audit dead flags, dependencies, license obligations, duplicate paths,
unchecked errors and model-input parsers; use properties/fuzzing where useful.
Split large functions at their decision boundaries. At equal result, choose
the simpler implementation.

## Measurement contract

Measure the executor users run; scripted-loop results do not establish a
product-path improvement. Compare PWR, a minimal shell/edit control and
competitors at the same model revision, quantization, sampling, window and
budget. List every difference that cannot be equalized, including endpoint
protocol, prompt/tool rendering and sampling support.

Before each campaign, specify the smallest relevant difference and use a
power analysis to choose tasks × trials; declare an insufficient budget rather
than presenting an underpowered campaign as confirmation. Record per-task
paired binary outcomes, unbiased repeated-trial pass@1 and between-trial
variance. Use exact McNemar or a task-paired bootstrap for differences, Wilson
intervals for proportions and Holm correction for multiple comparisons.
Report effects with intervals plus time, tokens, human interventions and false
acceptance. Repeated trials must not be treated as independent new tasks.

Every run automatically records binary/commit and sidecar identity, model
repository/revision, quantization, engine/version, effective sampling and each
value's provenance, actual window, seed, machine and load. Freeze task and
verifier identities. Restore owner acceptance artifacts in an isolated hidden
verifier. Docker unavailable is an environment failure, not a task verdict.
Record unattended first-cycle outcomes separately from oracle-nudged outcomes.

Track cold/warm TTFT, prefill/decode tok/s, matched pure mlx-lm and llama.cpp
overhead, bandwidth/bytes-per-token roofline, harness tokens per turn and cached
prefix fraction. Also task time, model calls, compaction cost, engine weights/KV
peak memory, core/app RSS, idle CPU and startup. Timing runs require one engine
on an idle machine; repeat and report medians and dispersion. Promote suitable
deterministic metrics to CI regression checks only after calibration. Negative
and inconclusive results remain part of the evidence.

## Exit criterion before Windows

**PLANNED, numerical thresholds pending initial baseline and owner approval
before F6.** All criteria must hold in the preregistered confirmatory campaign:

1. In each tier, non-inferiority within an approved margin against the simple
   loop and the two best eligible competitors on the same model; statistically
   significant superiority in at least S, M and L, with costs reported.
2. Approved per-tier daily-use bounds for warm TTFT, task time and interventions.
3. Approved lightness bounds for harness tokens, cache reuse, peak memory,
   app/core RSS and idle app CPU.
4. Zero false verified results and zero effects beyond declared sandbox bounds
   in the campaign; local suites and CI green, with skipped coverage explicit.
5. F1 findings closed or remaining risks accepted in writing; no known dead code.

Zero observed violations is evidence about this campaign, not proof of universal
safety. If a criterion cannot be achieved on this hardware/model range, show
the evidence and propose a revision; do not declare success. Historical
`docs/thresholds.json` bars remain at their build-time paths and do not
substitute for this gate. Amend them and build fingerprints only through the
existing threshold-change process if required by an approved campaign.

## Contributor records and release policy

Keep W0–W10 traceability in the implementation plan and public milestone state
in `docs/milestones.json`. Release candidate checks belong in
`docs/release/next-release-readiness.md`. Preserve historical technical findings
and negative results; distinguish them from current candidate evidence.

Public source, comments and contributor documents use English. Personal
editorial material, session handoffs, machine schedules and restore instructions
are maintained outside the repository. Public reports use anonymised evidence
identifiers and explicitly say when raw artifacts are unpublished.

Contributions preserve unrelated working-tree changes. Performance campaigns
must use an exclusive engine lease and report machine load; only one inference
engine runs at a time for comparable timing measurements. Model downloads and
private artifacts are not source files.

Changes land on `develop`; the release process is documented in
[distribution](docs/distribution.md). A version tag builds a draft prerelease
after CI; publication is a separate maintainer decision. Neither a document
update nor a local suite pass advances a release gate without its required
evidence.

## Evidence vocabulary

Every claim in a current document carries one of these words, or `unknown`:

- **IMPLEMENTED** — a reachable path exists, cited by source; says nothing
  about model quality.
- **EXPERIMENTAL** — implemented, off by default or explicitly labelled,
  benefit not established.
- **PLANNED** — an item of the [implementation plan](docs/plan/implementation-plan.md).
- **HYPOTHESIS** — a falsifiable claim with its control, metric, threshold and
  rejection rule.
- **MEASURED** — a result with its conditions, counts and provenance (commit,
  deployment, corpus revision).

A document that cannot cite evidence says `unknown`. Changing a status cites
the evidence in the same edit.

## Authority

1. This contract — purpose, promise, principles.
2. [decisions.md](docs/decisions.md) — dated decisions, including what is not built.
3. [The implementation plan](docs/plan/implementation-plan.md) and
   [roadmap](docs/roadmap.md) — order of work.
4. The current technical documents indexed in [docs/README.md](docs/README.md)
   — what the code does, at a named revision.
5. [SECURITY.md](SECURITY.md) — the operational boundary.
6. Everything in [docs/archive/](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/README.md) and
   [docs/reviews/](docs/reviews/README.md) — evidence about earlier revisions,
   never a description of the current one.
