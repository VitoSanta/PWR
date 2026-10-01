# Feature status

What exists, what state it is in, and what the plan does with it. Checked
against `develop` at `bff93062` on 2026-10-01. Classes follow the review's
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
| Goal mode acceptance contract | IMPROVE | IMPLEMENTED, incomplete | `acceptance_contract_hash`, `verify_goal` | W3.1, W1.5 |
| Persistence and resume (`--continue`, app sessions, rewind) | IMPROVE | IMPLEMENTED | `conversation.rs`, `serve.rs` | W6.4 |
| Model Manager, downloads, fit rating | IMPROVE | IMPLEMENTED | `crates/pwr-models`, `model-manager.ts` | W7.7, W2.5 |
| Budgets and recovery detectors | SIMPLIFY | IMPLEMENTED, many independent limits | `converse.rs`, `repetition.rs`, `stall.rs` | W2.6 |
| Selection and certification (`pwr models select/certification/certify`, Verified registry) | SIMPLIFY | IMPLEMENTED; registry empty | `crates/pwr-cli/src/selection.rs`, `compatibility.rs`, `verified-models.json` | W10.1 |
| Project wiki (overview, work log, graph, summaries) | SIMPLIFY | IMPLEMENTED | `crates/pwr-orchestrator/src/wiki.rs`, `graph.rs` | W10.2, W5.2 |
| Scripted runner vs conversation turn | MERGE | The app, the console and Goal mode run one executor (`executor.rs`); the scripted loop (`pwr run`, `eval run`) still does not | `executor.rs`, `lib.rs` | W2.4 (open); [executor-parity.md](plan/executor-parity.md) |
| Ledger, checkpoint, task state, snapshots | MERGE | IMPLEMENTED, overlapping | `run_state.rs`, `conversation.rs`, `session.rs` | W6.2, W6.3 |
| 3D knowledge graph view | REMOVE (from default) | IMPLEMENTED: outline default, experimental 3D opt-in | `apps/desktop/src/app/ui/workbench/knowledge.ts` | W7.2 |
| Automatic module summaries after each turn | REMOVE (as default) | IMPLEMENTED: off by default, opt-in and pre-emptible | `summarise_while_idle`, `serve.rs` | W5.2 |
| Permissive campaign comparison as default | REMOVE (as default) | IMPLEMENTED: strict default, legacy explicitly noncausal | `pwr_eval::compare` | W8.1, W10.3 |
| Semantic retrieval (embedding fusion) | EXPERIMENTAL | Opt-in (`PWR_SEMANTIC_RETRIEVAL=1`) | `crates/pwr-cli/src/semantic.rs` | Later (W4.7) |
| `look_at` and image input | EXPERIMENTAL | Offered to vision models only | `look_at_tool`, `converse.rs` | Later |
| Evidence-state compaction | EXPERIMENTAL | Scripted loop only (`--context-policy`) | `crates/pwr-orchestrator/src/evidence.rs` | W4.7 |
| Goal review by the same model | EXPERIMENTAL | IMPLEMENTED, one round; since 2026-10-01 also when only technical checks pass and no acceptance is declared (the common case). 3/8 → 6/8 on one deployment, one trial per task: a signal, not a result | `review_prompt`, `executor.rs` | Kept, labelled as an opinion, not verification; to be measured (W8) |
| llama.cpp / GGUF | EXPERIMENTAL | CLI only; server started per generation | `crates/pwr-llama` | Later |
| Quick Calibration | KEEP (as a compatibility smoke test) | IMPLEMENTED, nine requests, suite `quick-calibration-6`: a model passes when it makes one valid call to an offered tool and uses a result; which tool it reaches for is a non-critical check. A verdict records the adapter that read the replies | `crates/pwr-models/src/calibration.rs` | Not a capability predictor |
| Exact token preflight | MISSING | — | — | W4.2 |
| Goal-wide budget | KEEP | IMPLEMENTED (W1.4, working tree): actions, refusals, verification/review caps, wall-clock | `GoalBudget`, `GoalLimits` in `executor.rs`; workspace `goal_budget` | — |
| Shared recovery budget | MISSING | Independent recovery limits remain | — | W2.6 |
| Frozen verifier artifacts | KEEP | IMPLEMENTED (working tree): persisted hashes, per-file authorization, before/after check validation | `pwr-verify::acceptance`, `Checkpoint`, `verify_goal` | W3.1 |
| One structured outcome | IMPROVE | PARTIAL: conversation/ACP/UI migrated; scripted runner and CLI JSON pending | `pwr-domain::TurnOutcome` | W2.1 |
| Comparative benchmark on the product path | MISSING | — | — | W8 |
| Retrievable large tool output | MISSING | Output is bounded and hashed; bytes past the bound are dropped | — | W4.4 |
| One adapter per model family (Qwen, GLM, Seed, Gemma 4, gpt-oss, Granite, Mistral/Devstral, Liquid/LFM2) | KEEP | IMPLEMENTED; all 22 models installed on the maintainer's Mac pass the critical calibration checks (2026-10-01) | `crates/pwr-compat` | W10.1 |
| Sampling resolution: user → declared profile → card (incl. the coding set of a per-mode card) → `generation_config.json` → a floor of 0.6 / 0.95 / 20 → never greedy by default | KEEP | IMPLEMENTED 2026-10-01; each value says where it came from | `crates/pwr-mlx`, `crates/pwr-models/src/sampling.rs` | [D-2026-09-30-7](decisions.md) |
| Recovery from collapsed replies (presence penalty over 1,024 tokens, clean-start compaction, a note on a file rewritten 12 times) | EXPERIMENTAL | IMPLEMENTED 2026-10-01, effect unmeasured | `converse.rs`, `stall.rs` | W8 |
| Prefill progress shown while a first word is awaited | KEEP | IMPLEMENTED 2026-10-01 | `_pwr/model_progress`, `conversation.ts` | W5.5 |
| Stack-matrix runner with hidden tests, verified in Docker; `watch.py` to follow a run | KEEP (research) | IMPLEMENTED | `evidence/stack-matrix/` | W8 |
| Warning before switching model in a long conversation | MISSING | — | — | — |
