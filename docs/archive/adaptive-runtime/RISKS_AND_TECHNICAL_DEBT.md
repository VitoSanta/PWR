> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Risks and technical debt

| Risk | Impact | Probability | Mitigation |
|---|---|---|---|
| Adapter changes alter Ollama prompts/calls | High | High | wire fixtures, dual path, corpus gate |
| Abstraction becomes framework | High | Medium | introduce only two concrete consumers (Ollama/generic, then Qwen) |
| Profile/config ambiguity | High | High | provenance + precedence + immutable resolved selection |
| Hardware probe lies/varies by OS | High | Medium | unknown fields, conservative budgets, fixture probes |
| Benchmark overfits corpus | High | High | diverse frozen suites, category reporting, regression gates |
| Backend semantics diverge | High | Medium | backend contract tests and normalized errors/metrics |
| Context regression | High | High | compaction fidelity and retained-fact assertions |
| Certification goes stale | Medium | High | version scope/demotion on digest/backend/harness change |
| CLI refactor disrupts safety | High | Medium | retain policy/lease/audit integration tests |
| Platform sandbox gap | High | Current | do not claim parity; document/refuse unconfined mode |
