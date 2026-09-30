# Architecture

**Checked against `develop` at `0776ff4f`, 2026-09-30.** What the code is, not
what it should become; the changes are in the
[implementation plan](plan/implementation-plan.md) and referred to by item.

## Shape

PWR is **a Mac desktop app that drives a local agent through a Rust process**,
with inference in a Python process PWR starts and owns. It is not a
distributed system: the concurrency that matters is between processes,
streams, sessions and the local resources they share (one engine, one
workspace, one event log).

```text
┌──────────────────────── PWR.app (Tauri 2) ────────────────────────┐
│ Angular UI (signals)  ── bridge.ts ──  Rust shell (src-tauri)      │
│   conversation, diffs, permissions,      starts/stops the core,    │
│   workbench, Model Manager               relays JSON-RPC, terminal,│
│                                          engine installer          │
└───────────────────────────────┬───────────────────────────────────┘
                                │ stdin/stdout, JSON-RPC (ACP + _pwr/*)
┌───────────────────────────────▼───────────────────────────────────┐
│ pwr serve --stdio  (crates/pwr-cli)                                │
│   sessions, settings, permission modes, post-turn checks, Goal mode│
│        │                                                           │
│        ▼                                                           │
│ converse::take_turn  (crates/pwr-orchestrator)                     │
│   prompt, compaction, tool calls, policy, recovery detectors       │
│        │                 │                    │                    │
│        ▼                 ▼                    ▼                    │
│   pwr-provider      pwr-tools           pwr-verify                 │
│   pwr-runtime       (policy, Seatbelt,  (check discovery,          │
│   pwr-compat         files, commands,    baseline, classification) │
│        │             services, fetch)                              │
│        ▼                                                           │
│   pwr-mlx ──── stdin/stdout JSON lines ──► pwr_mlx.py (MLX sidecar)│
│   pwr-llama ── HTTP 127.0.0.1 ──────────► llama-server (CLI only)  │
│                                                                    │
│   pwr-store (SQLite event log)  pwr-repo (index)  pwr-models       │
└────────────────────────────────────────────────────────────────────┘
```

## Components

| Component | Responsibility | Size (lines, incl. tests) |
|---|---|---|
| `apps/desktop` (Angular) | Conversation, models, permissions, diffs, workbench, metrics. Never writes to a workspace itself. | — |
| `apps/desktop/src-tauri` | Finds and starts the core, relays JSON-RPC, terminal tabs, the engine installer, workspace trust. Commands: `core_start`, `core_send`, `core_stop`, `engine_status`, `engine_install`, `engine_cancel`, `term_*`, `workspace_is_trusted`, `trust_workspace`, `default_workspace`, `chat_home_path`, `open_external`, `debug_export_chat` | — |
| `pwr-cli` | The `pwr` binary: command line, `pwr serve`, the chat console, configuration, sessions, permission modes, **post-turn verification and Goal mode** | 23,159 |
| `pwr-orchestrator` | The conversation turn (`converse.rs`), the scripted loop (`lib.rs`), B0/B2 baselines, context compilation, compaction, window arithmetic, wiki and graph, personal memory | 20,937 |
| `pwr-tools` | Tool policy, Seatbelt profile, file tools, commands, services, fetch, documents, screenshots | 9,123 |
| `pwr-models` | Hugging Face catalogue, fit rating, verified downloads, model profiles, Quick Calibration | 6,758 |
| `pwr-eval` | Corpora, materialisation, scoring, campaigns, comparisons, thresholds | 4,278 |
| `pwr-domain` | Versioned domain types: messages, actions, events, deployments, reasoning, outcomes | 3,006 |
| `pwr-mlx` | The MLX engine's Rust side: sidecar process, protocol, inspection, embeddings | 2,413 (+ 1,063 Python) |
| `pwr-verify` | Check discovery, execution, baseline comparison, failure classification | 1,730 |
| `pwr-llama` | GGUF inspection, a managed `llama-server`, streaming | 1,680 |
| `pwr-compat` | Family conventions for tool calls and reasoning, normalised to canonical actions | 1,635 |
| `pwr-runtime` | Backend selection and host observation; no task or policy logic | 1,426 |
| `pwr-repo` | Incremental repository inventory, shallow symbols and imports, lexical retrieval | 1,191 |
| `pwr-provider` | The model boundary: generation, streams, cancellation, metrics | 637 |
| `pwr-observe` | Export of a run's log and reports read back from it | 607 |
| `pwr-store` | SQLite migrations and the append-only, hash-chained event log | 375 |

The Cargo workspace has 15 members (`Cargo.toml`). `unsafe_code` is forbidden
workspace-wide.

## The production path

1. The app sends `session/prompt` (with `goalMode` true or false) to `pwr serve`.
2. `serve.rs` resolves the session, its workspace configuration and permission
   mode, and hands the prompt to the **session executor**
   (`pwr_orchestrator::executor::execute`, policy *conversation* or *goal*),
   with itself as the executor's host (`SessionHost`): the model turn, the
   verification, the review, what to say and the messages to keep.
3. The host's turn (`run_chat_turn` in `main.rs`) builds the catalogue (the
   conversation catalogue, the chat-only one without a workspace, or with
   `look_at` for a vision model), then calls `converse::take_turn`.
4. `take_turn` loops: compact if the prompt nears the threshold, generate
   through the provider, parse and normalise tool calls (`pwr-compat`), check
   each against policy, execute it (`pwr-tools`), append its result, until the
   model answers in prose, calls `complete`, or a detector stops the turn.
5. Back in `run_chat_turn`, if the turn edited files, the front end prepares
   the checks' policy and hands them to `executor::close_turn`, which runs
   them and puts the verdict in the answer, the model's next prompt and the
   turn's typed outcome.
6. In Goal mode the executor loops around steps 3–5: on a `complete` it runs
   the full verification, once runs a review round, and either ends verified
   or sends the model back with the evidence, inside one goal budget. It
   returns a `SessionEnd` (reply, stopped, out of budget, error); `serve.rs`
   turns that into the ACP response.

## Three execution semantics

The same model and tools run under three different loops. This is the
architecture's main problem (review §2, §3.2; plan W2).

| | Conversation turn | Goal mode | Scripted run (`pwr run`, `eval run`) |
|---|---|---|---|
| Code | `converse::take_turn` + `run_chat_turn`, sequenced by `executor::execute` | `executor::execute` (goal policy) around the turn | `run_action_loop_with_prompt_budget_and_context_tiers`, `orchestrator/src/lib.rs:2936` |
| Verification | `executor::close_turn` after the turn; the turn is already over | full verification on each `complete`, acceptance contract, one review round | inside the loop: baseline, checks on completion, recovery cycle |
| Completion | `complete` held once if unseen results or nothing done | verified only with a declared, unchanged acceptance check | `verified: false` allowed when no verifier exists |
| Compaction | mechanical record (`compaction.rs`) | same | ledger compaction; optional `recency-fill` / `evidence-state` policies |
| Catalogue | no `record_progress`, `propose_verifier`; adds `remember`, `recall_project`, `wiki_query`, `look_at` | same | the full action catalogue |
| Used by | the app, `pwr chat` | the app | the command line and every campaign |

Every campaign so far measured the third column; the app ships the first two.

## Dependencies that do not follow responsibility

- `pwr-models` depends on `pwr-orchestrator` for the window arithmetic
  (`crates/pwr-models/src/fit.rs:30`). Plan W2.5 moves the arithmetic.
- The scripted loop still sequences its own verification and completion
  (`orchestrator/src/lib.rs`); plan W2.4 converges it onto the executor. The
  executor itself (`crates/pwr-orchestrator/src/executor.rs`) now owns the
  conversation policy, the goal loop and the checks that close a turn.

## State

Workspace state lives in `<workspace>/.pwr/`, personal state in `~/.pwr/`
(or `PWR_HOME`), engine state under
`~/Library/Application Support/ai.pwr.desktop/engine`. What each file is, and
which of them decide behaviour, is in
[state-and-persistence.md](state-and-persistence.md).

## Where to read next

[agent-loop.md](agent-loop.md) (the turn and Goal mode) ·
[context.md](context.md) · [tools-and-sandbox.md](tools-and-sandbox.md) ·
[verification.md](verification.md) · [inference.md](inference.md) ·
[models.md](models.md) · [desktop.md](desktop.md) · [pwr-serve.md](pwr-serve.md)
