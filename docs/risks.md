# Risks

The ten principal risks, from the [review of 2026-09-30](reviews/2026-09-30-technical-review.md)
(§25), each checked against the code ([verification](reviews/2026-09-30-verification.md))
and tied to the plan item that mitigates it. Likelihoods are qualitative, for
the intended use; none is a measured rate. Reviewed when a plan gate passes.

| # | Risk | Likelihood | Impact | Evidence (baseline `4ae7c5f1`; state at `aa1d0707`, 2026-10-01) | Mitigation | State |
|---|---|---|---|---|---|---|
| 1 | **False acceptance** — a goal reported verified after its tests were weakened | High | Critical | Goal acceptance hashed `.pwr/checks.json` only; the artifacts it runs are now frozen too (declared or inferred), a changed one stops the goal, even under Full access; a dependency chain conventions cannot see is still not covered | W3.1, W1.3; W8.4 counts false acceptance | Partly mitigated (local, CI not run) |
| 2 | **Damage to the person's work** — a stale rewrite overwrites a newer edit | Medium-high | Critical | Overwrites are now bound to the version the conversation read (W1.1, done); writes go through a temporary file and a re-checked move (W1.2, done); the instant between the re-check and the move remains | — | Mitigated (W1.1, W1.2) |
| 3 | **Asymmetric policy** — commands change what file tools may not | High when scripts run | Critical | Protected paths and dependency trees are now in the Seatbelt profile (W1.3); Full access still has no profile, by design, and says so in the outcome | W1.3; W1.9 disclosure; W2.1 `confinement` in the outcome | Mitigated outside Full access (local) |
| 4 | **Loss of the specification** in long sessions | High on long sessions | High | Compaction used to keep 800/200 characters of requests; the objective and its revisions are now carried whole (W4.1) | W4.1 | Mitigated (local) |
| 5 | **Costly loops or wrong stops** | Medium-high | High | W1.4 bounds every goal branch; failures are compared by fingerprint (W1.5); but a model that rewrites one file seventy times in an hour is only told (a note every 12th write), not stopped | W2.6 recovery budget | Partly mitigated |
| 6 | **Unrepresentative benchmark** | High | Critical for investment | Scripted and product loops differ; the stack-matrix runner nudges after a failure | W2.4 one path; W8.2 interventions reported; W8.4 protocol | Open |
| 7 | **Latency and memory pressure on consumer Macs** | High on long tasks | High | Prefill is cancellable and summaries are off by default (W5.1, W5.2); the memory reserve is still arithmetic (W5.3); **new, measured 2026-10-01**: a second engine beside the first overruns the GPU working set (about 55 GB of 64 GB) and both emit nonsense; switching model in a long conversation re-reads the whole prompt (minutes) | W5.3, W5.5; a warning before a switch; W8.7 stress | Partly mitigated |
| 8 | **Maintenance debt** | High | High | `main.rs` 13,562 lines, `serve.rs` 6,308, orchestrator `lib.rs` 8,902, `converse.rs` 3,989 (2026-10-01); overlapping state; many toolchains and families | W2.3, W10.4; W6.2 ownership map; W10.1–W10.2 less surface | Open |
| 9 | **Insufficient differentiation** | High | Critical for the product | Local models, repo maps and permissions exist elsewhere (Aider, OpenCode); uplift inconclusive | A precise niche ([MASTER_SPEC](../MASTER_SPEC.md)); W8.4 measured advantage | Open |
| 10 | **Open-source sustainability** | High without simplification | High | Rust + Angular + Tauri + a Python engine; private evidence; documents that disagreed | This rewrite (W0.1); W8.5 public evidence; W9 reproducible releases | Partly addressed (documents) |

## Risks the review did not list

(Added 2026-10-01 from the model-compatibility and long-context work.)

| Risk | Evidence | Mitigation |
|---|---|---|
| **Full access is chosen for convenience and forgotten** — every protection off, checks unconfined | `PermissionMode::Full` → `SandboxPolicy::FullAccess` (`crates/pwr-cli/src/main.rs`) | W7.4 confinement on every turn; W3.1 acceptance still checked by hash; W1.9 text at the selector |
| **A test suite that "passes" without exercising the sandbox** on a machine missing Docker, .NET or a browser | Early `return`s in host-dependent tests | W0.2 skip reporting |
| **Prompt overflow after a large tool call** | Tool-call arguments uncounted in the running estimate (`converse.rs`) | W4.2 |
| **A model's own call format is not read, and the model is refused as "Limited"** — a defect of PWR taken for the model's | Measured 2026-10-01: Qwen3-Coder, Devstral, LFM2, Granite and Gemma 4 were Limited for PWR's reasons; all 22 installed models pass the critical checks now | One adapter per family; a verdict records the adapter that read it and is void when it changes; `tool_selection` asks for a valid call, not a particular one |
| **Silent degradation of a long conversation** — the model collapses into "!!!!" or a repeated phrase | A reported KV-cache corruption near 60k tokens on the pinned engine; PWR's own collapse at 27k (Qwen3-Coder) | A clean start after two collapsed replies; an engine upgrade after checks; a workspace compaction ceiling (`compact_ceiling_tokens`) — all unmeasured. The 32k default ceiling was withdrawn on 2026-10-02 ([D-2026-10-02-3](decisions.md)), so long conversations now reach 75 % of the window |
| **The engine's libraries are pinned at versions with known upstream defects** | See [distribution.md](distribution.md) | W9.2 |
