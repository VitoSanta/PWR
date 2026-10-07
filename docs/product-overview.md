# Product overview

**Development snapshot: 2026-10-07, `develop` at `5f6c4f38`.** This page
describes committed product behaviour. The latest published release remains
**v0.2.0-alpha (2026-09-28)**; unreleased changes below are not in that DMG.
Uncommitted interface changes are not release evidence.

## Purpose and intended use

PWR is an open-source local coding agent for Apple-silicon Macs. It combines
a managed MLX runtime, a desktop conversation, repository tools, inspectable
changes and project checks. The model proposes actions; the harness controls
file versions, permissions, execution, verification and budgets.

The current supported use is supervised work on bounded bugs, features and
refactors in recoverable repositories. Creating projects is implemented, but
results on small-model tasks are variable. The project does not yet demonstrate
general capability gains over other agents using the same model and budget.

Inference does not require a cloud API or a PWR account. Downloading models,
installing dependencies and authorised network tools can contact external
services. Local inference is not a promise that every operation is offline.

## Components

- **Desktop:** Angular and Tauri; conversations, model controls, diffs,
  terminal tabs, file browsing, project knowledge and a local web preview.
- **Rust core:** `pwr serve --stdio`, using ACP/JSON-RPC with `_pwr/*`
  extensions. The interface asks the core to perform changes, including Revert.
- **Session executor:** sequences conversation and Goal mode for desktop and
  console. The scripted `pwr run` / `pwr eval run` loop remains separate.
- **MLX sidecar:** a managed Python process with streaming, cancellation,
  family templates, prompt cache and runtime metrics.
- **Verification and evidence:** check discovery, baseline comparison,
  frozen acceptance artifacts and a local SQLite event log.

See [architecture](architecture.md), [inference](inference.md) and
[the protocol](pwr-serve.md) for the component boundaries.

## Available features and their limits

| Feature | Current behaviour | Boundary |
|---|---|---|
| Operational conversation | Read, search, edit, run commands, stream answers, steer and Stop | Supervised; model loops and backend failures remain possible |
| Versioned edits | Hash guards, exact replacements, atomic file writes, diffs and conflict-aware Revert | No global filesystem transaction or arbitrary command rollback |
| Project checks | Explicit checks, CI/scripts/build-system discovery, including nested projects | Passing checks establish only what they exercise |
| Goal mode | Bounded continuation, technical checks, request review and declared acceptance | A model's completion or review cannot certify acceptance by itself |
| Frozen acceptance | Persisted hashes of selected test/configuration artifacts, checked before and after | Inferred artifacts cannot cover every indirect dependency |
| Permissions | Protected, Standard and Full access; requests identify the action | Full access removes command confinement |
| Model Manager | Hugging Face MLX discovery, memory fit estimates, downloads with verification/resume | Physical fit and calibration are not task-quality measurements |
| Quick Calibration | Local compatibility checks with adapter provenance | Not a benchmark or general model certification |
| Context continuity | Objective revisions retained, compaction, checkpoints and resume | Some context estimates remain heuristic; restored traces are partial |
| Terminal diagnostics | Read recent terminal output with a dedicated conversation permission | Desktop support required; reading is distinct from executing |
| Local page diagnostics | Rendered text/console; vision models also receive an image | Fresh browser profile, no shared preview login; experimental |
| Page steps | Up to 12 click/type/press steps, option selection and scrolling, then a screenshot | DOM operations and synthetic events; framing/trusted-event limitations |
| Screenshot in chat | ACP image block displayed under the completed page action | PNG transport capped at 6 MiB; not full historical trace persistence |
| Project knowledge | Wiki, outline and project recall; optional semantic retrieval and 3D graph | Projections/model summaries are not authoritative verification |
| Chat without workspace | Read-only attached references and conversation | No project edits or command execution |

### Current defaults

Conversation compaction uses **75% of the granted context window, with no
default token ceiling**. A workspace may set `compact_ceiling_tokens`;
collapsed-reply recovery can compact earlier. The abandoned 32k ceiling is
historical, not the current default.

Ordinary turns default to 100 actions. Goal defaults are 208 actions,
6 refused completions, 9 verification runs, one review and 60 minutes.
An optional `goal_budget.work` counts generated-token equivalents (generated
tokens plus one eighth of uncached prompt tokens read); checks/tools are not
charged. It is checked at generation/executor boundaries, not an exact
per-token cut-off. These defaults are policies, not measured optima.

The MLX free-buffer cache defaults to **2 GiB**, configurable with
`PWR_MLX_CACHE_GB`, and is cleared after a completed generation. This is not
a cap on weights, KV cache or total RAM. Active/cache memory is recorded for
diagnosis; the long-run effect of this change remains unmeasured.

## Changes since v0.2.0-alpha

- Whole-file overwrites use the version the conversation saw; writes use
  an atomic path with a final version check.
- Sandboxed commands protect acceptance paths and installed dependencies;
  frozen acceptance artifacts and structured outcomes improve reporting.
- Desktop, console and Goal mode share the session executor; cancellation
  reaches preparation, tools, permissions and checks.
- Family adapters and calibration fix compatibility failures. Sampling keeps
  its source and avoids assuming a model-card mode that is not known.
- Engine pins are now MLX 0.32.3, mlx-lm 0.31.3, mlx-embeddings 0.1.0 and
  mlx-vlm 0.7.2. Managed installs with obsolete pins offer reinstallation.
- Nested check discovery, zero-test detection and command diagnostics address
  previously missing or misleading verification evidence.
- Terminal diagnostics, page checks, browser steps and inline screenshots
  expose evidence from the program that actually runs.
- Focus tools use a resizable grid and terminal tabs; the conversation has
  simpler action descriptions, folding, timestamps and stable scrolling.

See the [changelog](../CHANGELOG.md) for the unreleased record and
[release v0.2.0-alpha](release/v0.2.0-alpha-release-notes.md) for what shipped.

## Experimental Goal support

The verified-proposals phase asks for one file at a time, applies the ordinary
edit policy, verifies it and restores rejected/non-improving proposals. It is
enabled by profile only for **Ornith 1.5 9B** (`goal_proposals: 5`), unless the
workspace overrides it. Recorded product-path runs improved one repair task
for that model but did not meet a no-regression criterion across models.
It is not a default improvement for all models.

`goal_aids.pointers`, `core_tools`, `block_edits`, `plan` and `paced_reasoning`
are implemented switches, off by default, with unmeasured task benefits.
The repeated-repair notice quotes failing lines after three file-change /
failed-command cycles; it is advisory and its model effect remains unmeasured.
Details and conditions are in [the implementation plan](plan/implementation-plan.md)
and [experiment log](experiment-log.md).

## Choosing and evaluating PWR

PWR is intended for developers who want local open-weight inference and an
integrated Mac workspace with explicit edit controls and verification evidence.
Its useful distinction is that combination. Local models, tools and permissions
are not exclusive to PWR, and comparative superiority is not established.

Windows support, a persistent desktop GGUF backend, a stable general extension
interface and general computer use are planned directions. Same-model capability,
latency and memory comparisons require controlled runs on the product path.
See [evaluation](evaluation.md), [known risks](risks.md),
[security](../SECURITY.md) and [next-release readiness](release/next-release-readiness.md).
