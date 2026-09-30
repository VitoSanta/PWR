> **Historical record, superseded as current design on 2026-09-12.** The body is kept as evidence of the design it records; it is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md).

# PWR product evolution

## Decision

PWR evolves from an experimental macOS-first/Ollama-first coding agent tuned
around a small set of 20B--35B deployments into an **adaptive local agent
runtime**. The product is the harness: it selects and drives a local deployment
according to measured hardware, backend, model and task evidence. A model name
is never an agent-core branch.

This is an architectural direction, not an approved runtime rewrite. The
existing verified Ollama workflow remains the baseline.

## Why

Today `README.md` positions a 30B model at 32K context on Apple Silicon.
`strategies/models.json` instead carries seven exact tags, and `main.rs`
constructs `OllamaProvider` directly in chat, doctor, inspection, evaluation,
calibration and run paths. This gives good experimental control but makes
portable selection, a smaller model, and a second backend product work rather
than configuration.

## Product promise and boundaries

One coherent CLI experience must scale from a constrained CPU/laptop profile
to a high-memory workstation. Hardware tier describes the experience; a model
is a replaceable deployment. `Auto`, `Fast`, `Balanced`, `Quality` and `Max
Quality` are user intent, not hardware tiers. Offline operation is mandatory;
telemetry stays optional, transparent and off by default.

Non-goals for the first migration: cloud SaaS, model downloading, a plugin
market, distributed agents, a multi-model swarm, fine-tuning and an IDE.
Future multi-model routing is preserved by making a selection an immutable
per-task decision, not by implementing concurrent model roles now.

## Principles

1. Measure and label evidence; do not infer quality from parameter count.
2. Keep policy separate from discovery and wire-protocol translation.
3. Retain current safety, audit, deterministic verification and Ollama support.
4. Prefer one concrete interface with two consumers to speculative frameworks.
5. All fallbacks explain why they occurred and what evidence is missing.

## Strategic effect

The target user broadens from an enthusiast with enough memory for a ~30B
deployment to a developer who wants local coding assistance on any supported
machine. Autonomy, context and speed scale honestly with evidence rather than
pretending that every device offers the same agent.
