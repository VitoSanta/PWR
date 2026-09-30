> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Model profile specification

Profiles are versioned, immutable evidence documents keyed by stable model
identity (digest where possible), backend deployment and adapter version; they
must not use a tag as their sole identity. Current `ModelProfile`,
`ModelDefinition`, `CalibrationProfile` and `ParameterSource` provide useful
provenance fields but should be split rather than enlarged indefinitely.

```yaml
profile_version: 2
id: qwen/example
family: qwen
identity: {vendor_model: unknown, deployment_selectors: [qwen-example], digest: null}
declared: {architecture: unknown, native_context: unknown, vendor_capabilities: {}}
backend_observed: {backend: ollama, context_limit: unknown, tool_protocol: native}
pwr_policy: {adapter: qwen-v1, defaults: {temperature: {value: 0.7, source: pwr_policy}}}
benchmark: {suite_version: null, status: experimental, capability_confidence: {tool_use: to-benchmark}}
compatibility: {structured_output: partial, thinking: unknown, parallel_tools: unknown}
context: {tested: [], recommended: null, safe: null}
execution: {preferred_edit_strategy: replace_text, max_files: null}
versions: {PWR: 0.1.0, adapter: qwen-v1, benchmark: null}
```

`declared` is vendor evidence; `backend_observed` comes from live inspection;
`benchmark` is empirical and hardware/backend scoped; `pwr_policy` is an
explicit product decision. Unknown must remain unknown. Calibration belongs in
a separate device/deployment artifact and references this profile. Validation
rejects ambiguous provenance, stale schema and a certified claim without a
benchmark reference.
