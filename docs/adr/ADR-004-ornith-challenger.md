> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# ADR-004: Ornith challenger

**Status:** Accepted provisionally. **Decision:** `ornith-1.5:35b` is the first comparable challenger.

It runs the same frozen task/evaluation protocol as Qwen3.8. PWR will not tune benchmark-specific prompts after inspecting hidden test data. Consequence: model differences are reported with profiles, deployment fingerprints, and raw outcome counts.
