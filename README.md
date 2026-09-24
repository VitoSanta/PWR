# PWR

**An R&D project investigating how much agent capability a harness can extract from local LLMs. Software engineering is the first proving ground.**

[![CI](https://github.com/VitoSanta/PWR/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/VitoSanta/PWR/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-2f718e.svg)](LICENSE)

PWR is an experimental local agent harness that gives locally served models an engineering environment: repository access, editing, commands, verification, persistent evidence, and an inspectable conversation. Its research goal is to measure model behavior and adapt the representation of tools, context, and task state so that imperfect models spend less inference on mechanical work and recover from predictable failures. The target is sustained autonomous engineering, from investigating an unfamiliar repository to implementing and checking multi-file changes.

The central question is **same model + same task + different harness: what improves, at what resource cost?** This is not a claim that local weights equal frontier models, or that PWR already competes with Codex or Claude Code. Harness-level competitiveness is a long-term ambition.

**What it is meant to become:** a coding agent with no capability ceiling, driven from a conversation. Anything on the machine it runs on and anything on the network the user authorizes — files, shells, processes, toolchains, Git, local applications and services, a browser, the open internet — reachable by asking for it in the chat, with the scope of each grant visible and revocable. The authorization model exists to make that reach explicit, not to keep the agent small, and ease of use is a release condition rather than a finishing touch: a capability that needs a remembered flag or a raw log to drive is not finished. Today's implementation is much smaller than that, and the table below says how much.

## Why this project exists

Local API compatibility does not imply reliable agent behavior. Tool-call syntax, evidence retention, edit accuracy, and latency depend on the actual model, quantization, serving backend, template, and loaded context. PWR will test three mechanisms: deployment-conditioned action representations; source-linked working state and context; and recovery driven by observed effects. Each must beat a strong fixed-policy baseline before becoming a default. These are research hypotheses, not unique inventions or demonstrated advantages.

Initial users are developers and agent researchers working on repositories they control, with one local inference host. macOS on Apple silicon is the current execution laboratory. Smaller deployments are part of the intended research cohort; no particular parameter count or 262K context window is a product requirement.

## Current maturity

Public alpha, not production-ready. Status as of **2026-09-24**; the evidence
behind each line is in [`docs/roadmap.md`](docs/roadmap.md) and
[`docs/backlog.md`](docs/backlog.md). Nothing here is a capability claim: the
live runs below are single diagnostic runs, not a benchmark.

| Status | What exists |
|---|---|
| **IMPLEMENTED, run live** | PWR's own **MLX engine** (a sidecar over mlx-lm that renders chat templates, controls reasoning, and keeps a prompt cache that holds across a turn); a **conversation loop** with goal mode, steering, approvals, stall detection and a record of every generation; **ACP over stdio** (`pwr serve --stdio`); a **desktop app** (Tauri 2 + Angular, `apps/desktop`) with streaming chat, diffs, a message queue and permissions, a context panel with manual and automatic compaction, and a **Model Manager** that rates Hugging Face models for this machine and downloads them verified ([details](docs/models-and-context.md)); untested models run **Provisional** with conservative defaults, and a bounded, mechanically scored **Quick Calibration** marks them locally calibrated or limited; **Reasoning Effort** (Low / Medium / High) sets a thinking budget where the engine can enforce one, clamped to the context ([details](docs/model-compatibility.md)); typed, audited tools for files, search (including the project's installed dependencies, read-only), edits, commands, services and Git; repository check discovery and verification against acceptance contracts; persisted events, reports and diagnosis. Live, unattended: a task from an empty crate verified by its own tests in five minutes; a website and a 37-test game built through the app, with steering. |
| **IMPLEMENTED, OPT-IN** | A local **context filter**: document sections ranked by BM25 fused with a small embedding model (`multilingual-e5-small`) running offline on MLX (`POORAI_SEMANTIC_RETRIEVAL=1`). Measured on twelve labelled requests and one small-model run per arm; not yet a default. |
| **IMPLEMENTED, LIMITED** | **llama.cpp / GGUF** (`--backend llama`): metadata, a managed `llama-server`, streaming generation -- but the server is started per generation, so it reloads the model every turn; not yet a performance path. Evaluation runner with a strict paired-trial comparator; the terminal console. |
| **PLANNED** | One runtime contract shared by the conversation and scripted runs (the two loops share tool execution and approvals, not planning, compaction or completion); Windows and Linux process sandboxes; a fixed tool-loop baseline for measuring harness uplift. |
| **RESEARCH / IDEAS** | Harness uplift for 7–14B models; a per-model registry of fallbacks learned from the audit; images for models with a vision encoder; MCP, browser and computer control as extensions, one at a time. |

## Try the experimental implementation

Requires an Apple-silicon Mac, Rust 1.88+, and MLX model folders under
`~/.lmstudio/models` (LM Studio's folder, used only as storage -- LM Studio
itself is not needed) or `POORAI_MLX_MODELS`. PWR runs models itself and
uses no cloud service.

```bash
sh scripts/setup-mlx.sh        # the engine's Python environment (.venv-mlx) and the local encoder
cargo build --release          # the core, target/release/pwr
cd apps/desktop && npm install && npx tauri build --bundles app   # the desktop app
```

The app starts the core itself. From a terminal, `target/release/pwr chat`
opens a conversation in the current directory and `pwr run "<task>"` runs a
task unattended; `--help` on any command lists its flags, and
[`docs/current-cli.md`](docs/current-cli.md) has a manual path including GGUF.
`POORAI_MLX_PYTHON` names another interpreter for the engine.

**Permissions.** A conversation runs in one of two modes, chosen per workspace
in the app: **Ask** (the default) asks before changing dependencies, reaching
the network, installing toolchains, rewriting Git history or publishing;
**Auto** grants all of those without asking. In both, commands the model runs
are confined by macOS's sandbox to the workspace. **Only macOS has a sandbox
adapter**: elsewhere a command is refused unless `POORAI_ALLOW_UNCONFINED=1`
is set, and the app says when commands are not sandboxed. Read
[SECURITY.md](SECURITY.md) first.

```bash
cargo test --workspace         # the offline regression suite -- harness contracts, not model quality
```

## Read the design

| Document | Authority |
|---|---|
| [MASTER_SPEC.md](MASTER_SPEC.md) | Project definition, design principles, scope and evidence vocabulary |
| [Glossary](docs/glossary.md) | What each term means across the canonical set, and which three were ambiguous before |
| [Research](docs/local-agent-research.md) | Prior art, research gap, hypotheses and falsification decisions |
| [Architecture](docs/architecture.md) | Candidate runtime, context, tools, policy, multimodality and agent UX |
| [Evaluation](docs/evaluation.md) | Controls, measurements, provenance and promotion gates |
| [Audit](docs/adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md) | What the code actually contains; subsystem disposition |
| [Migration](docs/adaptive-runtime/MIGRATION_PLAN.md) | Replacement order, historical decisions and documentation map |
| [Roadmap](docs/roadmap.md) | Sequential research milestones, followed by preserved historical records |

These documents supersede older product requirements. Dated implementation notes, ADRs and [experiment logs](docs/experiment-log.md) remain evidence about their recorded revisions; the ADR series is closed at ADR-012 and new durable decisions amend the canonical documents instead. No historical measurement is silently promoted to a result for the new design.

Built and maintained by [Vito Santanelli](https://github.com/VitoSanta). [Contributing](CONTRIBUTING.md) · [Apache-2.0](LICENSE).
