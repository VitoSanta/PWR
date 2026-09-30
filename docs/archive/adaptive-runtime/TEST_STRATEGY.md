> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Test strategy

Unit-test contracts, provenance validation, selector scores/precedence,
resource budgeting, adapter rendering/parsing and context packing. Use fake
backends for deterministic discovery, streams, errors, metrics and malformed
responses; they must implement the same public contract as Ollama.

Integration-test Ollama against local HTTP fixtures for tags/show/chat/stream,
cancellation and errors; retain existing `pwr-ollama` tests. Test the
runtime with fake backend + real temporary workspace/tool/verification policy,
including no verifier, stale edit, loop detection, compaction, context tier
retry and failed test recovery. Existing orchestrator tests are the baseline.

Behavioral and benchmark tests run frozen task suites with artifacts and
intervals; they are not ordinary CI pass/fail model-quality tests. Contract
tests assert every certified profile capability against its adapter/backend.
Cross-platform CI tests parsers and fixtures without needing GPUs/models;
hardware-in-the-loop calibration is scheduled/opt-in. Every migration phase
adds request/event compatibility fixtures plus a rollback test.

## Conceptual compatibility matrix

| Capability | Qwen profile A | Qwen profile B | Future family |
|---|---|---|---|
| Tools / structured output / thinking / parallel tools | to-benchmark | to-benchmark | unknown |
| Long context / editing / planning / recovery | to-benchmark | to-benchmark | unknown |

No value is asserted until model+backend+adapter evidence exists.
