> **Historical record, superseded as current design on 2026-09-12.** The body is kept as evidence of the design it records; it is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md).

# Architectural decisions

| ADR | Context / decision | Alternatives / consequence | Status |
|---|---|---|---|
| AD-01 model-agnostic core | Core consumes capability contracts, never family/tag conditionals. | Branch names in loop; adapters/profiles add a small boundary. | Proposed |
| AD-02 backend abstraction | Extend current `ModelProvider` into transport/lifecycle discovery contracts. | One giant provider trait; keep contracts narrow. | Proposed |
| AD-03 capabilities over names | Measured/declarative capability state drives policy. | Parameter-count or family inference; uncertainty is explicit. | Proposed |
| AD-04 Qwen first class | First family adapter is Qwen, preserving current tags as evidence only. | Immediate many-family support; validates boundary cheaply. | Proposed |
| AD-05 profiles separate evidence | Vendor, backend, benchmark and PWR policy remain distinct. | One mutable JSON profile; provenance remains auditable. | Proposed |
| AD-06 tiers are UX | Tier derives from feasible measured experience. | RAM/parameter thresholds; avoids false promises. | Proposed |
| AD-07 canonical protocol | Typed actions/messages replace raw transport JSON at core boundary. | DSL; no new language is introduced. | Proposed |
| AD-08 certification | Badges require versioned benchmark gates and can regress. | Static allowlist; evidence cost is intentional. | Proposed |
| AD-09 precedence | defaults < certified profile < calibration < task policy < user config < session override. | User intent wins; unsafe overrides still receive admission checks. | Proposed |
