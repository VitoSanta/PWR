> **Historical record, superseded as current design on 2026-09-12.** The body is kept as evidence of the design it records; it is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md).

# Vision and Scope

PWR makes local coding agents dependable enough to be evaluated as engineering systems. The target user supplies a repository and a task; PWR selects a bounded configuration, makes inspectable changes, and returns verification evidence.

## In scope (MVP)

Ollama on the local host; macOS-first discovery with portable interfaces; text/tool-capable local models; repository indexing; tool-calling loop; diff-aware edits; commands in a sandbox policy; build/test/lint verification; calibration and benchmark datasets.

## Explicitly out of scope

Multi-tenant hosting, remote providers, autonomous long-running background work, hidden chain-of-thought storage, browser control, unbounded shell access, cross-model automatic routing, and distributed execution. Each requires separate threat modelling and evidence.

## Success criteria

For a locked task corpus and machine snapshot, PWR must reproduce profile selection, command policy, and recorded outcome. Improvements require statistically reported evaluation deltas with identical or versioned inputs.
