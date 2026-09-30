# Models

**Checked against `develop` at `0776ff4f`, 2026-09-30.** How PWR finds, rates,
downloads and adapts to models, and what its model evidence does and does not
show. Code: `crates/pwr-models` (catalogue, fit, downloads, profiles,
calibration), `crates/pwr-compat` (family conventions),
`crates/pwr-domain/src/reasoning.rs` (Reasoning Effort).

**What "model-aware" means here.** PWR adapts to a model's *conventions* — its
chat template, tool-call format, reasoning markers, sampling defaults — and to
the machine's memory. That is compatibility, and it is implemented. It does
**not** choose a better agentic strategy per model; nothing measured shows
that it could yet (review §6).

## Families and adapters

`pwr-compat` normalises each family's way of writing tool calls and reasoning
(Qwen's `<tool_call>` blocks, Harmony channels for gpt-oss, Gemma 4's
`<|tool_call>` and thought channel, fenced calls, unterminated calls, invented
tool names or argument spellings with one reading) into canonical actions.
An adapter has a revision (`gemma4-v2`) that is part of a calibration's
provenance. Fixes are made to help every model where possible, not per model.

Profiles for known models (`strategies/models.json`) carry vendor sampling
and a prompt suffix; a profile names an exact artifact or deployment, never a
bare family or tag. Adding a profile requires updating the counts in
`crates/pwr-domain/tests/declared_profiles.rs`.

## Status of a model

| Status | Meaning | What the person can do |
|---|---|---|
| **Verified** | PWR's own controlled evaluation of this exact artifact ships with PWR (`crates/pwr-models/verified-models.json`) | Everything |
| **Locally calibrated** | Quick Calibration ran on this machine and every agent-critical check passed | Everything; confidence *preliminary* |
| **Provisional** | Nothing measured; where every new model starts | Everything, with conservative defaults |
| **Limited** | Calibration found an agent-critical check failing | Chat without a workspace; agent tasks refused with the reason |
| **Incompatible** | Unreadable folder, no chat template (MLX), or no calibration reply at all | Nothing |

Confidence beside it: *established*, *preliminary*, *reduced* (evidence from a
setup that differs in a way that may matter), *untested*.

**No model is Verified.** The registry is empty on purpose: no evaluation on
record has the artifact provenance an entry requires. Plan W10.1 audits
whether the Verified/certification surface earns its place before W8 produces
evidence of that kind.

## Quick Calibration

A bounded, deterministic check that PWR can *operate* a model — a compatibility
smoke test, not a capability measurement (`crates/pwr-models/src/calibration.rs`;
suite `quick-calibration-5`, `profile.rs:33`).

| Check | Scored by | Agent-critical |
|---|---|---|
| `termination` | the reply ended by itself | yes |
| `instruction_following` | exactly `READY` | no |
| `structured_output` | JSON with exactly the two requested fields | no |
| `code_understanding` | exactly `42` | no |
| `repository_file_selection` | exactly `src/parser.rs` from a three-file fixture | no |
| `tool_selection` | exactly one call, to `read_file` | yes |
| `tool_arguments` | arguments validate against the schema and name the file | yes |
| `tool_result_continuation` | uses a given tool result (`1337`) | yes |
| `answer_after_reasoning` | with thinking allowed, answers `51` | no |
| reasoning budget | with a 16-token budget, the engine closes the phase and an answer follows | no (sets the reasoning profile) |

At most nine requests, each capped at 512 answer tokens and 120 s,
temperature 0, fixed seed, no files read or written, cancellable. Locally
calibrated when every agent-critical check passes; Limited otherwise;
Incompatible when no request produced a reply.

**It does not show** that a model codes well, works at long context, follows
instructions over a long conversation, or behaves the same under another
quantization, backend, template or engine version.

## Provenance and reuse

A calibration records: model reference, Hub revision (`.pwr-revision`), the
backend's artifact digest, a fingerprint of the weight files' names and sizes,
size, quantization, format, architecture, tokenizer and chat-template
fingerprints, backend and its version (`mlx-lm <v>; mlx <v>; sidecar <hash>`),
PWR version, calibration suite version, a coarse hardware class
(`macos-arm64-apple-m2-64gb`), and the time. Unknown facts are recorded as
unknown.

`pwr_models::profile::compare` decides whether it still applies:

| What differs | Outcome |
|---|---|
| backend, format, artifact digest, weight files, size, quantization, revision, tokenizer, chat template, architecture, calibration suite | **stale** — not applied; Provisional |
| backend major version (for 0.x, the minor) | **stale** |
| backend minor/patch, the sidecar's hash, hardware class, a fact known on one side only | **reduced** — applied with lower confidence |
| PWR version, path, time | applies |

Local calibrations live in `~/.pwr/model-evidence/<hash>.json` (or
`$PWR_EVIDENCE_DIR`), never in a workspace.

## Reasoning Effort

**Low / Medium (default) / High**, per workspace (`reasoning_effort` in
`.pwr/chat-config.json`). It sets the budget of a model's explicit thinking
phase in one generation; it does not change how many actions the agent takes.

| Capability (read from the template, refined by calibration) | What the levels do |
|---|---|
| `native_budget` (template reads `thinking_budget`, e.g. Seed-OSS) | the template is told the budget in 512-token steps and the engine enforces it exactly |
| `explicit_thinking_stream` (`<think>…</think>`, `<seed:think>`) | the engine counts thinking tokens and closes the block at the budget |
| `template_controlled` (template reads `reasoning_effort`, gpt-oss) | the model's own low/medium/high; no token budget |
| `observable_only` (llama.cpp) | no effect |
| `none`, `unknown` | no effect |

| Budgets | Low | Medium | High |
|---|---|---|---|
| declared by an exact-artifact profile | from the profile | | |
| calibrated (a forced close worked) | 2,048 | 6,144 | 12,288 |
| conservative (Provisional) | 1,024 | 4,096 | 8,192 |

Before each generation the budget is clamped so the answer keeps a reserve
(`pwr_domain::plan_reasoning`): room = window − prompt − a safety margin;
the answer keeps min(4,096, room); reasoning gets what is left of the
request, or none under 256 tokens. At the budget the engine closes the block
once and lets the answer follow; a model that reopens it or stops without an
answer ends `reasoning_unfinished`, and the turn retries once with thinking
off.

**Correction to the previous document:** reasoning *is* carried in the
history — each assistant step keeps up to 16,000 characters of its reasoning,
the end of it, until compaction folds it (`kept_reasoning`,
`crates/pwr-orchestrator/src/converse.rs:2519`). It is never written to the
event log: a test asserts no event payload contains it.

`pwr eval run --reasoning-effort` bounds a campaign's reasoning the way the
app does, at Medium unless told otherwise; `--reasoning-effort off` lets the
model reason as its template does, which is what every campaign before
2026-09-30 measured.

## The Model Manager

Opened from the model chip. Source: the Hugging Face Hub's JSON API only
(`/api/models`, `/api/models/{repo}`, the tree at a revision, the resolved
`config.json`); `PWR_HF_BASE_URL` points elsewhere; `HF_TOKEN` is sent for
gated repositories. On a Mac the app shows MLX models only.

**Cards** show what the Hub reports (name, author, base model, parameters,
architecture, context, licence, downloads, likes), each with its source;
missing values read *unknown*. **Filters** (applied in the core): fits this
machine, parameters, context, size, quantization, family. Discover loads 20
repositories per page (**Load more models** follows the Hub's cursor).
Repositories whose config needs repository code (`auto_map`) are marked
incompatible: PWR never runs code from a model repository.

### Fit

`pwr_models::fit::estimate` rates a variant for this machine with the window
arithmetic ([inference.md](inference.md#the-working-window)): weights take
their file size; the engine about 1 GiB; the host keeps a quarter of memory,
at least 8 GiB; prefill holds a second copy of the cache (MLX) or four times
it (llama.cpp).

| Rating | Meaning |
|---|---|
| Recommended | ≥ 32k tokens of context fit and the weights take ≤ 60 % of what is left after the reserve |
| Should fit | ≥ 16k fits |
| Tight fit | 8k–16k fits |
| Not recommended | the weights do not fit, or < 8k of context would |
| Incompatible | no engine here runs the format, or it needs repository code |
| Unknown | memory or size unreadable |

The rating is about loading with a useful context — not speed, quantization
quality or MoE sparsity.

### Downloads

Into `~/.pwr/models/<owner>/<name>/` (or `PWR_MLX_MODELS`). The client names
repository, commit, variant and format; the core re-reads the file list, sizes
and checksums at that commit and then: refuses any file without a size and a
checksum (LFS SHA-256, or the Git blob SHA-1 for small files); keeps paths
inside the model folder; requires the space left to download plus 5 GiB;
keeps a matching existing file and never overwrites a different one; writes
`<file>.part`, verifies, renames; resumes from `.part`. Progress, verification,
completion, failure (disk, conflict, network, verification) and cancellation
arrive as `_pwr/download_progress`. `pwr models download` accepts only
artifacts listed in `strategies/artifacts.json`.

A gated repository currently fails as a generic download error (plan W7.7).

**On this Mac** lists every model in the engines' folders with its size;
**Delete** asks, then removes the files — refused for the model the workspace
uses, anything outside the models folder (symlinks included), a folder with no
`config.json`, and a folder containing another model.

## Model evidence on record

- Local calibrations on the maintainer's M2 Max, 64 GB: `gemma-4-31B-it-MLX-6bit`
  and `gpt-oss-20b-MXFP4-Q8` became Locally calibrated after adapter fixes of
  2026-09-29 ([release/v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md)).
- Small models building from scratch (`corpus/small-apps-v1.json`, four tasks,
  one trial, 2026-09-29/30): gpt-oss-20b 4/4, Qwen3-14B 3/4, Ornith-1.5-9B 3/4,
  Qwen2.5-Coder-14B 1/4 — development runs, not a comparison.
- Nemotron-3-Nano-4B and Kimi-Linear-48B were refused: they need code from
  their repositories to load.

None of this is evidence that PWR improves a model; see
[evaluation.md](evaluation.md).
