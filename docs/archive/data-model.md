> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Data Model

```text
ModelDefinition { id, digest, family, quantization, capabilities, metadata, observed_at }
ModelStrategy { id, model_selector, role, prompting, reasoning, tool_policy, retrieval_policy }
DeploymentDescriptor { id, provider, endpoint, model_ref, backend_options, auth_ref? }
HardwareProfile { id, compatibility_key, os, cpu, accelerators, memory, storage, probe_version }
RuntimeSnapshot { id, hardware_id, deployment_id, timestamp, available_memory, pressure, loaded_models, backend_state }
CalibrationProfile { id, compatibility_key, model_digest, deployment_fingerprint, harness_rev, stable_points, raw_artifacts }
ExecutionProfile { id, strategy_id, calibration_id?, context, reserves, concurrency, budgets, rationale }
EvaluationRun { id, corpus_rev, task_set, execution_profile_id, snapshots, seeds, outcomes, artifacts }
```

IDs are UUIDv7; content uses SHA-256/BLAKE3 hashes; timestamps UTC RFC 3339. Compatibility keys deliberately exclude personal identifiers. Validation prevents `ExecutionProfile` references to incompatible calibration and prevents an evaluation from losing corpus/verifier provenance.

`ExecutionProfile.budgets` is free-form JSON on the wire and is parsed into a typed `ExecutionBudgets { max_actions, edit_verify_cycles, context_retries }` before use, which is what the action loop and the recovery budget are drawn from. It used to be read loosely at the call site, so recovery ran on a default constructed on the spot and the profile's budgets bound nothing. A profile whose budgets do not parse, or whose action or edit-verify budget is zero, is rejected rather than defaulted.

`RuntimeSnapshot.loaded_models` is populated from the backend rather than left empty, and is an input to profile selection, not only a record beside it.

Artifacts are content-addressed and a write refuses to replace one that exists. A model definition was previously stored under its digest alone, so re-inspecting the same deployment minted a fresh id and overwrote the evidence an earlier probe artifact referred to.

Not enforced: a persisted artifact's `schema_version` is never compared against `SCHEMA_VERSION`, so an artifact from another version deserialises if its shape happens to fit.
