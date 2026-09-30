> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Model selection and routing

Selection is a deterministic, explainable single-model decision for one task.
It consumes discovered deployments, registry profiles, hardware/budget,
backend capabilities, task requirements, calibration/benchmark evidence and
user overrides. It writes `ModelSelection { backend, deployment, profile,
context_budget, execution_policy, confidence, reasons }` to the run record.

```text
candidates = discover().filter(compatible_with(user_override, budget, backend))
for c in candidates:
  score[c] = hardware_fit + context_fit(task) + capability_confidence(task)
           + benchmark_quality(task) + speed(preference) - uncertainty_penalty
choose highest evidence-backed candidate; derive policy from task + capabilities
if none: explain rejects; use explicit unknown model only in generic mode
```

Hard constraints precede scores: explicit model/backend, local/privacy policy,
memory, backend protocol and minimum task context. Calibration evidence beats a
declared context. Ties prefer user intent then lower-risk/certified evidence,
not a family name. Future multi-model roles can call this selector per role,
but today only one frozen selection is permitted.

## What is implemented (AR-012)

`pwr models select` runs `SelectionRequest::select` over everything the
configured backend can currently reach, and reports the decision, the resource
budget it was made against, every rejected candidate with its reason, and the
artifacts eligible for this host that no backend is currently serving.

It loads nothing and runs nothing. Candidate facts come from backend discovery
plus evidence already on disk: context limit and size from discovery,
tool/streaming support from a persisted capability probe, speed from a matching
calibration, certification from a matching registry record. A capability nobody
probed is `Unknown`, not `false` -- reported as `false` it would look like a
measured absence, and a deployment that does support tools would be excluded on
evidence nobody gathered. Quality is left absent because nothing here measures
it, so a fast deployment never scores as a good one for want of a benchmark.

Local configuration is read before the backend is contacted, so an unreadable
registry is reported as an unreadable registry rather than as "no backend
answered".

`run --model auto` uses the same selector and additionally requires a
calibration for the deployment it picks. Explicit `--model` is never
substituted.

## A model reference names one artifact

Measured on LM Studio: `qwen/qwen3.5-9b` names both an MLX 4-bit build of
5.98 GB (`architecture: qwen3_5`) and a GGUF Q4_K_M build of 6.55 GB
(`architecture: qwen35`). Under the bare key they produce one deployment
fingerprint, so a calibration, a capability probe or a certification of one is
read back as evidence for the other.

The deployment is therefore the variant-qualified reference
(`qwen/qwen3.5-9b@4bit`), and the digest is derived from it. A bare key that
names several artifacts is refused with all of them spelled out; one that names
a single artifact still resolves, because the rule is about identity and not
about spelling.

The variant is addressable for chat but not for loading: `/api/v1/models/load`
answers `model_not_found` for it and takes the bare key, so a load sends the key
and then checks which artifact actually came up rather than assuming.
