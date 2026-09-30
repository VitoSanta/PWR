# Feature status

What exists, what state it is in, and what the plan does with it. Checked
against `develop` at `0776ff4f` on 2026-09-30. Classes follow the review's
feature audit (§14): **KEEP**, **IMPROVE**, **SIMPLIFY**, **MERGE**,
**REMOVE** (from the default product), **EXPERIMENTAL**, **MISSING**. Status
words are defined in [MASTER_SPEC](../MASTER_SPEC.md#evidence-vocabulary).

| Feature | Class | Status | Where | Plan |
|---|---|---|---|---|
| MLX engine with managed lifecycle (install, load, template, prompt cache, reasoning budgets) | KEEP | IMPLEMENTED | `crates/pwr-mlx`, `crates/pwr-mlx/sidecar/pwr_mlx.py`, `apps/desktop/src-tauri/src/engine.rs` | W5.1, W5.3, W9.2 |
| Precise editing with hashes, diffs, revert with conflict detection | KEEP | IMPLEMENTED; rewrites bound to the version read (W1.1); atomic, re-checked writes (W1.2) | `crates/pwr-tools/src/lib.rs`, `atomic.rs`; `converse.rs` `own_overwrite`; `_pwr/revert` | — |
| Repository checks and baseline before/after | KEEP | IMPLEMENTED | `crates/pwr-verify` | W3 |
| Streaming, Stop, steering | KEEP | IMPLEMENTED | `converse.rs`; `_pwr/steer`, `session/cancel` | W5.1 (Stop during prefill) |
| Search, windowed read, dependency source read | KEEP | IMPLEMENTED | `search` with `in_dependencies`, `read_file` windows | — |
| Sandbox and approvals (three permission modes) | IMPROVE | IMPLEMENTED, asymmetric | `sandbox_profile`, `refuse_if_protected`; `PermissionMode` | W1.3, W1.9, W7.4 |
| Goal mode acceptance contract | IMPROVE | IMPLEMENTED, incomplete | `acceptance_contract_hash`, `verify_goal` | W3.1, W1.4, W1.5 |
| Persistence and resume (`--continue`, app sessions, rewind) | IMPROVE | IMPLEMENTED | `conversation.rs`, `serve.rs` | W6.4 |
| Model Manager, downloads, fit rating | IMPROVE | IMPLEMENTED | `crates/pwr-models`, `model-manager.ts` | W7.7, W2.5 |
| Budgets and recovery detectors | SIMPLIFY | IMPLEMENTED, many independent limits | `converse.rs:60-131`, `repetition.rs`, `stall.rs` | W2.6 |
| Selection and certification (`pwr models select/certification/certify`, Verified registry) | SIMPLIFY | IMPLEMENTED; registry empty | `crates/pwr-cli/src/selection.rs`, `compatibility.rs`, `verified-models.json` | W10.1 |
| Project wiki (overview, work log, graph, summaries) | SIMPLIFY | IMPLEMENTED | `crates/pwr-orchestrator/src/wiki.rs`, `graph.rs` | W10.2, W5.2 |
| Scripted runner vs conversation turn | MERGE | Two loops | `lib.rs:2936` vs `converse.rs:829` | W2.3, W2.4 |
| Ledger, checkpoint, task state, snapshots | MERGE | IMPLEMENTED, overlapping | `run_state.rs`, `conversation.rs`, `session.rs` | W6.2, W6.3 |
| 3D knowledge graph view | REMOVE (from default) | IMPLEMENTED | `apps/desktop/src/app/ui/workbench/knowledge.ts` | W7.2 |
| Automatic module summaries after each turn | REMOVE (as default) | IMPLEMENTED, on | `summarise_while_idle`, `serve.rs:2781` | W5.2 |
| Permissive campaign comparison as default | REMOVE (as default) | IMPLEMENTED | `pwr_eval::compare` | W8.1, W10.3 |
| Semantic retrieval (embedding fusion) | EXPERIMENTAL | Opt-in (`PWR_SEMANTIC_RETRIEVAL=1`) | `crates/pwr-cli/src/semantic.rs` | Later (W4.7) |
| `look_at` and image input | EXPERIMENTAL | Offered to vision models only | `look_at_tool`, `converse.rs:649` | Later |
| Evidence-state compaction | EXPERIMENTAL | Scripted loop only (`--context-policy`) | `crates/pwr-orchestrator/src/evidence.rs` | W4.7 |
| Goal review by the same model | EXPERIMENTAL | IMPLEMENTED, one round | `review_prompt`, `serve.rs` | Kept, labelled as an opinion, not verification |
| llama.cpp / GGUF | EXPERIMENTAL | CLI only; server started per generation | `crates/pwr-llama` | Later |
| Quick Calibration | KEEP (as a compatibility smoke test) | IMPLEMENTED, nine requests | `crates/pwr-models/src/calibration.rs` | Not a capability predictor |
| Exact token preflight | MISSING | — | — | W4.2 |
| Global budget (time, recoveries) | MISSING | — | — | W1.4, W2.6 |
| Frozen verifier artifacts | MISSING | — | — | W3.1 |
| One structured outcome | MISSING | Planned in the 2026-09-12 contract, not built | — | W2.1 |
| Comparative benchmark on the product path | MISSING | — | — | W8 |
| Retrievable large tool output | MISSING | Output is bounded and hashed; bytes past the bound are dropped | — | W4.4 |
