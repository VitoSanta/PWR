# Risks

The ten principal risks, from the [review of 2026-09-30](reviews/2026-09-30-technical-review.md)
(§25), each checked against the code ([verification](reviews/2026-09-30-verification.md))
and tied to the plan item that mitigates it. Likelihoods are qualitative, for
the intended use; none is a measured rate. Reviewed when a plan gate passes.

| # | Risk | Likelihood | Impact | Evidence at `0776ff4f` | Mitigation | State |
|---|---|---|---|---|---|---|
| 1 | **False acceptance** — a goal reported verified after its tests were weakened | High | Critical | Goal acceptance hashes `.pwr/checks.json` only (`crates/pwr-cli/src/serve.rs:723`); the tests it runs are protected only if listed in `.pwr/protected.json` | W3.1 frozen acceptance artifacts; W1.3 protection for commands; W8.4 counts false acceptance | Open |
| 2 | **Damage to the person's work** — a stale rewrite overwrites a newer edit | Medium-high | Critical | Overwrites are now bound to the version the conversation read (W1.1, done); writes go through a temporary file and a re-checked move (W1.2, done); the instant between the re-check and the move remains | — | Mitigated (W1.1, W1.2) |
| 3 | **Asymmetric policy** — commands change what file tools may not | High when scripts run | Critical | Protected paths and dependency trees are not in the Seatbelt profile (`crates/pwr-tools/src/lib.rs:2092-2375`); Full access has no profile at all | W1.3; W1.9 disclosure; W2.1 `confinement` in the outcome | Open |
| 4 | **Loss of the specification** in long sessions | High on long sessions | High | Compaction keeps 800/200 characters of requests (`compaction.rs:48-54`) | W4.1 objective kept verbatim | Open |
| 5 | **Costly loops or wrong stops** | Medium-high | High | Goal budget checked on one branch only (`serve.rs:2182`); failures compared by check name | W1.4 goal budget; W1.5 fingerprints; W2.6 recovery budget | Open |
| 6 | **Unrepresentative benchmark** | High | Critical for investment | Scripted and product loops differ; the stack-matrix runner nudges after a failure | W2.4 one path; W8.2 interventions reported; W8.4 protocol | Open |
| 7 | **Latency and memory pressure on consumer Macs** | High on long tasks | High | Uncancellable prefill; background summaries after every turn; a heuristic memory reserve | W5.1, W5.2, W5.3; W8.7 stress | Open |
| 8 | **Maintenance debt** | High | High | `main.rs` 13,196 lines, `serve.rs` 6,131, orchestrator `lib.rs` 8,731; overlapping state; many toolchains and families | W2.3, W10.4; W6.2 ownership map; W10.1–W10.2 less surface | Open |
| 9 | **Insufficient differentiation** | High | Critical for the product | Local models, repo maps and permissions exist elsewhere (Aider, OpenCode); uplift inconclusive | A precise niche ([MASTER_SPEC](../MASTER_SPEC.md)); W8.4 measured advantage | Open |
| 10 | **Open-source sustainability** | High without simplification | High | Rust + Angular + Tauri + a Python engine; private evidence; documents that disagreed | This rewrite (W0.1); W8.5 public evidence; W9 reproducible releases | Partly addressed (documents) |

## Risks the review did not list

| Risk | Evidence | Mitigation |
|---|---|---|
| **Full access is chosen for convenience and forgotten** — every protection off, checks unconfined | `PermissionMode::Full` → `SandboxPolicy::FullAccess` (`crates/pwr-cli/src/main.rs:1106-1124`) | W7.4 confinement on every turn; W3.1 acceptance still checked by hash; W1.9 text at the selector |
| **A test suite that "passes" without exercising the sandbox** on a machine missing Docker, .NET or a browser | Early `return`s in host-dependent tests | W0.2 skip reporting |
| **Prompt overflow after a large tool call** | Tool-call arguments uncounted in the running estimate (`converse.rs:2448-2559`) | W4.2 |
