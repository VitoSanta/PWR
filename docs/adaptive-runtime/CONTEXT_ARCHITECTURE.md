> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Context architecture

The effective budget is `min(native model, backend limit, safe calibration,
resource budget, user cap)` then divided by task policy into reply reserve,
system/template, task, repository working set, history and tool results. It is
not a universal `num_ctx`. Current `context::compile` already records section
cost/cuts and `run_state` bounds tool results; CLI currently owns section
assembly and static chat choices.

Build a repository map first, retrieve targeted excerpts, maintain a working
set of changed/read files and plan/verification facts, then compact oldest
observations into auditable summaries. Preserve task, constraints, current
plan, changed-file hashes and failing diagnostics as protected facts. On
overflow: shrink retrieval, compress tool result, compact history, lower to a
measured tier, then stop with evidence. Adapter/tokenizer estimates are inputs
to packing; backend-reported tokens correct future estimates.
