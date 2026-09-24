# PWR — Product Narrative and Technical Source of Truth

**Compiled 2026-09-24** from the `stage` branch at commit `ad9eefa4`, plus the
untracked `docs/product-screenshots/` folder, and **updated the same day for
the 0.1.0 release**: engine scripts bundled, Revert through the core, a
webview CSP, models in `~/.pwr/models`, and on a Mac the app runs MLX only. Every claim below was checked
against source code, tests, canonical documents or recorded runs in this
repository. Where no evidence was found the text says `unknown` or
`requires author confirmation`.

Test run while compiling this document (2026-09-24, `cargo test --workspace
--no-fail-fast` on this tree, exit 0): **1,078 passed, 0 failed, 5 ignored**
across 97 test binaries. The readiness assessment earlier the same day
recorded 1,074 passed and 5 ignored.

This document is **source material**, not marketing. It is meant to feed the
website, README, GitHub releases, CV, LinkedIn and later press material
without anyone having to re-derive facts. When this document and the code
disagree, the code wins and this document is wrong and should be fixed.

## How to read the status words

| Word used here | Meaning | Repository equivalent |
|---|---|---|
| **IMPLEMENTED** | A reachable code path exists in the shipped core or app, with tests. It says nothing about model quality or production maturity. | `IMPLEMENTED` in [MASTER_SPEC.md](../MASTER_SPEC.md) |
| **EXPERIMENTAL** | Built, but opt-in, limited, research-only, or not validated end to end. | `PROTOTYPED`, "IMPLEMENTED, OPT-IN", "IMPLEMENTED, LIMITED" in the README |
| **PLANNED** | A selected direction with a backlog item. Not available. | `PLANNED` |
| **VISION** | The long-term destination stated in the canonical documents. Not a commitment to a date or a feature. | MASTER_SPEC "destination" |

"Live" means a feature was exercised once or a few times against a real local
model on the maintainer's machine (Apple M2 Max, 64 GB). No live result in this
repository is a benchmark. Sample sizes are almost always n=1.

---

## 1. Product identity

**Product name:** PWR. The binary and crate prefix are lowercase `pwr`
([glossary](glossary.md#names)). The desktop bundle is `PWR.app`, identifier
`ai.pwr.desktop` ([tauri.conf.json](../apps/desktop/src-tauri/tauri.conf.json)).
The project was called **poorAI / PoorAI** until the rename on 2026-09-24
(commit `323f2b64`). See §30 and the final report for leftovers.

**Category (most accurate):** a local, open-source **coding-agent harness and
desktop app** for Apple-silicon Macs. It runs open-weight models on the
machine it is installed on. It is also a research platform that measures how
much a harness changes what a local model can finish.

### One-liner candidates (≈15 words max)

1. A local coding agent for your Mac: open-weight models, your repository, checks you can see.
2. An open-source harness that lets local models edit, run and verify code on your Mac.
3. A desktop coding agent that runs local models and shows its changes and evidence.
4. Research-grade local agent harness: local models, sandboxed tools, deterministic checks, auditable runs.

### Short description (2–3 sentences)

PWR is an open-source desktop app and Rust core that turns a local,
open-weight language model into a coding agent working inside a repository
you choose. It runs the model on your Mac with its own MLX engine, gives it
typed and sandboxed tools, and checks its work with the repository's own
build and test commands. It is a public alpha and a research project: it
measures what a harness adds to a local model rather than claiming parity with
cloud agents.

### Medium description (one paragraph)

PWR is an experimental local agent harness with a desktop app. You open a
repository, pick a model that runs on your own Mac, and talk to it; the agent
reads, searches, edits files, runs commands and starts local services through
typed tools. Commands run inside a macOS sandbox confined to the workspace,
and actions that reach beyond it (dependency changes, network, toolchain
installs, Git history rewrites, publishing) ask first unless you switch the
workspace to Auto. After it edits, PWR runs the repository's own checks and
reports what they said. Every action is recorded in a local, hash-chained
event log you can report on and diagnose. PWR computes a context window from
your machine's memory and the model's configuration, compacts the
conversation mechanically when it fills, and treats a model it has never seen
as *Provisional*. A nine-request Quick Calibration then marks it locally
calibrated or limited. It is a public alpha for Apple-silicon Macs, built by
one developer, and its central research question is how much a harness adds
to the same model on the same task.

### Technical description (for engineers)

PWR is a 15-crate Rust 2024 workspace (MSRV 1.88, `unsafe_code = "forbid"`)
plus a Tauri 2 / Angular 22 desktop client. The core (`pwr`) exposes a
terminal console, an unattended runner and an evaluator. It also exposes
`pwr serve --stdio`, which speaks the Agent Client Protocol (JSON-RPC 2.0 over
stdio) with `_pwr/*` extensions; the app is a client of that server and holds
no agent logic.

Inference runs in PWR's own engine: a Python sidecar over `mlx-lm`/`mlx-vlm`
that renders chat templates, streams reasoning and answer on separate
channels, enforces a per-generation thinking budget and keeps a prompt cache
across a turn. A llama.cpp path for GGUF (managed `llama-server`) exists but
is limited.

The conversation loop (`pwr_orchestrator::converse`) decodes tool calls
against a typed catalogue and passes each action through a shared
gate/perform step (repetition detection, approvals, audit). Actions execute in
a policy-enforcing executor with a macOS Seatbelt profile, hash-guarded
edits, argv-only commands and supervised services. The loop compacts context
mechanically and bounds silence, unparseable output, backend faults, stalls
and reasoning overruns with named stop reasons.

Verification discovers the repository's own checks (declared → CI →
`package.json` → a 19-entry build-system marker registry → a static web-asset
check), baselines them before the first edit and compares afterwards. Events
live in SQLite with a per-run hash chain. Model handling is
provenance-scoped: evidence is tied to artifact digest, quantization,
template, tokenizer, backend version and hardware class, with deterministic
reuse rules.

---

## 2. What PWR is

**What kind of application.** A standalone desktop application (`PWR.app`)
plus a command-line core (`pwr`). The app is the product surface; the command
line is the development and research tool ([pwr-serve.md](pwr-serve.md),
[current-cli.md](current-cli.md)). PWR is explicitly **not** an editor or IDE
plugin. Editor integration is not a goal, even though ACP editors could in
principle connect ([pwr-serve.md](pwr-serve.md) "Decision").

**Where it runs.** On the user's own Mac with Apple silicon. The model, the
agent loop, the tools, the verification and the event log all run locally.
Windows is a stated product target but is not implemented (see §21).

**Model and runtime ecosystem today.**
- **MLX** models (Hugging Face MLX repositories: `config.json`, safetensors,
  tokenizer, chat template) through PWR's own MLX engine. This is the default
  and the only path used live in the app.
- **GGUF** models through llama.cpp (`pwr --backend llama`). EXPERIMENTAL
  and **command line only**: a release build of the app on macOS always runs
  MLX, and the Model Manager hides GGUF. The server restarts for every
  generation ([backlog R.4](backlog.md)). It is kept for Windows.
- Ollama and LM Studio *servers* were supported until 2026-09-19 and were
  removed (tag `last-with-http-backends`). Models live in `~/.pwr/models`
  (default since 2026-09-24; it was LM Studio's folder, `~/.lmstudio/models`,
  before).

**How the user interacts.** Through a conversation. The user types a
request. They can attach files, folders or images, and they can steer the
agent while it works. The agent answers or acts, and actions appear as
objects with status and diffs. Permission prompts appear when an action needs
an approval the workspace has not granted. The user can stop a turn at any
time.

### Terms, kept distinct

| Term | In PWR |
|---|---|
| **Model** | The open-weight weights plus tokenizer and chat template, in a folder on disk (MLX) or a `.gguf` file. PWR never runs code shipped in a model repository ([model-compatibility.md](model-compatibility.md#security)). |
| **Runtime / backend** | The engine that loads and serves the model: PWR's MLX sidecar ([crates/pwr-mlx](../crates/pwr-mlx/src/lib.rs), [pwr_mlx.py](../crates/pwr-mlx/sidecar/pwr_mlx.py)) or a managed `llama-server` ([crates/pwr-llama](../crates/pwr-llama/src/lib.rs)), behind the `ModelProvider`/`InferenceBackend` contracts ([crates/pwr-provider](../crates/pwr-provider/src/lib.rs)). |
| **Harness** | Everything around the model: prompt and context assembly, tool catalogue, decoding and repair of tool calls, policy and sandbox, execution, verification, compaction, stop rules, audit. This is what PWR is. |
| **Agent** | The model plus the harness, acting in a workspace through a conversation. |
| **Tools** | Typed actions the harness offers and executes (`read_file`, `search`, `replace_text`, `run_command`, …). The model proposes; the harness validates, authorizes and executes ([crates/pwr-tools](../crates/pwr-tools/src/lib.rs)). |
| **Workspace / repository** | The folder the user opens and trusts. Writes are confined to it. PWR keeps its per-workspace state in `<workspace>/.pwr/`. |
| **Context** | What one generation is sent. PWR separates the *window* (what the engine is asked to serve) from what fills it, and reports both ([models-and-context.md](models-and-context.md)). |
| **Verification** | Running the repository's own deterministic checks, independent of the model ([crates/pwr-verify](../crates/pwr-verify/src/lib.rs)). |
| **Evidence** | Observations with provenance: check results, tool receipts, file hashes, audit events. A model's claim is not evidence ([glossary](glossary.md)). |

---

## 3. Why PWR exists

These points are reconstructed from [MASTER_SPEC.md](../MASTER_SPEC.md),
[README.md](../README.md), [local-agent-research.md](local-agent-research.md),
the [roadmap](roadmap.md), the [backlog](backlog.md) and comments in the code.

- **A valid API response is not a reliable agent.** "A valid
  OpenAI-compatible response does not establish reliable tool selection, edit
  semantics, working-state retention or autonomous completion"
  ([MASTER_SPEC](../MASTER_SPEC.md)). Transport, template, context and
  harness defects can produce the same symptom as a weak model.
- **Harness defects look like model weakness.** This is the project's
  strongest recorded evidence ([local-agent-research.md](local-agent-research.md)).
  Examples: preserving a just-read result changed one compaction task from
  0/1 to 1/1; a read-only completion-contract fix changed a diagnosis task
  from 0/6 to 6/6. All of these are development observations, not general
  effect sizes.
- **Local model variability.** Tool-call syntax, edit accuracy, reasoning
  behaviour and latency depend on the exact model, quantization, engine,
  template and loaded context. PWR therefore treats a *deployment*, not a
  model name, as the unit of identity ([glossary](glossary.md)).
- **Context constraints were the dominant failure.** Under the old regime a
  16,384-token window left roughly 5,000 tokens of history. 35 of 35
  development trials compacted at least twice and 2 resolved. The window came
  from a speed calibration later found wrong by a factor of six
  ([roadmap](roadmap.md) "What is actually blocking everything"). This is why
  PWR computes its window from memory and model shape instead of probing
  (§9).
- **Tool-use reliability of smaller models.** Many harness rules exist
  because a specific local model failed in a specific way. Examples: a
  missing `args` field, `cd sub && npm run build` sent seventy times, a
  refusal counted as a malformed call. Each is documented where it was fixed
  (e.g. comments on `ActionProposal` in
  [pwr-tools/src/lib.rs](../crates/pwr-tools/src/lib.rs)).
- **Deterministic verification.** Completion claims should rest on the
  repository's own checks, not on the model saying it is done
  ([ADR-008](adr/ADR-008-deterministic-verification.md), design principle 6).
- **Hardware differences.** The same rules must rate a 16 GB Mac and a 128 GB
  one ([models-and-context.md](models-and-context.md)). Tests pin both.
- **Local-first control.** The maintainer's rule: **no external cloud
  services** ([roadmap](roadmap.md), 2026-09-23). The model backend must be
  local: PWR starts it itself as a child process.
- **Efficient use of smaller open-weight models.** Small models are the
  declared target of the context work, and the adopted research thesis asks
  whether a harness can raise verified success for 7–14B models against a
  fixed tool-loop baseline ([backlog R.10](backlog.md),
  [external review](external-review-2026-09-23.md)). **This is a
  hypothesis; no result exists yet.**
- **Access.** One backlog note records the intent that PWR is for people who
  cannot pay for API access ([backlog D.16](backlog.md)). The glossary says
  the name states the thesis: how much engineering capability a harness can
  extract "from a model that a frontier lab would call inadequate, on
  hardware a developer already owns".

---

## 4. Vision — *VISION, not current behaviour*

From [MASTER_SPEC.md](../MASTER_SPEC.md) and [README.md](../README.md):

> PWR is meant to become a coding agent with no capability ceiling, driven
> from a conversation. It would reach anything on the machine it runs on and
> anything on the network the user authorizes: files, shells, processes,
> toolchains, Git, local applications and services, a browser, the open
> internet. The scope of each grant would be visible and revocable.

The product the research serves is a standalone desktop app for **macOS and
Windows** that alone drives every capability the agent has
([MASTER_SPEC](../MASTER_SPEC.md) "Minimum research core").

The research vision is a reproducible account of **which harness
intervention helps which local deployment and task regime**, measured as
"same model + same task + different harness"
([README](../README.md)).

**Invariant principles.** The canonical documents state these as holding
whatever the implementation becomes:

1. The model proposes semantics; the harness preserves facts. A model claim
   is never automatically a verified fact.
2. Authorization is explicit and observable. The authorization model exists
   to make reach visible, not to keep the agent small. Tool output never
   grants permission.
3. Completion is an evidence contract. Missing checks stay explicit, and a
   model cannot waive checks or change acceptance criteria.
4. Local data and traces stay local by default. External inference or
   network tools are explicit, consented decisions.
5. Measured and shipped paths must be the same path ("a measured path and a
   product path that differ are two products", [CONTRIBUTING](../CONTRIBUTING.md)).
6. Negative results are deliverables. An intervention that does not beat a
   fixed baseline is removed.
7. Ease of use is an acceptance requirement. A capability that needs a
   remembered flag or a raw log to drive is not finished.

**Sequencing, not scope limits.** Browser/computer control, network research
tools and MCP are "on the intended path" but come after the coding loop can
be measured ([MASTER_SPEC](../MASTER_SPEC.md) "Boundaries and non-goals").

---

## 5. Mission

**Draft mission (today):** Make open-weight models running on a developer's
own Mac do real, checkable work in a real repository: read, edit, run and
verify inside a sandbox, with every change, check and decision visible and
recorded locally. Then measure honestly how much of that the harness, rather
than the model, is responsible for.

Shorter: *Local models, real repositories, verifiable work, honest
measurement.*

---

## 6. Design principles

Each principle below is one the repository states and enforces in code.

| Principle | What it means concretely in PWR | Evidence |
|---|---|---|
| **Local first** | Inference runs on PWR's own engine as a child process on the same Mac (stdio for MLX, loopback for llama.cpp). The MLX and embedding sidecars force `HF_HUB_OFFLINE=1`. The maintainer's rule is "no external cloud services". | [pwr-mlx](../crates/pwr-mlx/src/lib.rs); [pwr_mlx.py](../crates/pwr-mlx/sidecar/pwr_mlx.py) l.73; [pwr_embed.py](../crates/pwr-mlx/sidecar/pwr_embed.py) l.31; [roadmap](roadmap.md) 2026-09-23 |
| **Model proposes, harness preserves facts** | File hashes, tool outcomes, check state and changed files are recorded by the harness. Compaction keeps changed files "from the checkpoint, the audit's account, not from the messages". | [MASTER_SPEC](../MASTER_SPEC.md) principle 1; [models-and-context.md](models-and-context.md) |
| **Deployment-aware, not name-aware** | Reasoning capability is read from the chat template, not a family name. Evidence is keyed on digest, quantization, template, tokenizer and backend version, and a changed quantization makes a calibration stale. | [model-compatibility.md](model-compatibility.md); [profile.rs](../crates/pwr-models/src/profile.rs) |
| **Unknown ≠ unsupported** | An untested model is *Provisional* and usable. Only concrete evidence makes it *Incompatible*. | [compatibility.rs](../crates/pwr-models/tests/compatibility.rs) `an_unknown_model_is_provisional_and_usable_not_unsupported` |
| **Hardware aware** | The window is computed from host memory, a reserve, the model's per-token cache cost and trained length. Every ceiling is kept and the binding one is named. The Model Manager rates variants per machine. | [window.rs](../crates/pwr-orchestrator/src/window.rs); [fit.rs](../crates/pwr-models/src/fit.rs) |
| **Deterministic verification** | Checks are the repository's own commands, discovered in a fixed order and run by the harness. Goal completion requires a pre-existing acceptance check whose file hash has not changed. | [pwr-verify](../crates/pwr-verify/src/lib.rs); [serve.rs](../crates/pwr-cli/src/serve.rs) `acceptance_contract_hash` |
| **Bounded recovery** | Every retry has a named, consecutive bound: 3 silent turns, 3 unparseable replies, 3 backend faults, 2 compactions per turn, 1 reasoning-finalization retry, a no-progress limit of 3 windows × 6 actions, and a 26-action check-in. | [converse.rs](../crates/pwr-orchestrator/src/converse.rs) constants; [stall.rs](../crates/pwr-orchestrator/src/stall.rs) |
| **Transparent execution** | Actions stream to the client as proposed → started → completed/failed/refused, with diffs. Stop reasons map to typed terminal classes. Reasoning streams live but is never stored. | [pwr-serve.md](pwr-serve.md); [converse.rs](../crates/pwr-orchestrator/src/converse.rs) `TurnStep`, `StopReason` |
| **Authorize effects, not models** | Approval categories name effects (dependency change, network, local service, toolchain install, history rewrite, publish, verifier proposal). Denial is the default and grants are never inferred. | `Approval` in [pwr-tools](../crates/pwr-tools/src/lib.rs) |
| **No shell interpretation** | An executable and its arguments stay separate. Shell builtins and operators are refused with the correct alternative (`cwd`, `stdin`). | [CONTRIBUTING](../CONTRIBUTING.md); backlog D.E2E-1 |
| **A refusal carries what it knows** | A stale-hash refusal names the current hash, and an ambiguous argument names what should have been sent. | [CONTRIBUTING](../CONTRIBUTING.md) |
| **Reproducibility / provenance** | Numbers come with counts and provenance. Campaign comparison refuses undeclared differences (`compare_strict`). Sampling values in profiles carry their origin. | [pwr-eval](../crates/pwr-eval/src/lib.rs); [strategies/models.json](../strategies/models.json) note |
| **User control** | Ask/Auto per workspace; allow once / for this session / reject; Stop; steering; workspace trust; Revert; compaction threshold; Reasoning Effort; manual window. | [SECURITY.md](../SECURITY.md); [apps/desktop/README.md](../apps/desktop/README.md) |
| **Open weights, no remote code** | `trust_remote_code=False`; repositories needing `auto_map` are marked incompatible; `.py` files are never downloaded. | [catalog.rs](../crates/pwr-models/src/catalog.rs); [model-compatibility.md](model-compatibility.md) |
| **Negative results are deliverables** | Failed ideas are kept with their evidence. Example: a 1.5B "System One" decider and Rizzo Flow (4B) were tried for context selection and set aside (C.22a). | [backlog](backlog.md) C.22, C.22a; [experiment-log.md](experiment-log.md) |

---

## 7. Complete feature inventory

Format: **Name** — STATUS. *User value.* How it works. *Evidence.*

### 7.1 Desktop application

1. **Tauri 2 + Angular desktop app** — IMPLEMENTED. *A single window for
   conversation, changes and evidence, without a terminal.* A Rust shell
   starts `pwr serve --stdio` and relays ACP messages over Tauri IPC. The
   Angular 22 client (zoneless, signals) renders the conversation. *Evidence:*
   [apps/desktop/src-tauri/src/lib.rs](../apps/desktop/src-tauri/src/lib.rs),
   [apps/desktop/README.md](../apps/desktop/README.md),
   [core/bridge.ts](../apps/desktop/src/app/core/bridge.ts).
2. **Streaming conversation** — IMPLEMENTED. *The user sees the answer and the
   model's reasoning as they are generated.* ACP `agent_thought_chunk` carries
   reasoning, and `agent_message_chunk` with `_meta.pwr.live` carries the
   reply in progress. *Evidence:* [pwr-serve.md](pwr-serve.md); `TurnStep::Streaming` in
   [converse.rs](../crates/pwr-orchestrator/src/converse.rs); backlog D.13.
3. **Action feed with tool-call phases** — IMPLEMENTED. *The user sees each
   action as an object that is proposed, running, then completed, failed or
   refused.* `ToolCallStep` becomes ACP `tool_call`/`tool_call_update`, with
   ACP kinds (`read`, `search`, `edit`, `delete`, `move`, `execute`,
   `fetch`). *Evidence:* [pwr-serve.md](pwr-serve.md) "Tool kinds";
   [conversation.ts](../apps/desktop/src/app/ui/conversation.ts).
4. **Message queue and steering** — IMPLEMENTED. *The user can redirect a long
   turn without stopping it.* Messages typed during a turn are queued and
   sent when it ends, or delivered at the next action boundary with "Send
   now" (`_pwr/steer`). Each steer increments the objective revision in the
   audit. *Evidence:* [apps/desktop/README.md](../apps/desktop/README.md);
   `Continuity::steering` in [converse.rs](../crates/pwr-orchestrator/src/converse.rs); backlog D.E2E-26.
5. **Changes panel** — IMPLEMENTED. *Every file the agent changed, with a diff
   and +/- counts.* Diffs come from the core's `tool_call_update` diff
   content, captured before and after the edit. *Evidence:*
   [inspector.ts](../apps/desktop/src/app/ui/inspector.ts), [diff.ts](../apps/desktop/src/app/ui/diff.ts).
6. **Revert file / Revert all** — IMPLEMENTED. *Undo an agent edit from the
   Changes panel.* The app calls `_pwr/revert`. The core resolves the path
   through the workspace policy and refuses when the file no longer holds the
   model's version, so a later manual edit is never overwritten. It then
   restores the previous content (or removes a created file) and records
   `conversation.reverted` in the log. Until 2026-09-24 the Tauri shell wrote
   the file itself, outside the audit. *Evidence:* `revert`, `revert_file` and
   test `revert_restores_the_file_only_while_it_holds_what_the_model_wrote` in
   [serve.rs](../crates/pwr-cli/src/serve.rs); `revertChange` in
   [agent.store.ts](../apps/desktop/src/app/core/agent.store.ts).
7. **Evidence panel** — IMPLEMENTED. *One-click access to the repository's
   checks and the session record.* Buttons run Verify (the workspace's own
   checks), Changes, Report, Diagnose and Doctor through `_pwr/*` extension
   methods and show the core's text summary. *Evidence:*
   [inspector.ts](../apps/desktop/src/app/ui/inspector.ts) `commands`;
   [serve.rs](../crates/pwr-cli/src/serve.rs).
8. **Core log panel** — IMPLEMENTED. *Shows the core process's stderr for
   troubleshooting.* *Evidence:* [inspector.ts](../apps/desktop/src/app/ui/inspector.ts).
9. **Chat mode (no workspace)** — IMPLEMENTED. *Talk to a local model without
   opening a project.* The first launch opens Chat. The catalogue is reduced
   to `read_file` and `list_tree` over attachments only, so writes and
   commands are impossible because the calls cannot decode, not merely because
   the prompt says so. It lives in `~/.pwr/chat`. Measured prompt: 733 tokens,
   against 2,888 for a greeting in a workspace. *Evidence:*
   `chat_only_tool_catalog` in [converse.rs](../crates/pwr-orchestrator/src/converse.rs); backlog C.26.
10. **Workspace trust** — IMPLEMENTED. *PWR does not start agent work in a
    folder the user has not explicitly trusted.* The first open of a folder
    asks for trust, and trust is remembered per exact path in the app's data
    folder. *Evidence:* `trust_workspace`, `core_start` in
    [lib.rs](../apps/desktop/src-tauri/src/lib.rs); [workspace-trust.ts](../apps/desktop/src/app/ui/workspace-trust.ts).
11. **Conversations per workspace (list, resume, delete)** — IMPLEMENTED.
    *Pick up where you left off.* Uses ACP `session/list`, `session/load` and
    `session/resume`, plus `_pwr/session_delete`. A resume replays the
    transcript and reports files edited since and uncertain writes.
    *Evidence:* [pwr-serve.md](pwr-serve.md) "Lifecycle";
    [conversation_list.rs](../crates/pwr-orchestrator/tests/conversation_list.rs).
12. **Attachments** — IMPLEMENTED. *Give the agent documents or reference
    code.* Files are extracted, bounded and snapshotted by content hash.
    Folders outside the workspace become read-only reference roots. An
    unreadable attachment refuses the prompt. Drag and drop is supported.
    *Evidence:* [pwr-serve.md](pwr-serve.md) "A prompt turn".
13. **Command palette and keyboard shortcuts** — IMPLEMENTED. ⌘K palette, ⌘N
    new conversation, ⌘B sidebar, ⌥⌘B inspector, ⌘, settings. *Evidence:*
    [command-palette.ts](../apps/desktop/src/app/ui/command-palette.ts), [ui.ts](../apps/desktop/src/app/core/ui.ts).
14. **Appearance: System / Light / Dark** — IMPLEMENTED. Follows the OS live
    and is applied before first paint. Design tokens are defined for both
    themes. *Evidence:* [theme.ts](../apps/desktop/src/app/core/theme.ts),
    [styles/tokens.css](../apps/desktop/src/styles/tokens.css), [theme.spec.ts](../apps/desktop/src/app/core/theme.spec.ts).
15. **Adaptive panel layout** — IMPLEMENTED. The conversation keeps at least
    560 px. The inspector collapses first, then the sidebar becomes a rail.
    Panels are resizable and their sizes are remembered. *Evidence:*
    [layout.ts](../apps/desktop/src/app/core/layout.ts), [layout.spec.ts](../apps/desktop/src/app/core/layout.spec.ts).
16. **Accessible UI primitives** — IMPLEMENTED. A dialog with focus trap and
    restore, an ARIA listbox select, tooltips, toasts and a confirmation
    dialog instead of `window.confirm`. The design commit states WCAG AA
    contrast on the main pairs; no automated contrast test was found.
    *Evidence:* [ui/kit](../apps/desktop/src/app/ui/kit/), [kit.spec.ts](../apps/desktop/src/app/ui/kit/kit.spec.ts); commit `954e7b49`.
17. **First-run engine installer** — IMPLEMENTED (manual QA pending).
    *A downloaded app can install its own inference engine.* On an
    Apple-silicon Mac with no engine, a setup screen uses a bundled `uv` to
    create a standalone Python 3.11 plus pinned `mlx==0.32.0`,
    `mlx-lm==0.31.3`, `mlx-embeddings==0.1.0` and `mlx-vlm==0.6.17`, and to
    fetch the search encoder, under `~/Library/Application
    Support/ai.pwr.desktop/engine` (about 1.2 GB). It shows progress, cancel
    and retry, then runs a final import check. *Evidence:*
    [engine.rs](../apps/desktop/src-tauri/src/engine.rs), [engine-setup.ts](../apps/desktop/src/app/ui/engine-setup.ts), commit `1c1c62c3`.
18. **Settings** — IMPLEMENTED. Appearance and where the engine was found.
    *Evidence:* [settings.ts](../apps/desktop/src/app/ui/settings.ts).
19. **Safe external links** — IMPLEMENTED. Web links from the Hub or model
    replies open in the default browser through an `http(s)`-only command.
    *Evidence:* `open_external` in [lib.rs](../apps/desktop/src-tauri/src/lib.rs); commit `ad9eefa4`.
20. **Demo mode** — IMPLEMENTED (developer aid). `?demo` fills the UI with a
    recorded conversation in a browser, and `?setup` simulates the engine
    install. *Evidence:* [demo.ts](../apps/desktop/src/app/core/demo.ts), [apps/desktop/README.md](../apps/desktop/README.md).

### 7.2 Agent execution

21. **Conversation loop** — IMPLEMENTED. *One loop in which a turn may
    answer, read, edit, run, or any sequence of those.* Each generation is
    followed by decode → gate (repetition, approval) → perform (audited
    executor) → observation, until the model answers, calls `complete` or
    `decline`, or a bound stops it. *Evidence:* `take_turn` in
    [converse.rs](../crates/pwr-orchestrator/src/converse.rs); [session.rs](../crates/pwr-orchestrator/src/session.rs); [two_loops.rs](../crates/pwr-cli/src/two_loops.rs).
22. **Goal mode** — IMPLEMENTED. *Keep working across check-ins until the goal
    is verified.* The turn continues automatically at checkpoints, up to a
    208-action guard. A model's `complete` triggers full verification. The
    goal counts as verified only if technical checks pass **and** a declared
    `"kind": "acceptance"` check exists whose `.pwr/checks.json` hash is
    unchanged since the session began. Checks that were already failing are
    named and left alone. *Evidence:* `GOAL_MAX_ACTIONS`,
    `acceptance_contract_hash`, test
    `goal_mode_continues_past_a_checkpoint_and_requires_full_verification` in
    [serve.rs](../crates/pwr-cli/src/serve.rs); `verify_goal` in [main.rs](../crates/pwr-cli/src/main.rs).
23. **Post-edit check verdict** — IMPLEMENTED. *After any edit the user learns
    what the repository's checks said.* A baseline is captured before the
    first edit and compared afterwards. The verdict is `Green`, `NewFailures`
    or `BaselinePreserved { still_failing }`, and pre-existing failures are
    never reported as green. *Evidence:* `check_verdict` in [main.rs](../crates/pwr-cli/src/main.rs); [converse.rs](../crates/pwr-orchestrator/src/converse.rs) module doc.
24. **Stop / cancel** — IMPLEMENTED. Stop reaches an in-flight generation:
    dropping the stream stops the engine. ACP `session/cancel` ends the turn
    with stop reason `cancelled`. *Evidence:* [converse.rs](../crates/pwr-orchestrator/src/converse.rs); [pwr-serve.md](pwr-serve.md).
25. **Soft action budget (check-in)** — IMPLEMENTED. After 26 actions a turn
    stops and asks, and "carry on" continues with nothing discarded.
    *Evidence:* `ACTIONS_BEFORE_CHECKING_IN` in [converse.rs](../crates/pwr-orchestrator/src/converse.rs).
26. **Loop and stall detection** — IMPLEMENTED. A repeat of a refused action
    is refused without asking and recorded as `loop.detected`. A window of
    six actions that leaves the workspace unchanged and reads nothing new
    counts as stalled, and three stalled windows stop the work. The state
    survives goal check-ins. *Evidence:* [repetition.rs](../crates/pwr-orchestrator/src/repetition.rs), [stall.rs](../crates/pwr-orchestrator/src/stall.rs), [loop_detection.rs](../crates/pwr-orchestrator/tests/loop_detection.rs).
27. **Bounded fault handling** — IMPLEMENTED. Three consecutive empty turns,
    unparseable tool calls or backend faults stop the turn with a named
    reason. Work done before the stop is kept. A backend fault no longer
    discards the turn's history. *Evidence:* constants and `StopReason::said`
    in [converse.rs](../crates/pwr-orchestrator/src/converse.rs); [malformed_and_flaky.rs](../crates/pwr-orchestrator/tests/malformed_and_flaky.rs), [runaway_reply.rs](../crates/pwr-orchestrator/tests/runaway_reply.rs).
28. **Record of failed generations** — IMPLEMENTED. A generation that fails
    leaves a `turn.failed` record with what streamed and how long it took.
    *Evidence:* [converse.rs](../crates/pwr-orchestrator/src/converse.rs); backlog D.E2E-22.
29. **Unattended scripted run (`pwr run`)** — IMPLEMENTED (research surface).
    It grants only what `--approve` names, honours `.pwr/protected.json`, and
    ends `verified` only when the workspace's checks pass. It needs a probed
    capability artifact. Live: one task in an empty crate was verified by its
    own tests in about 5 minutes (n=1). *Evidence:* [current-cli.md](current-cli.md); `run_action_loop_*` in [orchestrator lib.rs](../crates/pwr-orchestrator/src/lib.rs); backlog D.E2E-28.
30. **Plan-first runs (`pwr run --plan`)** — EXPERIMENTAL. A bounded plan of
    subgoals with dependencies. It is off by default, and a plan is context,
    not authority. *Evidence:* [plan.rs](../crates/pwr-orchestrator/src/plan.rs), [planning.rs](../crates/pwr-orchestrator/tests/planning.rs), [plan_execution.rs](../crates/pwr-orchestrator/tests/plan_execution.rs).
31. **Toolchain provisioning (`pwr run --provision`)** — EXPERIMENTAL. It
    grants network plus any executable so a run can install a JDK, Go or
    Flutter inside the workspace. It is documented as a known limit of the
    boundary, for supervised use only. *Evidence:* `Approval::ToolchainInstall` in [pwr-tools](../crates/pwr-tools/src/lib.rs); [SECURITY.md](../SECURITY.md).
32. **Named sessions (`pwr run --session`, `pwr session list|show`)** —
    IMPLEMENTED (CLI). What earlier runs established is re-checked against
    the current workspace. *Evidence:* [sessions_cli.rs](../crates/pwr-cli/tests/sessions_cli.rs), [sessions.rs](../crates/pwr-orchestrator/tests/sessions.rs).
33. **Terminal console (`pwr chat`)** — IMPLEMENTED (developer tool, not a
    product surface). *Evidence:* [current-cli.md](current-cli.md).
34. **ACP server (`pwr serve --stdio`)** — IMPLEMENTED. JSON-RPC 2.0 over
    stdio with no network listener. Every outgoing message is validated
    against the vendored ACP schema in tests, and a golden transcript pins an
    edit plus an approval granted and refused. *Evidence:*
    [serve.rs](../crates/pwr-cli/src/serve.rs), [tests/fixtures/acp](../crates/pwr-cli/tests/fixtures/acp/).

### 7.3 Model and runtime support

35. **PWR MLX engine** — IMPLEMENTED. A Python sidecar over `mlx-lm` that
    renders chat templates itself (transformers' sandboxed Jinja), streams
    reasoning and answer separately, counts tokens with the model's
    tokenizer, supports cancellation within a token and reports its version
    (`mlx-lm`, `mlx`, script hash). *Evidence:* [pwr-mlx/src/lib.rs](../crates/pwr-mlx/src/lib.rs), [pwr_mlx.py](../crates/pwr-mlx/sidecar/pwr_mlx.py), 22 sidecar tests in [test_pwr_mlx.py](../crates/pwr-mlx/sidecar/test_pwr_mlx.py).
36. **Prompt cache held across a turn** — IMPLEMENTED; confirmed on one run
    (backlog D.E2E-21).
37. **Images for models with a vision encoder** — EXPERIMENTAL (steps 1–2 of
    3 built). A model whose `config.json` declares a vision encoder is loaded
    once through `mlx-vlm` for text and images. Images are stored in
    `.pwr/images/<sha256>`, and other models refuse the image. Live on
    Qwen3.6-35B-A3B: text was identical token for token to the `mlx-lm` load,
    at the same speed, for +0.9 GB. The "rendered page as screenshot" step is
    not built. *Evidence:* backlog C.25; [pwr-serve.md](pwr-serve.md) "A prompt turn"; [roadmap](roadmap.md) 2026-09-23.
38. **llama.cpp / GGUF backend** — EXPERIMENTAL (limited). GGUF metadata,
    a managed `llama-server` and streaming generation with tool calls
    constrained to the catalogue. The server restarts per generation, so it
    reloads the model every turn. Reasoning budget cannot be enforced. Not
    available in the macOS app.
    *Evidence:* [pwr-llama](../crates/pwr-llama/src/lib.rs), [real_smoke.rs](../crates/pwr-llama/tests/real_smoke.rs) (ignored by default); backlog R.4, B.7.
39. **Model behaviour adapters** — IMPLEMENTED. Generic, Qwen, Granite,
    Seed, Harmony (gpt-oss) and GLM codecs translate canonical requests into
    each template's conventions. *Evidence:* [pwr-compat](../crates/pwr-compat/src/lib.rs).
40. **Deterministic tool-call repairs ("fallbacks for errors of form")** —
    IMPLEMENTED (first version). These are unambiguous, versioned repairs,
    for example `replacement` → `replace`, several `replace_text` on one file
    merged into one atomic `apply_patch`, and a missing `args` meaning none.
    A missing field is filled only from the one field that can mean it.
    Measured on the A1 replay: 6/6 repaired, no regression. *Evidence:*
    backlog C.24; [malformed_calls.rs](../crates/pwr-orchestrator/tests/malformed_calls.rs); [scripts/model_forms.py](../scripts/model_forms.py).
41. **Engines are local child processes** — IMPLEMENTED. The MLX engine is
    a sidecar the core spawns and talks to over stdin/stdout, with no network
    endpoint. `llama-server` is started by the core on `127.0.0.1` only. The
    `PWR_LLAMA_HOST` override was removed on 2026-09-24, and there is no
    remote-inference option. *Evidence:* [pwr-mlx](../crates/pwr-mlx/src/lib.rs), `LOOPBACK` in [pwr-llama](../crates/pwr-llama/src/lib.rs); [SECURITY.md](../SECURITY.md).

### 7.4 Model Manager

42. **Hugging Face search** — IMPLEMENTED (MLX in the macOS app; GGUF
    only when the core runs llama.cpp). JSON API only, 20 repositories
    per page with "Load more models". Filters (applied in the core): fits
    this machine, parameter range, context length, download size,
    quantization and family. *Evidence:* [hub.rs](../crates/pwr-models/src/hub.rs), [catalog.rs](../crates/pwr-models/src/catalog.rs), [models-and-context.md](models-and-context.md).
43. **Per-variant fit rating** — IMPLEMENTED. Ratings are Recommended,
    Should fit, Tight fit, Not recommended, Incompatible or Unknown. Each
    comes with an explanation and its assumptions, using the same arithmetic
    as the working window. *Evidence:* [fit.rs](../crates/pwr-models/src/fit.rs).
44. **Verified, resumable downloads** — IMPLEMENTED (not re-exercised end to
    end by a person). The core re-reads the file list at a pinned commit.
    Every file needs a size and a checksum (LFS SHA-256 or git blob SHA-1).
    Downloads are refused without a checksum, write to `.part` files, verify,
    then rename. They resume with HTTP range requests, retry up to five times
    after a drop, keep a 5 GiB free-disk margin, never overwrite an existing
    file and record the Hub revision. *Evidence:*
    [download.rs](../crates/pwr-models/src/download.rs), [models.rs](../crates/pwr-models/tests/models.rs); commit `ad9eefa4`.
45. **Models on this Mac: list and delete** — IMPLEMENTED. Delete asks for
    confirmation first. It is refused for the model in use, for anything
    outside the models folder (symlinks included), and for folders without
    `config.json`. *Evidence:* [local.rs](../crates/pwr-models/src/local.rs), [local.rs tests](../crates/pwr-models/tests/local.rs).
46. **Model chip and switcher** — IMPLEMENTED. Switch between discovered
    models, change the working window (−/+) and open the Model Manager.
    Models that can see images are marked. *Evidence:* [model-picker.ts](../apps/desktop/src/app/ui/model-picker.ts).
47. **CLI downloads** — IMPLEMENTED. `pwr models download <artifact>` uses the
    same downloader. *Evidence:* [models-and-context.md](models-and-context.md).

### 7.5 Model compatibility and calibration

48. **Five compatibility states** — IMPLEMENTED. Verified, Locally
    calibrated, Provisional, Limited and Incompatible, each with a
    confidence level. **The Verified registry is empty on purpose.**
    *Evidence:* [profile.rs](../crates/pwr-models/src/profile.rs), [verified-models.json](../crates/pwr-models/verified-models.json).
49. **Quick Calibration** — IMPLEMENTED. At most nine fixed requests at
    temperature 0 with a fixed seed, 120 s each, scored mechanically (exact
    match, JSON parse, JSON-Schema validation). Four checks are
    agent-critical: termination, tool selection, tool arguments and
    tool-result continuation. It is cancellable and records nothing if
    cancelled. Live: Qwen3-14B 4-bit in 21 s with 9/9, and gpt-oss-20b in
    12.6 s with 9/9. *Evidence:* [calibration.rs](../crates/pwr-models/src/calibration.rs), [live_model.rs](../crates/pwr-models/tests/live_model.rs), [readiness](release/v0.1.0-alpha-readiness.md).
50. **Provenance and reuse rules** — IMPLEMENTED. Evidence becomes stale,
    reduced or applicable by fixed rules. It is stored in
    `~/.pwr/model-evidence`, never in a repository. *Evidence:*
    [compatibility.rs](../crates/pwr-models/tests/compatibility.rs).
51. **Conservative defaults for untested models** — IMPLEMENTED. No invented
    sampling values, the template's reasoning capability with conservative
    budgets, and the normal computed window, deliberately **not** lowered.
    "Use Conservative Defaults" is remembered per workspace. *Evidence:* [model-compatibility.md](model-compatibility.md).
52. **Limited mode gate** — IMPLEMENTED. A model that fails an agent-critical
    check keeps chat but is refused agent work in a workspace, and the reason
    is shown. *Evidence:* `a_model_that_cannot_call_tools_is_limited_and_keeps_chat`; `chat_turn` in [main.rs](../crates/pwr-cli/src/main.rs).
53. **Packaged model profiles** — IMPLEMENTED (legacy matching).
    [strategies/models.json](../strategies/models.json) sets sampling and
    thinking per model reference, not per artifact digest. Several selectors
    are Ollama-era tags. Profiles no longer imply verification.
54. **Operating-point calibration and capability probes** — IMPLEMENTED
    (research). `pwr calibrate` (v6 ladder) and `pwr models inspect --probe`.
    They are no longer required for chat. *Evidence:* [glossary](glossary.md); [audit](adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md).

### 7.6 Reasoning control

55. **Reasoning Effort (Low / Medium / High)** — IMPLEMENTED. A typed enum
    from client to engine, saved per workspace. Budgets are 2,048 / 6,144 /
    12,288 calibrated, 1,024 / 4,096 / 8,192 conservative, or declared by an
    exact-artifact profile. *Evidence:* [reasoning.rs](../crates/pwr-domain/src/reasoning.rs), [reasoning_effort.rs](../crates/pwr-domain/tests/reasoning_effort.rs).
56. **Budget clamping to the context** — IMPLEMENTED. The budget is
    recomputed after every compaction and always keeps an answer reserve of
    up to 4,096 tokens. A property test checks effective ≤ requested,
    `max_tokens` ≤ room, and input + generation ≤ window. *Evidence:*
    `plan_reasoning`; [reasoning_effort.rs](../crates/pwr-domain/tests/reasoning_effort.rs).
57. **Forced close of the thinking phase** — IMPLEMENTED (MLX, `<think>` and
    `<seed:think>` templates only). At the budget the engine appends the
    template's own closing delimiter once and lets the answer follow. A
    failure ends `reasoning_unfinished` and gets one retry with thinking off.
    It cannot loop. gpt-oss gets its native level, not a budget. *Evidence:* sidecar tests; `reasoning_enforcement_cannot_loop` in [two_loops.rs](../crates/pwr-cli/src/two_loops.rs).
58. **Honest degradation** — IMPLEMENTED. Models whose reasoning cannot be
    bounded get no invented budget, and the selector is disabled with an
    explanation. *Evidence:* `models_without_a_controllable_phase_are_not_given_a_budget`; [compatibility.spec.ts](../apps/desktop/src/app/core/compatibility.spec.ts).

### 7.7 Context management

59. **Computed working window** — IMPLEMENTED. The window follows from host
    memory minus a reserve (¼ of memory, at least 8 GiB), the weights, the
    per-token KV cost from `config.json`, a prefill transient, and the
    model's trained maximum. Every ceiling is recorded and the binding one is
    named. The user can override it. *Evidence:* [window.rs](../crates/pwr-orchestrator/src/window.rs).
60. **Context indicator and panel** — IMPLEMENTED. `Context 42% · 54k /
    128k`, with used (engine count or estimate, labelled), composition
    (estimate, labelled), auto-compaction threshold, last compaction and
    last reply's reasoning/answer tokens. *Evidence:* [context-meter.ts](../apps/desktop/src/app/ui/context-meter.ts); `_pwr/context`.
61. **Automatic compaction** — IMPLEMENTED. Runs between actions at a
    threshold (default 75%, settable 50–90% per workspace), at most twice per
    turn. *Evidence:* [compaction.rs](../crates/pwr-orchestrator/src/compaction.rs), [context_compaction.rs](../crates/pwr-orchestrator/tests/context_compaction.rs).
62. **Manual "Compact now"** — IMPLEMENTED. Uses the same function between
    turns and writes a snapshot so a reopened conversation continues
    compacted. *Evidence:* `_pwr/compact`.
63. **Mechanical (non-model) compaction summary** — IMPLEMENTED. See §9.
64. **Measured-tier context recovery** — IMPLEMENTED. A prompt the backend
    refuses is retried at a lower measured window before compaction is
    spent, where calibration data exists. *Evidence:* [audit](adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md); [context_tiers.rs](../crates/pwr-orchestrator/tests/context_tiers.rs).
65. **Session ledger and retrieved passages per turn** — IMPLEMENTED. A
    conversation turn is composed through the same `context::compose_turn`
    sections and retrieval as a run. *Evidence:* [context.rs](../crates/pwr-orchestrator/src/context.rs); `compose_chat_turn` in [main.rs](../crates/pwr-cli/src/main.rs).

### 7.8 Repository understanding and retrieval

66. **Incremental content-addressed repository index** — IMPLEMENTED.
    Honours `.gitignore`, uses mtime+size to skip unchanged files, stores
    content hashes, is cached in SQLite under `.pwr`, and skips files over
    1 MB. *Evidence:* [pwr-repo](../crates/pwr-repo/src/lib.rs), [gitignore_semantics.rs](../crates/pwr-repo/tests/gitignore_semantics.rs).
67. **Shallow symbols, imports and test ownership** — IMPLEMENTED.
    Recognizes declaration keywords across languages (22 keywords, the
    audit says eight languages), plus imports as written (unresolved) and a
    naming-convention guess at which file a test covers. It is not an AST or
    call graph. *Evidence:* [symbols.rs](../crates/pwr-repo/tests/symbols.rs), [graph.rs](../crates/pwr-repo/tests/graph.rs).
68. **Lexical retrieval with graph neighbours** — IMPLEMENTED. Scoring
    weights symbol match, path match and content occurrence, then adds
    import-proximity and test-ownership neighbours. *Evidence:* `retrieve` in [pwr-repo](../crates/pwr-repo/src/lib.rs), [retrieval.rs](../crates/pwr-repo/tests/retrieval.rs).
69. **Documentation retrieval by Markdown section (BM25)** — IMPLEMENTED.
    Headings are weighted. Precision rose from 0.03 to 0.10 on twelve
    labelled documentation requests. *Evidence:* backlog C.22.
70. **Semantic context filter** — EXPERIMENTAL (opt-in
    `PWR_SEMANTIC_RETRIEVAL=1`). `multilingual-e5-small` on MLX in an offline
    sidecar, fused with BM25 (reciprocal-rank fusion). On twelve requests,
    sections-only P@3 went from 0.19 to 0.24 with 11% fewer tokens. One
    Qwen3-14B run per arm. Not a default. *Evidence:* [embed.rs](../crates/pwr-mlx/src/embed.rs), [pwr_embed.py](../crates/pwr-mlx/sidecar/pwr_embed.py), `retrieve_with`.
71. **`pwr repo index` / `pwr repo rank`** — IMPLEMENTED. Prints the passages
    a turn would receive, with scores and reasons. *Evidence:* [current-cli.md](current-cli.md).
72. **Search over installed dependencies** — IMPLEMENTED. `search` with
    `in_dependencies` covers `node_modules`, `site-packages` and cargo
    registry sources pinned by `Cargo.lock`. Dependencies are read-only
    without `DependencyChange`. *Evidence:* `search_dependencies` in [pwr-tools](../crates/pwr-tools/src/lib.rs); backlog C.12, D.E2E-29.

### 7.9 Tool system — see §12 for architecture

73. **Reading** — IMPLEMENTED: `read_file` with windows. Re-reads of an
    unchanged file are reported as such. *Evidence:* `read_file_window`, `read_file_for_model`.
74. **Search** — IMPLEMENTED: literal or regex, gitignore-style path globs,
    dependency mode, and a default result bound.
75. **`find_definition`** — IMPLEMENTED: finds where a name is declared.
76. **`list_tree`** — IMPLEMENTED.
77. **Hash-guarded edits** — IMPLEMENTED: `replace_text`, `apply_patch`
    (multi-hunk under one hash), `apply_replace` and `write_file`. A stale
    hash is refused with the current hash.
78. **File operations** — IMPLEMENTED: `make_directory`, `delete_path` (hash
    guard, or explicit recursive), `move_path` (destination inside the
    workspace) and `restore_file` (back to how the run first saw it).
79. **Commands** — IMPLEMENTED: `run_command` with argv only and no shell,
    optional `stdin` and `cwd`, environment cleared, bounded output and
    process-group termination.
80. **Services** — IMPLEMENTED: `start_service` waits until a port accepts
    and `stop_service` returns the output. *Evidence:* [service.rs](../crates/pwr-tools/src/service.rs), [services.rs](../crates/pwr-tools/tests/services.rs).
81. **Git** — IMPLEMENTED: `vcs_status`, `vcs_diff`. History-rewriting
    arguments and pushes need approval.
82. **`fetch_url`** — IMPLEMENTED; needs the `NetworkAccess` approval.
83. **`extract_document`** — IMPLEMENTED (narrow): writes a PDF's text beside
    it with provenance. Not OCR or layout analysis. *Evidence:* [document.rs](../crates/pwr-tools/src/document.rs), [documents.rs](../crates/pwr-tools/tests/documents.rs).
84. **`complete` / `decline`** — IMPLEMENTED: structured terminal actions.
    A refusal is a legitimate outcome, not a malformed call.
85. **`propose_verifier` / `record_progress`** — IMPLEMENTED in scripted runs
    only. A person decides whether a proposed check is adopted.

### 7.10 Verification and evidence

86. **Check discovery** — IMPLEMENTED. The order is `.pwr/checks.json` →
    CI configuration → `package.json` (build and test scripts) → a marker
    registry of 19 build systems → a static web-asset check. If nothing
    matches, "no verifier" is recorded and success is not assumed.
    *Evidence:* `discover_checks`, `BUILD_SYSTEMS` in [pwr-verify](../crates/pwr-verify/src/lib.rs); [languages.rs](../crates/pwr-verify/tests/languages.rs).
87. **Baseline and comparison** — IMPLEMENTED. See feature 23.
88. **Known-failure declarations** — IMPLEMENTED. An owner can quarantine a
    specific failing check in `.pwr/checks.json`. *Evidence:* [acceptance.rs](../crates/pwr-verify/tests/acceptance.rs).
89. **Acceptance-contract goal verification** — IMPLEMENTED. See feature 22.
90. **Diagnostics extraction and failure classes** — IMPLEMENTED. File and
    line locations are pulled from compiler output. Classes: Compilation,
    Assertion, Environment, Provider, Policy, NonDeterminism. *Evidence:* [diagnostics.rs](../crates/pwr-verify/tests/diagnostics.rs), [failure_class.rs](../crates/pwr-verify/tests/failure_class.rs).
91. **Static web-asset check** — IMPLEMENTED. Every local asset a page
    references must exist. No browser is run and the check proves nothing
    about appearance. *Evidence:* [web.rs](../crates/pwr-verify/src/web.rs), [web_assets.rs](../crates/pwr-verify/tests/web_assets.rs).

### 7.11 Audit and observability

92. **Hash-chained local event log** — IMPLEMENTED. `.pwr/state.sqlite`,
    with a global chain and a per-run chain. It records every tool attempt,
    denied or allowed. *Evidence:* [pwr-store](../crates/pwr-store/src/lib.rs), [chain.rs](../crates/pwr-store/tests/chain.rs), [audit.rs](../crates/pwr-orchestrator/tests/audit.rs).
93. **`pwr report`** — IMPLEMENTED. JSON/JSONL output, including whether
    the chain holds.
94. **`pwr diagnose`** — IMPLEMENTED. Deterministic detectors for pathologies
    already found once, validated on real trace fixtures. *Evidence:* [diagnose.rs](../crates/pwr-observe/src/diagnose.rs), [diagnosis.rs](../crates/pwr-observe/tests/diagnosis.rs), [observe fixtures](../crates/pwr-observe/tests/fixtures/).
95. **`pwr doctor`** — IMPLEMENTED. Checks the machine and the engine.
96. **Reasoning audit without the reasoning text** — IMPLEMENTED. Events
    carry counts, budgets and clamping. A test asserts that no event contains
    the reasoning text.
97. **Typed terminal classes** — IMPLEMENTED. Every ACP response carries
    `_meta.pwr.terminal` (declined, interrupted, budget, recovery, protocol,
    provider).

### 7.12 Security and policy — see §19

98. **macOS Seatbelt sandbox** — IMPLEMENTED (macOS only).
99. **Ask / Auto permission modes** — IMPLEMENTED.
100. **Approval categories with once / session grants** — IMPLEMENTED.
101. **Unconfined execution refused off macOS** — IMPLEMENTED
     (`PWR_ALLOW_UNCONFINED=1` to override, recorded `sandboxed: false`, and
     the app says so).
102. **Credential paths denied; `HOME`/`TMPDIR` redirected** — IMPLEMENTED.
103. **Protected paths** (`.pwr/protected.json`) — IMPLEMENTED for `pwr run`.
     An unreadable file stops the run.
104. **Output redaction** — IMPLEMENTED (narrow regex: `api_key` / `token` /
     `password` assignments and AWS access key IDs). *Evidence:* `ToolPolicy::redact`.

### 7.13 Hardware awareness — see §11

105. **Normalized host profile** — IMPLEMENTED.
106. **Engine readiness checks** — IMPLEMENTED.

### 7.14 Installation

107. **Build from source** — IMPLEMENTED (§20).
108. **Bundled `.app`** — IMPLEMENTED. Contains the release core, the MLX
     engine's scripts (`sidecar/pwr_mlx.py`, `sidecar/pwr_embed.py`, passed to
     the core as `PWR_MLX_SIDECAR` / `PWR_EMBED_SIDECAR`) and `uv`. Ad-hoc
     signed. *Evidence:* [tauri.conf.json](../apps/desktop/src-tauri/tauri.conf.json), `core_start` in [lib.rs](../apps/desktop/src-tauri/src/lib.rs).
109. **macOS DMG** — EXPERIMENTAL (the 0.1.0 release artifact).
     `PWR_0.1.0_aarch64.dmg`, ad-hoc signed and not notarized. First launch
     goes through System Settings → Privacy & Security → Open Anyway.
110. **CLI launcher install** (`scripts/install-pwr.sh` → `~/.local/bin`) —
     IMPLEMENTED.

### 7.15 Evaluation and developer tooling

111. **Evaluator with baseline arms** — IMPLEMENTED (research). `pwr eval run
     --arm b0|b1|b2`: B0 is a conventional tool loop, B1 is PWR, and B2 is a
     staged workflow. `compare_strict` rejects duplicate trials and
     undeclared differences. *Evidence:* [baseline.rs](../crates/pwr-orchestrator/src/baseline.rs), [strict_pairing.rs](../crates/pwr-eval/tests/strict_pairing.rs).
112. **Regression suites A1–A4 and corpora** — IMPLEMENTED (A1 tool calls,
     A2 navigation, A3 editing, A4 verification). A5/A6 are planned.
     *Evidence:* [suites/](../suites/), [corpus/](../corpus/).
113. **`pwr check-corpus`** — IMPLEMENTED. Checks that an external corpus is
     fair before anything is measured on it.
114. **CI** — IMPLEMENTED (macOS runner): fmt, clippy with `-D warnings`,
     workspace tests and the milestone-table check. The desktop app is not in
     CI. *Evidence:* [.github/workflows/ci.yml](../.github/workflows/ci.yml).
115. **Hermetic test suite** — IMPLEMENTED. No test needs a model or a
     network, except a few `#[ignore]`d live and network tests.

### 7.16 PLANNED (not available)

| Item | Backlog ref |
|---|---|
| One runtime contract for chat and scripted runs (shared planning, compaction, completion, recovery) | R.11, C.1 |
| llama.cpp server kept alive across turns | R.4, B.7 |
| Windows core and Windows execution isolation (job object / AppContainer) | E.1, E.2 |
| Signed, notarized installable builds; clean-install QA | D.17, readiness |
| Desktop app in CI | R.8 |
| Licence inventory of Rust/npm/Python dependencies | R.6 |
| First Verified model entries | model-compatibility.md |
| Small-model (7–14B) paired study against B0 | R.10 |
| Rendered-page screenshots for vision models | C.25 step 3 |
| Session denylist (`reject_always`) | D.14 |
| 16 GB machine class | B.4 |
| Suites A5/A6 and product gate | A.7, A.8, C.8 |
| MCP, browser, apps, web research tools (one at a time, after measurement) | C.9, C.12, R6 |

**Feature count (§7):** 115 numbered features.
- **IMPLEMENTED: 109.** Four carry stated limits: 17 (manual QA pending),
  44 (not re-exercised end to end), 53 (legacy matching) and 98 (macOS
  only). Five are
  research or developer surfaces rather than product features: 29, 33, 54,
  111 and 113.
- **EXPERIMENTAL: 6** — 30, 31, 37, 38, 70 and 109.
- **PLANNED: 13** (table above).

---

## 8. Core agent loop

This is the actual flow of one conversation turn in the app, reconstructed
from `pwr-cli/src/serve.rs`, `pwr-cli/src/main.rs` (`chat_turn`) and
`pwr-orchestrator/src/converse.rs`.

```text
user prompt (ACP session/prompt; text + attachments/images)
  → session readiness: model chosen and not Limited/Incompatible for agent work
  → attachments extracted, bounded, snapshotted by hash; images stored
  → turn composition: system prompt + history + session ledger
      + retrieved passages ranked against the request (context::compose_turn)
  → [loop, per generation]
      steering delivered? → record objective revision
      prompt over threshold? → mechanical compaction (≤2 per turn)
      reasoning envelope computed (plan_reasoning) inside remaining room
      generation streamed (thinking / answer) — stop reaches it mid-flight
      decode tool calls against the typed catalogue (+ deterministic repairs)
        unusable? → bounded retry with the fault explained
      for each proposed action:
        gate: repeat-of-refused? → refuse; approval needed? → ask user
        perform: policy + sandbox → execute → receipt, diff, audit event
        observation appended as a tool result
      stall / budget / fault bounds checked
  → turn ends: answer | complete | decline | stop reason
  → if files changed: targeted checks after vs baseline → verdict note
  → goal mode: on `complete`, full verification + acceptance contract;
      otherwise continue at checkpoint until verified or 208-action guard
  → conversation.turn_ended recorded with terminal class
```

**Stage by stage:**

- **Preparation.** `session/new` loads the workspace config
  (`.pwr/chat-config.json`: model, window, permission mode, effort,
  compaction threshold). It refuses readably if no model is prepared. A
  Limited model is refused agent work here.
- **Context construction.** The system prompt (workspace or chat-only),
  the message history, a per-turn session ledger built from files as they
  are on disk, and repository passages retrieved against the request (§13).
- **Inference.** Streamed through the backend. Reasoning goes to a separate
  channel and is never written back into the history.
- **Tool decision.** Calls are decoded against the offered schema.
  Unambiguous form errors are repaired deterministically and recorded.
  Anything else is refused with the exact expected fields.
- **Action.** `session::gate` runs, then `session::perform`. These are the
  same two steps the scripted loop uses (R1).
- **State update.** The checkpoint holds the turn, action count, changed
  files and their hashes, and the objective revision. The audit event is
  appended before the next step.
- **Verification.** Only if the workspace changed: a targeted baseline
  comparison. In goal mode, a full comparison plus the acceptance contract.
- **Recovery.** Bounded, class-specific: compaction, a lower measured
  window, re-asking after an unparseable call, retrying a reasoning overrun
  with thinking off, refusing repeats, and stopping on stalls. There is no
  unbounded retry.
- **Completion.** A prose answer can end an ordinary turn, but only
  `complete` can satisfy a goal's contract.

**The scripted loop is different.** `pwr run` and `pwr eval` use
`run_action_loop_*` in the orchestrator. That loop has its own planning,
ledger compaction, completion contract and recovery, and shares only gate,
perform and the tools. This divergence is documented and is planned to be
removed (R.11). **Do not describe measurements from the scripted loop as
measurements of the app.**

---

## 9. Context engineering

**Current implementation (conversation path):**

- **Working window.** Computed per model and host (`window::decide`) as the
  minimum of several ceilings: memory (host total − reserve − weights −
  engine overhead, divided by per-token cache cost including a prefill
  transient), trained length, and any user setting. It is rounded down to
  1,024. An unknown memory ceiling falls back to 32,768. The binding ceiling
  is named. The user can raise or lower it from the model chip.
- **Accounting.** `used` is the engine's exact prompt + generated count after
  the last reply (one reply stale), or a labelled estimate before any reply.
  Composition by category is a labelled estimate (characters ÷ 4). The
  reasoning planner counts the prompt as the engine count plus a denser
  estimate (three characters per token) for what was appended since, so it
  errs toward less reasoning.
- **Repository material.** Passages retrieved against the newest request
  are placed before it (§13). Retrieved passages are rebuilt on demand and
  dropped by compaction.
- **Conversation state.** Messages, a per-turn session ledger and a
  checkpoint (changed files with hashes, objective revision).
- **Compaction (auto and manual).** One function:
  `pwr_orchestrator::compaction::compact`. It is **mechanical, not
  model-written**.
  - *Kept:* the system prompt; the first request verbatim (≤800 characters)
    and every later request as a line; the newest messages verbatim in a
    tail sized in tokens (half the room, at least two messages; for a manual
    compaction, a quarter of the conversation); actions with their paths and
    commands; what the model said; every changed file with its current hash
    (from the audit); failures not followed by a success, with the last
    stderr line; the last check verdict.
  - *Dropped:* consumed tool output, superseded ledgers and retrieved
    passages, with counts of each.
  - A later compaction merges the earlier record rather than nesting it. The
    workspace, index and retrieval index are never touched.
  - Recorded as `context.compacted` with before/after estimates and trigger.
- **Overflow avoidance.** The threshold check runs before each request. If
  compaction cannot make room ("the recent exchanges alone already fill the
  window"), the turn stops with `ContextFull` and a readable explanation. Two
  compactions in one turn without finishing stops it as `Looping`. A refused
  prompt is retried at a lower measured window where calibration data exists.
- **Reasoning and context.** Reasoning is not stored in history, and its
  budget is recomputed from the room left after compaction.
- **Hardware/model dependency.** The window depends on host memory and the
  model's `config.json` (layers, KV heads, head dim, sliding layers, trained
  length). The rating in the Model Manager uses the same arithmetic.

**Not current / future:**
- The scripted run still has its own ledger compaction (C.1).
- Artifact rehydration (retaining raw command output bytes behind a
  reference) is PLANNED. Current command capture can hash bytes it did not
  keep ([architecture](architecture.md)).
- "Useful context" (the occupancy at which task performance holds) is not
  measured. Quick Calibration does not test long context, and the window is
  labelled *Context: provisional* for untested models.
- `PREFILL_TRANSIENT_FACTOR = 4` is a single observation on one model, and
  the code says it probably overstates for dense models.

---

## 10. Model awareness

| Aspect | Current behaviour | Evidence |
|---|---|---|
| Discovery | MLX folders with `config.json` under the models root; GGUF files under `PWR_LLAMA_MODELS`. The engine inspects config, template, tokenizer and weight names/sizes as data. | [pwr-mlx](../crates/pwr-mlx/src/lib.rs), [local.rs](../crates/pwr-models/src/local.rs) |
| Profiles | Packaged profiles by model reference (sampling, thinking on/off); declared reasoning budgets only for an exact artifact or deployment. | [strategies/models.json](../strategies/models.json), [profile.rs](../crates/pwr-models/src/profile.rs) |
| Capabilities | From Quick Calibration checks and the template. Parameter count is not recorded (neither engine reports it). | [model-compatibility.md](model-compatibility.md) |
| Vision | Detected from a vision encoder declared in `config.json`; such models load through `mlx-vlm`. The UI marks "sees images". | backlog C.25 |
| Context size | Trained maximum from `max_position_embeddings`, capped by memory arithmetic. | [window.rs](../crates/pwr-orchestrator/src/window.rs) |
| Calibration | Quick Calibration (§7.5); legacy operating-point calibration for research. | [calibration.rs](../crates/pwr-models/src/calibration.rs) |
| Untested models | Provisional, usable immediately, conservative defaults, "New model detected" prompt. | [compatibility.rs](../crates/pwr-models/tests/compatibility.rs) |
| Reasoning effort | Read from the template: `native_budget`, `explicit_thinking_stream`, `template_controlled`, `observable_only`, `none`, `unknown`. | [reasoning.rs](../crates/pwr-domain/src/reasoning.rs) |
| Runtime compatibility | MLX needs Apple silicon and a Python that imports `mlx_lm`. GGUF needs `llama-server`. `auto_map` repositories are incompatible. Models without a chat template are incompatible on MLX. | [catalog.rs](../crates/pwr-models/src/catalog.rs) |
| Selection | Per workspace, from the model chip or `_pwr/models`. The engine cannot be switched in a running app (`PWR_BACKEND=llama` at launch). | [models-and-context.md](models-and-context.md) |
| Metadata | Hub card data labelled with its source; unknown values read "unknown". | [catalog.rs](../crates/pwr-models/src/catalog.rs) |
| Provenance | Digest, weights fingerprint, quantization, revision, tokenizer and template fingerprints, backend version, suite version, hardware class. | [profile.rs](../crates/pwr-models/src/profile.rs) |

**Why it matters.** The project recorded that model profiles were never
loaded outside the checkout, so every workspace ran greedy with reasoning on
(D.E2E-24). It also recorded that a gpt-oss profile pinned reasoning to `low`.
Both silently changed behaviour. The provenance rules exist so that evidence
gathered on one artifact is never applied to another.

---

## 11. Hardware awareness

Only verified mechanisms:

- **Host profile** (`pwr_runtime::host::detect_host`, via `sysinfo`):
  platform, OS name and version, architecture (arm64 / x86_64), CPU, Apple
  chip name, generation and tier, total and available memory with a unified
  flag, GPUs, and free disk on the models volume. Unknown facts are `null`
  and listed. *Evidence:* [host.rs](../crates/pwr-runtime/src/host.rs).
- **Unified memory.** Apple-silicon GPU memory is the machine's memory, and
  no separate VRAM is claimed. NVIDIA VRAM comes from `nvidia-smi`. Other
  Windows GPUs are listed by name with memory unknown (a 32-bit field wraps).
- **Model fit** (§7.4 feature 43), using the same arithmetic as the window.
- **Working window** (§9) derived from memory.
- **Free disk.** Downloads require remaining bytes + 5 GiB.
- **Engine readiness.** Checked by running the engine, with the fix named if
  it is missing.
- **Quantization.** Read from the MLX config or the GGUF file name and
  displayed. The fit estimate uses file size, **not** quantization quality.
- **Runtime selection.** MLX is rated Incompatible off Apple silicon. A GGUF
  model larger than a discrete GPU's memory is never rated better than
  "Should fit".
- **Hardware class for provenance.** Coarse, e.g. `macos-arm64-apple-m2-64gb`.
- **Legacy hardware probe** (`hardware::probe_hardware`) is kept because
  calibrations are keyed on its hash.

**Not claimed.** No speed or performance prediction. The fit estimate
explicitly does not model speed, quantization quality or MoE sparsity.
Windows detection code exists but has not been run on Windows. The 16 GB
class has not been evaluated (B.4).

---

## 12. Tool system

- **Who provides tools.** The harness. The catalogue is defined once
  (`action_tool_catalog` in the orchestrator, `ActionProposal` in
  `pwr-tools`), and the same schema is offered and decoded against.
- **How the model sees them.** As a tool catalogue rendered by the model's
  adapter into its template's convention (native tool schema). The
  conversation catalogue omits run-only tools (`record_progress`,
  `propose_verifier`). Chat mode offers only `read_file` and `list_tree`.
- **Schemas.** JSON Schema per tool. Harness-owned bounds (search result
  count, tree size) are defaulted rather than required, because requiring
  them cost turns (107 refused `search` calls in one pilot).
- **Permissions and policy.** `ToolPolicy` resolves every path against the
  workspace root and refuses escapes and protected paths.
  `required_approval` maps actions to approval categories: dependency
  manifests, history-rewriting Git arguments, publish/push, network, local
  services, toolchain install. The gate asks the user through ACP
  `session/request_permission` when the grant is missing.
- **Execution.** Direct `argv`, never a shell. Environment cleared.
  `HOME`/`TMPDIR` inside the workspace. Seatbelt profile on macOS. Bounded
  stdout/stderr (head and tail). Process-group kill on timeout. Services are
  supervised for readiness and lifecycle.
- **Outputs.** A typed `ToolResult`: exit code, stdout and stderr (bounded,
  redacted), duration, artifact hash, truncation flags, `sandboxed`,
  `failing_files`. For edits, the new hash and a diff to the client.
- **Error handling.** Refusals are data the model reads and carry what the
  harness knows (current hash, expected fields). Policy denials and tool
  faults are distinguished (`{"denied": …}` vs `{"tool_failure": …,
  "failure_category": …}`).
- **State changes.** Every write is hash-guarded. The checkpoint records
  changed files, and the audit records the attempt before and the receipt
  after.
- **Safeguards.** No shell. `cd`, `&&`, `echo "cmd"` and never-exiting
  servers run via `run_command` are refused with the right alternative.
  Repeated refused actions are refused without asking. Credentials are never
  readable.

**Categories:** read/navigate (`read_file`, `search`, `find_definition`,
`list_tree`, `extract_document`); edit (`replace_text`, `apply_patch`,
`apply_replace`, `write_file`); filesystem (`make_directory`, `delete_path`,
`move_path`, `restore_file`); execute (`run_command`, `start_service`,
`stop_service`); VCS (`vcs_status`, `vcs_diff`); network (`fetch_url`);
control (`complete`, `decline`; run-only `record_progress`,
`propose_verifier`).

*Evidence:* [pwr-tools/src/lib.rs](../crates/pwr-tools/src/lib.rs);
[filesystem_and_vcs.rs](../crates/pwr-tools/tests/filesystem_and_vcs.rs),
[adversarial.rs](../crates/pwr-tools/tests/adversarial.rs),
[harness_torture.rs](../crates/pwr-tools/tests/harness_torture.rs),
[sandbox_and_approvals.rs](../crates/pwr-tools/tests/sandbox_and_approvals.rs);
[action_channel.rs](../crates/pwr-orchestrator/tests/action_channel.rs).

---

## 13. Repository understanding

What actually happens, as opposed to generic "RAG":

1. **Inventory.** `pwr_repo::index_incremental` walks the workspace with the
   `ignore` crate (gitignore semantics) and excludes policy paths. Files
   whose mtime and size are unchanged are reused from the SQLite cache under
   `.pwr`. Files over 1 MB are not indexed.
2. **Per-file record.** Path, content hash, bytes, declared symbols (a
   keyword scan over the first 2,000 lines), imports as written (up to 200),
   a guessed test subject from naming conventions, and up to 4,000 distinct
   lowercase terms.
3. **Retrieval for a turn.** The newest request is tokenized with stop words
   removed. Code files are scored by exact symbol match (40), partial symbol
   match (12), path match (8) and content occurrences (2 each, capped). The
   top seeds pull in neighbours by import proximity and test ownership.
   Markdown documents are ranked by section with BM25 (k1 1.5, b 0.9,
   headings ×3), at most two sections per document and 90 lines per section.
   Optionally, sections are fused with embedding similarity (reciprocal-rank
   fusion, k = 60).
4. **Context construction.** Excerpts (±12 lines of context around a match)
   are placed before the request within the remaining budget.
5. **Model-driven exploration.** Beyond retrieval, the model uses `search`
   (including dependencies), `find_definition`, `list_tree` and `read_file`.

**What it is not:** not an AST, not an LSP, not a resolved call graph, and
not a mandatory vector database. The glossary says so explicitly, and
heuristic edges are labelled as guesses.

*Evidence:* [pwr-repo/src/lib.rs](../crates/pwr-repo/src/lib.rs); tests
[retrieval.rs](../crates/pwr-repo/tests/retrieval.rs),
[symbols.rs](../crates/pwr-repo/tests/symbols.rs),
[graph.rs](../crates/pwr-repo/tests/graph.rs).

---

## 14. Evidence and verification

### The four things users see, and how they differ

| Concept | What it is | Where |
|---|---|---|
| **Changes** | The files the agent changed in this conversation, with before/after diffs, captured by the core at each edit. It says *what changed*, not whether it is right. | Changes panel; `_pwr/changes` |
| **Evidence** | Independent observations: the repository's check results (`Verify`), the session report, the diagnosis of known failure patterns, and the machine/engine check. It says *what was established*. | Evidence panel; `_pwr/verify`, `report`, `diagnose`, `doctor` |
| **Verification** | Running discovered checks and comparing them to a baseline. This is deterministic: exit codes of the repository's own commands. | [pwr-verify](../crates/pwr-verify/src/lib.rs) |
| **Core log / audit** | Two different things. The **Core log** panel is the core process's stderr, for troubleshooting. The **audit** is the hash-chained event log in `.pwr/state.sqlite`, which `pwr report` and `pwr diagnose` read. | Core log panel; [pwr-store](../crates/pwr-store/src/lib.rs) |

### What PWR accepts as evidence that work is complete

- **Ordinary conversation turn.** The turn can end with prose. If files
  changed, the user is told the check verdict: *Green* (every check passes),
  *NewFailures* (named), or *BaselinePreserved* (no new failures, and the
  still-failing checks named). Pre-existing failures are never called green.
- **Goal mode.** Only `complete` from the model triggers verification, and
  the goal is **verified** only when full verification passes **and** at
  least one declared `"kind": "acceptance"` check exists whose
  `.pwr/checks.json` hash equals its hash when the session started. The model
  cannot create or relax the contract that judges it. Without such a check,
  the result reads "Technical checks passed, but the goal is not verified".
- **Scripted run.** Ends `verified` only when the workspace's own checks
  pass. With no verifier, completion is refused. A person can adopt a
  proposed verifier via the `VerifierProposal` approval.

### Deterministic, model-driven, or mixed?

**Mixed, with a deterministic judge.** The model decides what to do and when
it believes it is done. The harness decides whether that is verified, by
running the repository's commands and comparing exit codes. No model judges
another model anywhere in verification or calibration.

### Honest limits

- Passing checks prove what the checks test, not every natural-language
  requirement (MASTER_SPEC principle 6).
- Check discovery from CI files is heuristic. The web-asset check does not
  render.
- Prose and documentation tasks have no mechanical acceptance. The system
  reports "not verified" rather than pretending ([direction.md](direction.md)).
- The planned common result semantics (`completed` / `blocked` / … ×
  `checks_passed` / `not_checked` …) are **PLANNED**, not implemented
  ([MASTER_SPEC](../MASTER_SPEC.md)).

### Failure, retry and recovery

- Failure classes: Compilation, Assertion, Environment, Provider, Policy,
  NonDeterminism, with located diagnostics.
- The scripted loop has a recovery budget (default 3 edit–verify cycles and
  1 context retry) and `recovery_decision`.
- The conversation has bounded counters and stop reasons (§8). In goal mode,
  a failed verification is fed back as "The goal is not complete: full
  repository verification failed… Evidence: …" and the goal continues until
  it is verified, stopped, stalled or at the 208-action guard.

*Evidence:* [pwr-verify tests](../crates/pwr-verify/tests/),
[completion_checks.rs](../crates/pwr-orchestrator/tests/completion_checks.rs),
[unrunnable_checks.rs](../crates/pwr-orchestrator/tests/unrunnable_checks.rs),
[evidence_state.rs](../crates/pwr-orchestrator/tests/evidence_state.rs),
[serve.rs](../crates/pwr-cli/src/serve.rs).

---

## 15. Transparency and observability

What the current app exposes while PWR works:

- **Reasoning.** Streamed live for models that reason. It is **not stored**
  with the conversation and never logged. It is the model's own output, not
  a summary.
- **Actions.** Each tool call with a human-readable detail (path, query,
  command and `cwd`) and phase: proposed, running, completed, failed or
  refused.
- **Permission requests.** What exactly is being asked, with allow once,
  allow for this session, or reject.
- **Changes.** Diffs per file, with +/- counts, Revert and Revert all.
- **Evidence.** Verify, Changes, Report, Diagnose and Doctor output.
- **Check verdict** after edits, as a note in the conversation.
- **Compactions.** Each is shown in the conversation. The context panel
  shows the last one.
- **Context and model state.** Used/window, estimated composition, last
  reply's reasoning and answer tokens (with `~` for estimates), model
  compatibility status, effort, and whether commands are sandboxed.
- **Why a turn ended.** Stop reasons in plain language, plus the typed
  terminal class.
- **Core log.** Raw stderr of the core.
- **Outside the app.** `pwr report` (including chain integrity) and `pwr
  diagnose` on the same event log.

Not exposed: a dedicated verification layout (D.11 open), replay of past
actions' details on resume (the transcript is replayed, not the action
objects), and an explicit terminal-state/evidence-outcome pair (planned).

---

## 16. Model Manager

| Topic | Current behaviour |
|---|---|
| Discovery | "Discover" tab: Hugging Face Hub JSON API (`/api/models`, `/api/models/{repo}`, `/tree/{revision}`, `resolve/{revision}/config.json`). No HTML scraping. `PWR_HF_BASE_URL` overrides the host. `HF_TOKEN` is sent for gated repos. |
| Search / filter | Query per format (MLX or GGUF). Filters: fits this machine, parameters, context length, download size, quantization, family. 20 repos per page, cursor pagination. |
| Metadata | Name, author, base model, parameters, architecture, context length, licence, format, engine, downloads and likes as the Hub reports them, labelled; missing = "unknown". |
| Hardware fit | Per-variant rating with explanation and assumptions (§11). |
| Formats | MLX (config, weights, tokenizer, template files; never `.py`) and GGUF (one quantization, all shards; no projectors or imatrix). |
| Quantization | GGUF from the file name (says so); MLX from config. |
| Download | Verified, resumable, conflict-safe (§7.4 feature 44). Progress at most 5×/s over `_pwr/download_progress` with states preparing / downloading / verifying / completed / failed(kind) / cancelled. |
| Delete | Confirmed, permanent, guarded (§7.4 feature 45). |
| Use / select | A finished MLX download is immediately selectable. A GGUF download for a non-running engine says to restart with `PWR_BACKEND=llama`. |
| Local location | `PWR_MLX_MODELS` / `PWR_LLAMA_MODELS`, default `~/.pwr/models/<owner>/<name>/` (was `~/.lmstudio/models` before 2026-09-24). |
| Integrity | LFS SHA-256 or git blob SHA-1 per file, mandatory. The `.pwr-revision` marker is written only for a full commit id. |
| Runtimes | MLX (PWR engine). llama.cpp in the core and command line only; hidden in the macOS app. |

Status: IMPLEMENTED. The readiness assessment notes it was "not re-exercised
end to end" after its last changes, and the manual test list includes search,
download, cancel/resume, verify and delete on a clean Mac.

---

## 17. Desktop experience

| Aspect | Fact | Evidence |
|---|---|---|
| Frontend framework | Angular 22 (zoneless, signals), TypeScript 6, `marked` + DOMPurify for Markdown, Vitest | [package.json](../apps/desktop/package.json) |
| Desktop framework | Tauri 2 (system WebView); Electron explicitly excluded for memory next to a local model | [pwr-serve.md](pwr-serve.md) |
| Core language | Rust 2024 (core), Rust 2021 (Tauri shell, own workspace); Python sidecar for MLX | [Cargo.toml](../Cargo.toml) |
| IPC | Angular ↔ Tauri commands/events ↔ Rust shell ↔ `pwr serve --stdio` (ACP, JSON-RPC 2.0, one process per workspace) | [lib.rs](../apps/desktop/src-tauri/src/lib.rs), [bridge.ts](../apps/desktop/src/app/core/bridge.ts) |
| Supported OS today | macOS on Apple silicon | [readiness](release/v0.1.0-alpha-readiness.md) |
| Intended OS | macOS and Windows; Linux "not a target" | [pwr-serve.md](pwr-serve.md) |
| Themes | System / Light / Dark, token-based design system in layered CSS | [styles/](../apps/desktop/src/styles/) |
| Webview security | CSP: scripts from the app only (Tauri hashes the inline theme script), inline styles allowed (Angular), images `self`/`data:`/`blob:`, network only to Tauri IPC; external links only `http(s)`, opened in the browser | [tauri.conf.json](../apps/desktop/src-tauri/tauri.conf.json) |
| Panels | Left: sidebar (workspace, conversations; collapses to a rail). Centre: conversation + composer (≥560 px). Top bar: conversation title, context indicator, model chip, inspector toggle. Right: inspector (Changes / Evidence / Core log). Floating: model popover, Model Manager, settings, command palette. | [app.html](../apps/desktop/src/app/app.html), [layout.ts](../apps/desktop/src/app/core/layout.ts) |
| Workspace model | First launch is Chat (no workspace). "Open a workspace" asks for trust of that exact folder, then starts the core there. The last workspace is reopened. Per-workspace settings live in `.pwr/chat-config.json`. | [lib.rs](../apps/desktop/src-tauri/src/lib.rs), [apps/desktop/README.md](../apps/desktop/README.md) |
| Window | Overlay title bar integrated with macOS traffic lights, 1360×880 default, 640×480 minimum | [tauri.conf.json](../apps/desktop/src-tauri/tauri.conf.json) |
| Language | English UI (the app language decision D.16 is formally open) | commit `954e7b49`; backlog D.16 |
| Tests | 7 spec files, 34 unit tests (format, store, compaction/extension notifications, Model Manager states, theme, layout, kit a11y) | `apps/desktop/src/**/*.spec.ts` |

---

## 18. Privacy and local-first model

Stated precisely, per operation:

| Operation | Network? | Details |
|---|---|---|
| **Model inference** | **No** (local) | The MLX sidecar is a child process over stdio, with `HF_HUB_OFFLINE=1` set by the sidecar. llama.cpp is a managed child on `127.0.0.1` by default (`PWR_LLAMA_HOST` overrides it). There is no remote-inference option. |
| **Conversation, tools, verification, audit** | No | All local. The event log is `<workspace>/.pwr/state.sqlite`. Chat mode data is in `~/.pwr/chat`. |
| **Commands the agent runs** | Denied by default | The sandbox denies network unless `NetworkAccess` (or `ToolchainInstall`/`--provision`) is granted. In **Auto** mode, network access is granted without asking. |
| **`fetch_url` tool** | Yes, if granted | Needs `NetworkAccess`. |
| **Model downloads** | Yes | Hugging Face (`huggingface.co` or `PWR_HF_BASE_URL`), started by the user. |
| **Hugging Face metadata** | Yes | Model Manager search, trees and configs, while the Model Manager is used. Anonymous use is subject to Hub rate limits. `HF_TOKEN` is sent if set. |
| **First-run engine install** | Yes | The bundled `uv` downloads a standalone Python 3.11 and the pinned `mlx*` packages. The search encoder `intfloat/multilingual-e5-small` comes from Hugging Face. The exact package index and Python distribution hosts are those `uv` uses by default; this is not documented in the repo (**requires author confirmation**). |
| **Semantic retrieval** | No at run time | The encoder is fetched at setup, and the sidecar runs offline. |
| **Update checks** | None found | No updater plugin or update endpoint is configured. |
| **Telemetry / analytics** | None found | No analytics code was found in the core or app. |
| **External links** | Opened in the user's browser | Only when clicked (`open_external`, http/https only). |
| **Source build** | Yes | `cargo`, `npm install` and `scripts/setup-mlx.sh` (pip + Hugging Face). |

**Safe summary line:** *Inference, tools, verification and logs run locally.
Network is used for model downloads and Hub metadata you ask for, for the
one-time engine installation, and for agent actions only when you grant
network access.*

**Do not say** "100% offline" or "never touches the network". The
installation and Model Manager need it, and Auto mode grants network to agent
commands.

---

## 19. Security and policy model

Current behaviour ([SECURITY.md](../SECURITY.md), `pwr-tools`):

- **Workspace trust (app).** The core is not started in a folder until the
  user trusts that exact path.
- **Permission modes (per workspace).** **Ask** (default) asks before
  dependency changes (manifests, lockfiles, installed packages), network
  access, toolchain installs, Git history rewrites and publishing; it grants
  local services and adopting a proposed check. **Auto** grants every
  approval category without asking but keeps confinement and policy
  refusals. A pre-mode configuration is read as Ask.
- **Grants.** Allow once (revoked after the action), allow for this session,
  or reject. `reject_always` is not offered (no denylist).
- **Scripted runs** grant only what `--approve` names.
- **Workspace boundaries.** Paths are resolved against the root, and escapes
  (including via move destinations) are refused. Seatbelt confines writes to
  the workspace and denies reads outside it, except system paths needed to
  start processes and known toolchain directories (`.cargo`, `.rustup`,
  `.nvm`, `/opt/homebrew`, Xcode, …).
- **Credentials.** `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.config/gh`,
  `~/.config/gcloud`, `~/.kube`, `~/.docker/config.json`, `~/.netrc` and
  `Library/Keychains` are denied to every sandboxed run.
- **`HOME` and `TMPDIR`** point inside the workspace.
- **Unconfined execution.** Refused where no sandbox applies (every non-macOS
  platform) unless `PWR_ALLOW_UNCONFINED=1`, which is then recorded as
  `sandboxed: false` and shown in the app. A command cannot print the
  sandbox's error to earn an unconfined retry (adversarial test).
- **Destructive actions.** Deletes are hash-guarded or explicitly recursive.
  Model deletion in the Model Manager needs confirmation and is guarded.
  History-rewrite Git arguments (`rebase`, `filter-branch`, `filter-repo`,
  `--force`, `-f`, `--amend`) need approval.
- **Protected state.** `.pwr/protected.json` paths cannot be written by
  `pwr run`; an unreadable file stops the run.
- **Installed dependencies** are read-only without `DependencyChange`.
- **Audit.** Every attempt, allowed or denied, goes into the hash-chained log.
  It is an inspectability aid, not tamper-proof storage against the machine's
  owner (audit).
- **Model repositories are data.** No remote code; sandboxed Jinja; no `.py`
  downloads.
- **Untrusted repository content.** A file that instructs the agent is prose
  and still has to pass policy.

**Known limits** (from SECURITY.md): `--provision` grants executables plus
network together; Linux and Windows have no sandbox adapter; `LocalService`
on Seatbelt covers every host address, not only loopback.

**Also in place:** a webview Content Security Policy, and Revert through the
core (`_pwr/revert`) with a version check and an audit record. `llama-server`
binds `127.0.0.1` only (`PWR_LLAMA_HOST` removed 2026-09-24).

---

## 20. Installation and distribution

### A) Prebuilt macOS DMG — the 0.1.0 release path

- `PWR_0.1.0_aarch64.dmg`, for Apple silicon (arm64) only, built by
  `npx tauri build` in `apps/desktop`.
- `PWR.app` bundles the release `pwr` core, the MLX engine's scripts and a
  portable `uv`. On first launch it installs the engine's Python (about
  1.2 GB) under `~/Library/Application Support/ai.pwr.desktop/engine`.
- **Ad-hoc signed** (`"signingIdentity": "-"`, commit `3ed8b41f`), so the
  bundle is sealed and `codesign --verify --deep --strict` passes. **Not
  notarized** and no Developer ID.

**Gatekeeper.** For an ad-hoc-signed, un-notarized app downloaded with a
browser, macOS refuses to open it on first launch and says it cannot verify
it. The wording varies by macOS version. The documented procedure is Apple's
own:

1. Open the DMG and drag `PWR.app` to `/Applications`.
2. Open `PWR.app` once. macOS blocks it; close the dialog (do not move it to
   the Bin).
3. Open **System Settings → Privacy & Security**, scroll to the message
   about PWR, click **Open Anyway** and confirm.
4. Later launches open normally.

Never recommend disabling Gatekeeper globally. **Decision (2026-09-24):**
`xattr` is not documented; only Apple's System Settings route is.

After the first launch, the app shows the engine setup screen and then points
to the Model Manager for a first model, which goes to `~/.pwr/models`.

### B) Build and run from the repository — IMPLEMENTED

Prerequisites (verified against [README](../README.md),
[apps/desktop/README.md](../apps/desktop/README.md), `Cargo.toml`,
`package.json`, `scripts/setup-mlx.sh`):
- Apple-silicon Mac (`setup-mlx.sh` exits otherwise).
- Rust **1.88+**.
- Node.js with npm (the lockfile names `npm@12.0.1` as package manager; the
  minimum Node version is not documented — **requires author confirmation**).
- `python3` for `setup-mlx.sh` (creates `.venv-mlx`).
- `uv` on `PATH` for a desktop bundle build (`brew install uv`), because
  `scripts/bundle-uv.sh` copies it.
- An MLX model folder under `~/.pwr/models/<publisher>/<name>` or
  `PWR_MLX_MODELS`, or download one from the Model Manager.

```bash
git clone https://github.com/VitoSanta/PWR.git
cd PWR
sh scripts/setup-mlx.sh                 # .venv-mlx: mlx, mlx-lm, mlx-vlm, mlx-embeddings + local encoder
cargo build --release                   # target/release/pwr
cd apps/desktop
npm install
npx tauri build --bundles app           # src-tauri/target/release/bundle/macos/PWR.app
```

Other verified commands:

```bash
npx tauri dev                           # app against the dev server (from apps/desktop)
target/release/pwr chat                 # terminal conversation in the current directory
target/release/pwr run "<task>" --model <ref>   # unattended; needs a probed capability artifact
cargo test --workspace                  # hermetic regression suite
```

Publication: the renamed history on `stage` replaces the old PoorAI history
on `main` with a force-push (decided 2026-09-24).

---

## 21. Current platform support

| Platform | Status | Basis |
|---|---|---|
| **macOS, Apple silicon** | **Supported (public alpha)** | The only target of the v0.1.0-alpha readiness assessment; MLX engine; Seatbelt sandbox; CI runs on `macos-14` (arm64). |
| **macOS, Intel** | **Unsupported** | MLX requires Apple silicon (the engine installer and `setup-mlx.sh` refuse). The llama.cpp path is not tested there. No build is configured. |
| **Windows** | **Planned** | A stated product target. Hardware detection and GGUF download code exist but have never run on Windows. No sandbox adapter (commands refused), and llama.cpp is not a performance path (E.1, E.2, R.4). |
| **Linux** | **Unsupported** | Declared "not a target" for the app. No sandbox adapter. Not in CI. |

---

## 22. Alpha status

**"Public alpha" means,** per README, SECURITY and the readiness document:

- **Implemented and exercised:** the conversation loop with goal mode,
  steering, approvals and stall detection; the MLX engine; the desktop app
  with streaming chat, diffs, queue, permissions, context panel and Model
  Manager; unknown-model handling; Quick Calibration; Reasoning Effort;
  sandboxed tools; repository checks; audited persistence. The full hermetic
  gate was green on 2026-09-24. Live runs (n=1 each, some steered) include:
  an empty crate verified by its own tests in about 5 minutes unattended; a
  website task passed through the app (Qwen3.6-35B-A3B, contract 4/4); a
  14-file game with 37 tests passed after steering; Quick Calibration on
  Qwen3-14B and gpt-oss-20b.
- **Known limitations:** two loops, not one runtime; token composition is
  estimated; no Verified models; Reasoning budgets are enforced only on the
  MLX engine for `<think>`-style templates; llama.cpp reloads per turn; the
  scripted loop still compacts differently; the fit estimate ignores speed
  and quality; prose tasks cannot be verified; no measurement of capability
  (every campaign so far ran under the old, starved regime and is not a
  capability claim).
- **Distribution limitations:** not notarized, ad-hoc signature only, so the
  first launch needs System Settings → Privacy & Security → Open Anyway. The
  readiness document's manual clean-Mac checklist still applies.
- **Platform limitations:** Apple silicon only.
- **Evolving areas:** unified runtime (R.11), small-model study (R.10),
  vision (C.25), fallback registry (C.24), onboarding (D.10), verification
  view (D.11), Windows.
- **Safety posture:** "Do not point it at anything you cannot afford to lose,
  and do not run it unattended on a repository you did not write"
  ([SECURITY.md](../SECURITY.md)).

---

## 23. Differentiators (evidence-backed)

Only themes with implementation evidence are kept. None is claimed as unique
in the industry. Prior art is acknowledged in
[local-agent-research.md](local-agent-research.md).

1. **Provenance-scoped model compatibility.**
   *What:* untested models are Provisional, not refused. A nine-check
   mechanical calibration assigns a status tied to an exact artifact
   fingerprint, with deterministic stale/reduced/applies rules.
   *Why:* local models vary by quantization, template and engine version;
   evidence from one artifact must not leak to another.
   *Evidence:* [profile.rs](../crates/pwr-models/src/profile.rs), [calibration.rs](../crates/pwr-models/src/calibration.rs), [compatibility.rs](../crates/pwr-models/tests/compatibility.rs).
2. **Hardware-derived context window, with every ceiling named.**
   *What:* the window is computed from host memory and the model's
   `config.json`, not probed, and the Model Manager's fit rating uses the
   same arithmetic.
   *Why:* the project's own history shows a starved window was the dominant
   cause of failure.
   *Evidence:* [window.rs](../crates/pwr-orchestrator/src/window.rs), [fit.rs](../crates/pwr-models/src/fit.rs); [roadmap](roadmap.md).
3. **Engine-enforced reasoning budgets that cannot destroy the reply.**
   *What:* PWR's own MLX engine counts thinking tokens with the model's
   tokenizer, closes the phase with the template's own delimiter once, keeps
   an answer reserve, and retries once without thinking.
   *Why:* local reasoning models can spend a whole turn thinking (Qwen3.6
   reasoned for 56,059 characters in one turn, backlog C.15).
   *Evidence:* [pwr_mlx.py](../crates/pwr-mlx/sidecar/pwr_mlx.py), [reasoning_effort.rs](../crates/pwr-domain/tests/reasoning_effort.rs), [two_loops.rs](../crates/pwr-cli/src/two_loops.rs); live Low-effort run closed at exactly 2,048 tokens.
4. **Acceptance-contract goal verification.**
   *What:* goal mode is verified only by pre-existing, hash-pinned acceptance
   checks, so the agent cannot write or relax its own judge.
   *Why:* a model-authored passing test is not independent evidence.
   *Evidence:* `acceptance_contract_hash`, `verify_goal` ([serve.rs](../crates/pwr-cli/src/serve.rs), [main.rs](../crates/pwr-cli/src/main.rs)).
5. **Mechanical, audit-sourced compaction.**
   *What:* no model writes the summary. Changed files and hashes come from the
   audit, and open errors and the last verdict are preserved.
   *Why:* model summaries can drop critical evidence, a risk the research
   notes flag for weaker models.
   *Evidence:* [compaction.rs](../crates/pwr-orchestrator/src/compaction.rs), [context_compaction.rs](../crates/pwr-orchestrator/tests/context_compaction.rs), [compaction_fidelity.rs](../crates/pwr-orchestrator/tests/compaction_fidelity.rs).
6. **Bounded, named recovery.**
   *What:* every retry path has a small consecutive bound and a
   plain-language stop reason mapped to a terminal class.
   *Why:* local models fail in repetitive ways (silence, runaway replies,
   stalls), and each bound in the code cites the run that motivated it.
   *Evidence:* [converse.rs](../crates/pwr-orchestrator/src/converse.rs), [stall.rs](../crates/pwr-orchestrator/src/stall.rs), orchestrator tests.
7. **Effect-based authorization with an OS sandbox.**
   *What:* approvals name effects (network, dependency change, toolchain
   install, publish…). Commands are argv-only under a Seatbelt profile, with
   credentials denied and HOME redirected, and are refused where no sandbox
   exists.
   *Evidence:* [pwr-tools](../crates/pwr-tools/src/lib.rs), [adversarial.rs](../crates/pwr-tools/tests/adversarial.rs), [sandbox_and_approvals.rs](../crates/pwr-tools/tests/sandbox_and_approvals.rs).
8. **Measurement discipline built into the code.**
   *What:* strict paired comparison refuses undeclared differences, a
   conventional-loop baseline arm (B0) exists, and diagnostic detectors are
   derived from real traces.
   *Why:* the stated goal is to attribute gains to the harness honestly.
   *Evidence:* [pwr-eval](../crates/pwr-eval/src/lib.rs), [baseline.rs](../crates/pwr-orchestrator/src/baseline.rs), [pwr-observe](../crates/pwr-observe/src/diagnose.rs).
9. **Protocol-first architecture.**
   *What:* the app is a pure ACP client. Policy, sandbox, audit and
   verification live only in the core, and the protocol is validated against
   the ACP schema.
   *Evidence:* [serve.rs](../crates/pwr-cli/src/serve.rs), [fixtures/acp](../crates/pwr-cli/tests/fixtures/acp/). Since 2026-09-24 this includes Revert.

**Not retained as a differentiator (insufficient evidence):** "small-model
amplification" or any uplift figure. The R.10 study has not run, and the
headline metric is defined but unmeasured.

---

## 24. Competitive positioning without marketing

**Category.** Agentic coding tools that act in a repository (read, edit, run,
verify) from a conversation. Within it, PWR is a **local-only, open-source
desktop harness for open-weight models on Apple silicon**.

**Design focus and priorities:**
- Running the model locally on the user's own engine, with no cloud service
  in the inference path.
- Adapting to the specific deployment (artifact, template, memory) rather
  than to a model brand.
- Making verification deterministic and evidence explicit, and saying
  "not verified" when that is the truth.
- Bounding every recovery and naming every stop.
- Measuring what the harness contributes, against a fixed baseline, with
  negative results retained.

**Relationship to other tools.** The research document studies Claude Code,
Codex, OpenCode, Cline, Roo Code, Aider, Continue, SWE-agent, OpenHands and
others as *mechanism prior art* ("not product rankings"). PWR makes **no
comparative performance claim**. The README says it is "not a claim … that
PWR already competes with Codex or Claude Code". On the MLX engine, one
2026-09-18 comparison with another harness (Bionic) found 4 of 5 on five
tasks for both. It is an outside reference, not a ranking.

---

## 25. Screenshot and product-story map

**Inventory found:** one file,
`docs/product-screenshots/04-empty-state-chat-light.jpg` (2720×1718, light
theme). It shows an empty "New conversation" in chat mode (a "Read-only chat"
chip), the model chip "Qwen3.6 35B A3B" with a 262k context indicator, the
inspector on Changes ("No changes yet"), and a collapsed sidebar rail. The
numbering (`04-`) suggests others are planned but not present.

Candidates below map features to shots worth capturing. **No final
selection is made here.**

**HERO CANDIDATES**
- A workspace conversation mid-task: action feed with an edit, and the
  Changes panel open on a diff.
- Goal mode ending with the check verdict / "verified" note next to the
  Evidence panel showing Verify output.
- The Model Manager Discover view with fit ratings for this Mac.

**CORE PRODUCT SCREENSHOTS**
- Empty chat state (exists: `04-empty-state-chat-light.jpg`).
- Permission prompt (Ask mode) for a dependency change: allow once / session
  / reject.
- Streaming reply with the reasoning section.
- Changes panel with multiple files and +/- counts.
- Conversation list in the sidebar; resume note ("files edited since").
- Dark theme version of a working session.

**TECHNICAL FEATURE SCREENSHOTS**
- Context panel: used / window, estimated composition, auto-compact
  threshold, Compact now, last reply's reasoning/answer tokens.
- Model popover: compatibility status, "New model detected", Quick
  Calibration progress and result, Reasoning Effort selector (and its
  disabled-with-reason state).
- Fit-rating explanation popover with assumptions.
- Download in progress / verifying / paused with resume.
- Evidence panel: Diagnose or Report output.
- First-run engine setup screen with progress.
- A compaction note in the conversation.
- Image attached to a vision model ("sees images" marker).

**DOCUMENTATION-ONLY SCREENSHOTS**
- Workspace trust dialog.
- Settings (engine location, appearance).
- Command palette.
- Core log panel.
- Narrow-window layout (rail + overlay inspector).
- Terminal `pwr report` / `pwr diagnose` output.
- Model deletion confirmation.

**NOT WORTH SHOWING PUBLICLY**
- Any screen showing "commands not sandboxed" / `PWR_ALLOW_UNCONFINED` as a
  normal state.
- Error states that expose local paths (Core log with home paths).
- Demo-mode (`?demo`) captures presented as real sessions without saying so.
- Model Manager with Hub rate-limit errors.
- Anything showing the old poorAI name or icon.

---

## 26. Website content building blocks — SOURCE COPY

**Hero headline candidates**
- Local models. Real repositories. Checked work.
- A coding agent that runs on your Mac and shows its evidence.
- Your repository, a local model, and checks you can see.

**Hero subheadline candidates**
- PWR is an open-source desktop agent for Apple-silicon Macs. It runs
  open-weight models on its own local engine, edits and runs code inside a
  sandbox, and verifies changes with your repository's own checks.
- An experimental harness that asks one question: with the same model and
  the same task, how much does a better harness help?

**Why PWR**
Local models are capable but uneven. Tool calls break, context runs out and
reasoning runs long, and a valid response is not a finished task. PWR puts
the model in an engineering environment designed around those failures:
typed tools, a sandbox, a context window sized for your machine, bounded
recovery, and verification that doesn't take the model's word for it.

**How it works**
1. Open a folder and trust it. Pick a model that fits your Mac.
2. Ask for what you want. PWR reads, searches, edits and runs commands
   through typed tools inside a macOS sandbox.
3. Anything that reaches beyond the workspace asks first (Ask mode), unless
   you choose Auto.
4. After edits, PWR runs your repository's own checks and tells you what they
   said. In Goal mode it keeps going until your acceptance check passes.
5. Every change is a diff, and every action is in a local, hash-chained log.

**Core capabilities** (each IMPLEMENTED): streaming agent conversation with
steering and stop; hash-guarded edits and sandboxed commands; local service
start/stop; Git status/diff; repository check discovery across 19 build
systems plus declared checks; goal mode with acceptance contracts; automatic
and manual compaction; Model Manager with fit ratings and verified
downloads; Quick Calibration and Reasoning Effort; chat without a workspace;
attachments; audit, report and diagnose.

**Local-first section**
Inference runs on your Mac through PWR's own MLX engine, with no cloud API
and no account. Conversations, logs and model evidence stay on your
machine. PWR uses the network when you download a model or browse Hugging
Face, during the one-time engine installation, and for agent actions only
when you allow network access.

**Models and hardware section**
PWR reads your Mac's memory and a model's configuration to compute a context
window that fits, and it names the limit that set it. The Model Manager rates
each variant for your machine before you download it. A model PWR has never
seen is usable right away as *Provisional*; a nine-check Quick Calibration
tells you whether it can drive agent work.

**Evidence and verification section**
PWR separates what changed (diffs) from what was established (checks). It
runs your repository's own build and test commands, before and after it
edits, and never calls a pre-existing failure green. In Goal mode, only an
acceptance check you declared beforehand, unchanged since the session
started, can mark the goal verified. No model grades another model.

**Open source section**
Apache-2.0. Rust core, Tauri + Angular app, Python MLX sidecar. Over a
thousand hermetic tests run on every change without a model or network. The
design documents record what was measured, what failed and what was removed.

**Alpha download section**
PWR is a public alpha for Apple-silicon Macs. It is not notarized yet, so
macOS will ask you to confirm the first launch in System Settings → Privacy &
Security. It runs model-generated commands against your files: use it on
repositories you can afford to lose, and read SECURITY.md first.

---

## 27. GitHub README source material

**Repository description (≤350 chars):**
> Local coding-agent harness and desktop app for Apple-silicon Macs. Runs
> open-weight models on its own MLX engine, edits and runs code in a macOS
> sandbox, and verifies with your repo's own checks. Public alpha, Apache-2.0.

**README introduction:**
> PWR is an experimental, open-source coding agent that runs entirely on
> your Mac's local models. A Rust core provides typed, sandboxed tools,
> computed context windows, bounded recovery, deterministic verification
> and a hash-chained audit log. A Tauri desktop app drives it over the Agent
> Client Protocol. It is also a research platform asking how much a harness
> changes what the same local model can finish.

**Feature overview:** reuse §26 "Core capabilities", with the status table
from the current README.

**Architecture overview:**
```text
PWR.app (Tauri 2 + Angular)  ──ACP/JSON-RPC over stdio──▶  pwr serve (Rust core)
                                                            ├─ converse loop (gate → perform)
                                                            ├─ pwr-tools (policy, Seatbelt, executor)
                                                            ├─ pwr-verify (check discovery, baseline)
                                                            ├─ pwr-repo (index, retrieval)
                                                            ├─ pwr-models (Hub, fit, download, calibration)
                                                            ├─ pwr-store (SQLite, hash chain)
                                                            └─ pwr-mlx ──▶ Python sidecar (mlx-lm / mlx-vlm)
```

**Installation:** §20 B, verbatim commands.

**Alpha warning:**
> Public alpha, not production-ready. PWR runs a language model's output as
> commands against your repository. Commands are sandboxed on macOS only. Do
> not point it at anything you cannot afford to lose. See SECURITY.md.

**Contributing / open source:**
> Contributions are welcome. Run what CI runs (`cargo fmt --check`, `clippy
> -D warnings`, `cargo test --workspace`, the milestone check) and judge by
> exit code. A change carries a test that fails without it, and a claim
> carries a status word and its evidence. See CONTRIBUTING.md.

---

## 28. CV source material

**One-line**
> Designed and built PWR, an open-source local coding-agent harness in Rust
> with a Tauri/Angular desktop app. It runs open-weight LLMs on a custom
> MLX engine, with sandboxed tools, deterministic verification and a
> hash-chained audit.

**2-bullet**
- Architected a 15-crate Rust workspace for a local LLM coding agent. It
  covers the conversation loop, typed tool executor under a macOS Seatbelt
  sandbox, repository indexing with BM25 retrieval, check discovery across 19
  build systems, SQLite hash-chained event log, and an Agent Client Protocol
  server, with over 1,000 hermetic tests and CI.
- Built an MLX inference sidecar with engine-enforced reasoning budgets and
  prompt caching, provenance-scoped model calibration, hardware-derived
  context sizing, and a Tauri 2 + Angular desktop client with a Hugging Face
  model manager (checksum-verified, resumable downloads).

**3-bullet**
- Built PWR, an Apache-2.0 local agent harness: Rust core (15 crates), Tauri
  2 + Angular desktop app, Python MLX sidecar, over the Agent Client Protocol
  (JSON-RPC over stdio) with schema-validated messages and golden
  transcripts.
- Engineered harness mechanisms for smaller open-weight models: typed,
  hash-guarded tools; argv-only sandboxed execution with effect-based
  approvals; mechanical context compaction; bounded, named recovery; and
  deterministic verification against the repository's own checks, including
  hash-pinned acceptance contracts.
- Implemented model and hardware awareness: memory-derived context windows,
  per-machine fit ratings, a nine-check mechanical Quick Calibration with
  provenance-based evidence reuse, and Low/Medium/High reasoning budgets
  enforced in the inference engine. The research framework includes paired
  baselines and strict trial comparison.

(No user numbers, stars, benchmarks or performance gains exist to cite.)

---

## 29. LinkedIn source material

**Project title:** PWR — local coding-agent harness for open-weight models

**Very short description:** An open-source desktop coding agent that runs
local LLMs on Apple silicon and verifies its work with your repository's own
checks.

**Project description:**
PWR is a public-alpha, open-source (Apache-2.0) coding agent that runs
entirely on local, open-weight models. It is built around one question: with
the same model and the same task, how much can the harness around the model
improve what gets done? It pairs a Rust core with a Tauri desktop app. The
agent reads, searches, edits and runs code through typed tools inside a macOS
sandbox. Actions that reach beyond the workspace ask first. After editing,
PWR runs the repository's own checks, and every action is recorded in a local
audit log. PWR sizes the context window from the machine's memory, treats
untested models as provisional until a short mechanical calibration, and
enforces reasoning budgets inside its own MLX engine. It makes no claim of
parity with cloud agents; measuring the harness's contribution honestly is
the point.

**Technologies:** Rust (Tokio, serde, rusqlite, reqwest/rustls, clap,
ratatui), Tauri 2, Angular 22 (signals, zoneless), TypeScript, Vitest,
Python, MLX (`mlx-lm`, `mlx-vlm`, `mlx-embeddings`), llama.cpp, Hugging Face
Hub API, Agent Client Protocol (JSON-RPC 2.0), SQLite, macOS Seatbelt, GitHub
Actions.

---

## 30. Claims that must not be made

| Claim | Why it is wrong or unsupported today |
|---|---|
| "Supports every LLM" / "any model" | MLX and GGUF only; MLX models need a chat template; `auto_map` repos are incompatible; GGUF is limited. |
| "Fully offline" / "100% offline" / "never uses the network" | Engine install, Model Manager and downloads use the network, and Auto mode grants network to agent commands. Say instead "inference runs locally". |
| "No cloud" without qualification | True for inference; Hugging Face and package hosts are contacted for installs and downloads. |
| "Beats / matches Claude Code, Codex, Cursor, Cline…" | No comparative measurement exists. |
| "X% better", "N× more capable", "amplifies small models" | R.10 has not run; no uplift is established. R2 found no uplift under the old regime. |
| "Production ready" / "stable" / "enterprise" | Public alpha; SECURITY.md warns against unattended use. |
| "Works on Windows / Linux / Intel Macs" | Apple silicon only. |
| "Verified models" / "certified models" | The Verified registry is empty. |
| "Benchmarked" / "state of the art" | Suites are regression checks; live runs are n=1 diagnostics. |
| "Fully sandboxed" / "safe to run unattended" | Sandbox is macOS-only; `--provision` and Auto widen access; reads of system/toolchain paths are allowed; Revert bypasses the audit; CSP is off. |
| "Tamper-proof audit" | The hash chain aids inspection; it is not tamper-proof against the machine's owner. |
| "Understands your codebase" / "semantic code graph" / "AST-aware" | Lexical plus shallow symbol/import heuristics; the semantic filter is opt-in and for docs sections. |
| "Proves your code is correct" | Checks prove what they test; prose tasks are not verifiable. |
| "Vision / sees your screen / computer use" | Images for vision models are experimental; no screenshot capture or computer control exists. |
| "MCP support", "browser automation", "plugins" | Not implemented. |
| "One agent runtime for chat and automation" | Two loops (R.11). |
| "Notarized / signed macOS app" | Ad-hoc signature only; not notarized. |
| "Faster inference" | Not measured as a claim; a spike measured LM Studio's MLX path as fast or faster at prefill. |
| "Remembers across sessions / learns" | Conversations resume; there is no cross-task learned memory. |
| "Reasoning Effort makes answers better" | It bounds thinking tokens only; documented as not a quality setting. |
| "Supports 262k context" as a product promise | The window depends on the machine; 262k is not a requirement. |

---

## 31. Evidence index

| Claim | Source | Tests | Docs / runs |
|---|---|---|---|
| App is an ACP client of the core | `apps/desktop/src-tauri/src/lib.rs`, `crates/pwr-cli/src/serve.rs` | `serve.rs` tests; `crates/pwr-cli/tests/fixtures/acp/` | `docs/pwr-serve.md` |
| Conversation loop and stop reasons | `crates/pwr-orchestrator/src/converse.rs`, `session.rs`, `stall.rs`, `repetition.rs` | `crates/pwr-cli/src/two_loops.rs`; `crates/pwr-orchestrator/tests/{loop_detection,malformed_and_flaky,runaway_reply,action_budget}.rs` | audit |
| Goal mode and acceptance contract | `serve.rs` (`GOAL_MAX_ACTIONS`, `acceptance_contract_hash`), `main.rs` (`verify_goal`) | `goal_mode_continues_past_a_checkpoint_and_requires_full_verification` | — |
| Check discovery and baseline | `crates/pwr-verify/src/lib.rs`, `web.rs` | `crates/pwr-verify/tests/*` | `docs/verification-recovery.md` (historical) |
| Tools, policy, sandbox | `crates/pwr-tools/src/lib.rs`, `service.rs`, `document.rs` | `crates/pwr-tools/tests/{adversarial,sandbox_and_approvals,filesystem_and_vcs,harness_torture,services}.rs` | `SECURITY.md` |
| Compaction | `crates/pwr-orchestrator/src/compaction.rs` | `context_compaction.rs`, `compaction_fidelity.rs`, `compaction_keeps_the_last_read.rs` | `docs/models-and-context.md` |
| Computed window | `crates/pwr-orchestrator/src/window.rs` | module tests; `occupancy.rs` | `docs/redesign-2026-09-17.md` Part D |
| Fit rating, Hub, downloads | `crates/pwr-models/src/{fit,hub,catalog,download,local}.rs` | `crates/pwr-models/tests/{models,local}.rs` | `docs/models-and-context.md` |
| Compatibility states, calibration, provenance | `crates/pwr-models/src/{profile,calibration}.rs`, `verified-models.json` | `compatibility.rs`, `live_model.rs` (ignored) | `docs/model-compatibility.md`; readiness |
| Reasoning Effort | `crates/pwr-domain/src/reasoning.rs`, `crates/pwr-mlx/sidecar/pwr_mlx.py` | `reasoning_effort.rs`, `test_pwr_mlx.py`, `two_loops.rs` | readiness (live Qwen3-14B, gpt-oss-20b) |
| Repository index and retrieval | `crates/pwr-repo/src/lib.rs`; `crates/pwr-mlx/src/embed.rs` | `crates/pwr-repo/tests/*` | backlog C.22 |
| Hardware profile | `crates/pwr-runtime/src/host.rs`, `hardware.rs` | module tests | `docs/models-and-context.md` |
| Audit chain | `crates/pwr-store/src/lib.rs` | `crates/pwr-store/tests/chain.rs`, `crates/pwr-orchestrator/tests/audit.rs` | `SECURITY.md` |
| Diagnosis | `crates/pwr-observe/src/diagnose.rs` | `crates/pwr-observe/tests/*` + real-trace fixtures | — |
| Evaluation arms | `crates/pwr-orchestrator/src/baseline.rs`, `crates/pwr-eval/src/lib.rs` | `crates/pwr-eval/tests/*`, `baselines.rs` | `docs/evaluation.md`; backlog R.10 |
| Engine installer | `apps/desktop/src-tauri/src/engine.rs`, `scripts/bundle-uv.sh` | — (manual) | commit `1c1c62c3` |
| Bundle/signing | `apps/desktop/src-tauri/tauri.conf.json` | — | commit `3ed8b41f` |
| Desktop UI | `apps/desktop/src/app/**` | 7 `*.spec.ts`, 34 tests | `apps/desktop/README.md` |
| Live product runs | — | — | backlog D.E2E-*, roadmap 2026-09-22/23 |
| Status and limits | — | — | `README.md`, `docs/release/v0.1.0-alpha-readiness.md`, `docs/backlog.md` |

---

## Appendix A — Documentation inconsistencies found

**Status 2026-09-24 (release pass):** items 1–13, 15 and 17 were corrected in
the documents and code named. Item 14 is kept deliberately: packaged profiles
match model references, including historical ones. Item 16 was resolved by
removing the fallbacks from `scripts/pwr`, `suites/run.sh` and the app. The
items below are kept as the record of what was found.

1. **MASTER_SPEC lists "model downloading" under "Not yet, and
   deliberately"**, but the Model Manager and `pwr models download` implement
   it.
2. **Readiness BLOCKING items are stale.** Items 1–2 were partly addressed
   after assessment: the engine installer was added and the core is now a
   bundle resource. The sidecar script is still not bundled (§20). Items 3
   (CSP) and 4 (Revert) are still open.
3. **The README lists images for vision models under "RESEARCH / IDEAS"**,
   while backlog C.25, `pwr-serve.md` and `current-cli.md` describe steps 1–2
   as built.
4. **`apps/desktop/README.md` says "the app never writes to a workspace
   itself"**, but Revert does (`restore_workspace_file`).
5. **Backlog D.10 says onboarding has "neither a design nor a line of
   code"**, but the first-run engine setup screen exists.
6. **The roadmap header still reads "PAUSED, 2026-09-17"** above later
   dated updates.
7. **The architecture document's component table** says "KEEP ratatui
   frontend … No API server currently". `pwr serve` and the Tauri app
   supersede this (the table is labelled a 2026-09-12 PLANNED target).
8. **Rename artefacts (sed-style capitalization):** "`PWR --backend llama
   chat`" (current-cli.md), "`PWR verify`" (CLI-spec.md), "`PWR --cli`"
   (manual-testing.md), and "the scripted research loops (`PWR run`, …)"
   (model-compatibility.md). The binary is lowercase `pwr`.
9. **The glossary calls calibration "the v5 protocol"**, while the audit
   says v6.
10. **Frontend test counts:** the readiness document says 4 files and 17
    tests; the current tree has 7 files and 34 tests.
11. **The CI workflow comment** mentions "Nothing here starts Ollama", a
    removed backend.
12. **`package.json` version `0.0.0`**, against Tauri/Cargo `0.1.0`.
13. **Default model folder `~/.lmstudio/models`** is named after LM Studio,
    which is no longer used. This may confuse users.
14. **`strategies/models.json` selectors** include Ollama-era tags
    (`qwen3.8:27b-mlx`, `gpt-oss:20b`) for removed backends.
15. **`pwr-serve.md` and MASTER_SPEC** describe the app as "for macOS and
    Windows" in the present tense. Windows is not implemented.
16. **`current-cli.md` and `engine.rs`** still fall back to
    `experiments/engine-spike-mlx-20260917/.venv` on the maintainer's machine
    (development builds only).
17. **SECURITY.md cites a `--allow-remote-endpoint` flag** ("The model
    backend is trusted to be local") that was removed with the HTTP backends
    on 2026-09-19 (`b865aabd`). No such flag exists. The MLX engine has no
    endpoint at all. `PWR_LLAMA_HOST` can bind llama.cpp to a non-loopback
    address without a refusal.

## Appendix B — Terminology used in this document

PWR (product), `pwr` (binary), harness (everything around the model), agent
(model + harness in a workspace), model (weights + tokenizer + template),
runtime/backend/engine (MLX sidecar or llama-server), tool (typed action),
context/window (what one generation is sent / what the engine serves),
verification (running the repository's checks), evidence (observations with
provenance), workspace (the trusted folder), audit (hash-chained event log),
Core log (stderr panel).
