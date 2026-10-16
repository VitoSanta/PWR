# Product overview

**Release v0.4.1-alpha, 2026-10-16.** This page describes the behaviour of
that release. The previous release was v0.4.0-alpha (2026-10-15).

The 0.4.1 maintenance update adds independent terminal tabs, resolves terminal
colours in the active theme, and uses an isolated local-only native webview
for desktop preview. Historical durations reconstructed from the request are
labelled “since request”, including queue time. Reopened historical conversations
do not currently restore Review diffs.

## Purpose and intended use

PWR is an open-source local coding agent for Apple-silicon Macs, built around
Google's Gemma 4. It combines a managed MLX runtime, a desktop conversation,
repository tools, inspectable changes and project checks. The model proposes
actions; the harness controls file versions, permissions, execution,
verification and budgets.

The current supported use is supervised work on bounded bugs, features and
refactors in recoverable repositories. Creating projects is implemented, but
results are variable: benchmarks of the Gemma line still show incorrect tool
calls, misread test outcomes and tasks that do not reach a conclusion. The
project does not demonstrate general capability gains over other agents using
the same model and budget.

Inference does not require a cloud API or a PWR account. Downloading models,
installing dependencies and authorised network tools can contact external
services. Local inference is not a promise that every operation is offline.

## Models

PWR runs the Gemma 4 family only. One family means one chat template and one
tool-call format, rendered as Google describes them; other families, and the
Ollama and LM Studio backends, are not supported. The line, one or more packs
per memory tier ([models](models.md)):

| Tier | Packs | Context |
|---|---|---|
| 16 GB | Gemma 4 E2B, E4B, E4B QAT (4-bit) | 131,072 tokens |
| 24 GB and up | Gemma 4 12B (4-bit); 26B-A4B (4-bit, QAT 4-bit, 6-bit) | up to 262,144 tokens |
| 64 GB and up | Gemma 4 31B (8-bit) | up to 262,144 tokens |

The context is the pack's full window unless this Mac's memory holds less, by
an estimate that keeps a 6 GiB reserve; the estimate is not a measurement of
task quality. Packs read images; whether a page check also receives a
screenshot depends on the pack declaring a vision encoder.

## Components

- **Desktop:** Angular and Tauri on Material 3, light and dark. The
  conversation fills the window; every control sits in the message bar,
  which folds into the PWR mark at rest. Tools (review, terminal tabs, files,
  web preview, knowledge, checks, activity) open from a right-click wheel and
  sit beside the conversation as resizable cards.
- **Rust core:** `pwr serve --stdio`, using ACP/JSON-RPC with `_pwr/*`
  extensions. The interface asks the core to perform changes, including Revert.
- **Session executor:** sequences conversation and Goal mode for desktop and
  console. The scripted `pwr run` / `pwr eval run` loop remains separate.
- **MLX sidecar:** a managed Python process with streaming, cancellation,
  Gemma 4's template, images, prompt cache, runtime metrics and optional
  drafting with the pack's assistant.
- **Verification and evidence:** check discovery, baseline comparison,
  frozen acceptance artifacts and a local SQLite event log.

See [architecture](architecture.md), [inference](inference.md) and
[the protocol](pwr-serve.md) for the component boundaries.

## Available features and their limits

| Feature | Current behaviour | Boundary |
|---|---|---|
| Operational conversation | Read, search, edit, run commands, stream answers, steer and Stop | Supervised; model loops and backend failures remain possible |
| Tool set | A core of nine tools every turn, plus memory and web groups, page checks and terminal reading where they apply | Fewer tools by design; the rest of the catalogue is not offered |
| Versioned edits | Hash guards, exact replacements, atomic file writes, diffs and conflict-aware Revert | No global filesystem transaction or arbitrary command rollback |
| Project checks | Explicit checks, CI/scripts/build-system discovery, including nested projects | Passing checks establish only what they exercise |
| Goal mode | Bounded continuation and recovery, technical checks, request review and declared acceptance | A model's completion or review cannot certify acceptance by itself |
| Frozen acceptance | Persisted hashes of selected test/configuration artifacts, checked before and after | Inferred artifacts cannot cover every indirect dependency |
| Permissions | Protected, Standard and Full access; requests identify the action | Full access removes command confinement |
| Model Manager | The Gemma 4 line with sizes and the context this Mac holds; downloads pinned to a revision, verified file by file, resumable | Fit is an estimate, not a task-quality measurement |
| Assistants | Each pack's assistant downloads on its own; a switch turns drafting on | Experimental, off by default: no pack has passed the exact-text and speed gates |
| Thinking | Gemma's thinking as a switch, capped at 4,096 tokens a reply | Off by default; slower, its benefit unmeasured |
| Context continuity | Objective revisions retained, compaction, checkpoints and resume | Some context estimates remain heuristic; restored traces are partial |
| Terminal diagnostics | Read recent terminal output with a dedicated conversation permission | Desktop support required; reading is distinct from executing |
| Local page diagnostics | Rendered text/console; packs with a vision encoder also receive an image | Fresh browser profile, no shared preview login; experimental |
| Page steps | Up to 12 click/type/press steps, option selection and scrolling, then a screenshot | DOM operations and synthetic events; framing/trusted-event limitations |
| Project knowledge | Wiki, outline and project recall; optional semantic retrieval and 3D graph | Projections/model summaries are not authoritative verification |
| Chat without workspace | Read-only attached references and conversation | No project edits or command execution |

### Current defaults

Sampling is Google's for Gemma 4: temperature 1.0, top-p 0.95, top-k 64.
Thinking is off.

Conversation compaction uses **75% of the granted context window, with no
default token ceiling**. A workspace may set `compact_ceiling_tokens`;
collapsed-reply recovery can compact earlier.

Ordinary turns default to 100 actions. Goal defaults are 208 actions,
6 refused completions, 9 verification runs, one review and 60 minutes.
An optional `goal_budget.work` counts generated-token equivalents (generated
tokens plus one eighth of uncached prompt tokens read); checks/tools are not
charged. It is checked at generation/executor boundaries, not an exact
per-token cut-off. These defaults are policies, not measured optima.

The context cache stays at 16 bits; an 8-bit cache for the global layers and
the long-context prefill bounds are engine switches, off unless asked for
([models](models.md)). The MLX free-buffer cache defaults to **2 GiB**,
configurable with `PWR_MLX_CACHE_GB`, and is cleared after a completed
generation. This is not a cap on weights, KV cache or total RAM.

## Changes since v0.3.0-alpha

- PWR runs Gemma 4 only. The registry, profiles and catalogue hold the
  Gemma 4 packs; the template, the empty thought block after a tool result,
  tool results as mappings and image attention follow Google's description.
- One pack per memory tier, with the context this Mac holds for each and a
  6 GiB reserve on every tier.
- Assistants for speculative decoding: downloaded from the Model Manager,
  turned on with a switch, off by default.
- Every turn is offered the core tool set; memory and web groups are added
  where they apply.
- Goal mode bounds turn recovery, stops an exhausted goal, counts empty
  search outcomes toward bounded progress recovery and keeps failing
  reproducer assertions apart from implementation defects.
- The desktop app is redesigned on Material 3, with a new mark and app icon.
- Engine pins are MLX 0.32.3, mlx-lm 0.32.0, mlx-embeddings 0.1.0 and
  mlx-vlm 0.7.8 on Python 3.14.8. Managed installs with obsolete pins offer
  reinstallation.

See the [changelog](../CHANGELOG.md) and the
[v0.4.1-alpha release notes](release/v0.4.1-alpha-release-notes.md).

## Experimental Goal support

The verified-proposals phase asks for one file at a time, applies the ordinary
edit policy, verifies it and restores rejected/non-improving proposals. No
Gemma 4 profile enables it; a workspace can, with `goal_budget.proposals`. Its
earlier measurements were on other model families and do not carry over.

`goal_aids` switches — `test_recipe`, `pointers`, `block_edits`, `plan`,
`paced_reasoning`, `accept_stop`, `confirmation_notes`, `test_call_notes` and
`core_check` — are implemented and off by default, with unmeasured task
benefits on Gemma 4 (`core_tools` is still accepted and now has no effect,
since every turn has the core set). The repeated-repair notice quotes failing
lines after three file-change / failed-command cycles; it is advisory and its
model effect remains unmeasured. Details and conditions are in
[the implementation plan](plan/implementation-plan.md) and
[experiment log](experiment-log.md).

## Choosing and evaluating PWR

PWR is intended for developers who want local open-weight inference and an
integrated Mac workspace with explicit edit controls and verification evidence.
Its useful distinction is that combination, now tuned to one model family.
Local models, tools and permissions are not exclusive to PWR, and comparative
superiority is not established.

Windows support, a stable general extension interface and general computer
use are planned directions. Same-model capability, latency and memory
comparisons require controlled runs on the product path.
See [evaluation](evaluation.md), [known risks](risks.md),
[security](../SECURITY.md) and [next-release readiness](release/next-release-readiness.md).
