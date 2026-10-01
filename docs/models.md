# Models

**Checked against `develop`, 2026-10-01; sampling correctness parent `84b4654c`.** How PWR finds, rates,
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
into canonical actions. An adapter has a revision that a calibration records.

| Family (revision) | Calls it reads |
|---|---|
| Qwen, Nemotron 3.x (`qwen-v3`) | `<tool_call>` JSON; the XML `<function=…><parameter=…>` form with or without its opening or closing `<tool_call>` tag (Qwen3-Coder arrives without the opening one); a list or dict written the Python way inside a parameter (`['-m', 'unittest']`); fenced JSON; `<tools>` written as a call; inline `<think>` |
| GLM-4.x | `<tool_call>name<arg_key>…<arg_value>…` |
| Seed-OSS | `<seed:tool_call>`, `<seed:think>` |
| gpt-oss (Harmony) | channels and `to=functions.…` |
| Gemma 4 (`gemma4-v2`) | `<|tool_call>call:name{…}<tool_call|>` and its thought channel |
| Granite (`granite-v2`) | a bare JSON array, and `<tool_call>{json}</tool_call>`; role-header reasoning |
| Mistral, Devstral, Magistral, Ministral (`mistral-v1`) | `[TOOL_CALLS]name[ARGS]{json}` (several in a row) and the older `[TOOL_CALLS][{…}]`; the request is rendered with the roles Mistral's template insists on (below) |
| Liquid LFM2 (`liquid-v1`) | `<|tool_call_start|>[name(key="v", …)]<|tool_call_end|>` (Python call syntax) and `<function_call>{json}</function_call>`; `<think>` |

A call a model wrote whole but could not finish (cut off inside the arguments)
is never guessed at: it is reported as cut off, and the turn retries smaller.
Fixes are made to help every model where possible, not per model.

**What the request carries for the chat template** (`template_messages`): every
tool call has an id (its own, else nine letters or digits, which Mistral's
template insists on) and every tool result the call's id and the tool's name
(Gemma 4's template names a result from the id; without it the template failed
with `TypeError`); a `tool` message that answers no call is sent as a note from
PWR in a user message; for the Mistral family runs of user messages become one
message and a note after a tool result rides on the result, because the
template counts only user messages and call-free assistant messages and
requires them to alternate. Other families' prompts are unchanged.

Profiles for known models (`strategies/models.json`) carry vendor sampling
and a prompt suffix; a profile names an exact artifact or deployment, never a
bare family or tag. Adding a profile requires updating the counts in
`crates/pwr-domain/tests/declared_profiles.rs`.

## Known issues of a family

Said beside the model in the compatibility panel, whatever its status (a model that passes Quick Calibration is not thereby free of them): **Gemma 4** (12B, 26B and 31B) can fall into a loop that writes "thought" or its channel markers over and over on long prompts with many tools — reported upstream at full precision as well, so it is the weights and not the quantization, and no sampling setting removes it ([google-deepmind/gemma#622](https://github.com/google-deepmind/gemma/issues/622), [#727](https://github.com/google-deepmind/gemma/issues/727), [the 12B discussion](https://huggingface.co/google/gemma-4-12B-it/discussions/41): about 44–60 % of trials on agent prompts of 8,000–23,000 tokens). Measured in the app on 2026-10-01 (12B, 26B, 31B, prompts of 11,000–22,000 tokens): replies of `<|channel>thought` repeated, or of one sentence repeated, stopped by the engine's repetition guard. PWR retries (with a presence penalty after the first), compacts above 20,000 tokens and says so, but for a long agent task another model is more dependable.

## Sampling

Resolved once for the UI, chat and evaluations (`enrich_mlx_sampling`), in this
order, each value remembering where it came from (the Sampling dialog shows it):

1. **the person's own values** (`.pwr-user-sampling.json` in the model folder);
2. **a declared profile** for the exact artifact (`strategies/models.json`);
3. **the model card**, read from the Hub at the downloaded revision and pinned
   (`.pwr-card-sampling.json`): an unambiguous, mode-neutral recommendation
   inside an explicitly named sampling scope, else the original model's
   `generation_config.json`; a "none found" is looked for again after a day;
4. **the artifact's `generation_config.json`**;
5. **PWR's floor: temperature 0.6, top_p 0.95, top_k 20**, when no temperature is
   declared anywhere (never greedy by default, because greedy decoding is what
   Qwen's cards warn leads to endless repetition). An explicit temperature 0
   stays greedy, and a vendor's lone temperature is not topped up with
   truncation it did not ask for.

**IMPLEMENTED A05 correction:** benchmark/evaluation/reproduction descendants,
unscoped coding prose, fenced and indented code, and HTML code blocks are
excluded. Complete alternatives are compared rather than blended. General-use
precedence on neutral cards is retained. Cache schema 4 includes repository,
artifact revision and requested mode, rejects old assumed-thinking choices,
and retains the pinned source revision and source-file URL.

The parser/cache can resolve an explicitly fixed thinking or non-thinking
switch. **Product enrichment uses unknown mode:** UI/chat/eval resolve sampling
before their reasoning planner, which can later disable thinking or remove its
switch. A profile's initial `think=true`, template capability or vendor default
therefore does not justify selecting a thinking recipe here. Mode-specific
per-generation selection remains **PLANNED W5.6** after final reasoning planning;
only neutral card values are currently applied. Artifact/profile/user values
and the existing floor retain their precedence and values. This fixes source
selection, not task capability; no model measurement supports an improvement.

`presence_penalty` and `repetition_penalty` are passed to mlx-lm, whose
penalties look at the **last 20 tokens only** (read from its source,
2026-10-01); a vendor's presence penalty (Qwen's 1.5) is defined over everything
generated so far. After a reply of a turn has looped, PWR asks for a presence
penalty of 1.0 over 1,024 tokens ([agent-loop.md](agent-loop.md#replies-that-fall-apart)); a
card-declared penalty still runs on the 20-token window, and whether to widen it
for the models that declare one (Ornith, Qwen3.5) is an unmeasured decision.

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
suite `quick-calibration-6`, `crates/pwr-models/src/profile.rs`).

| Check | Scored by | Agent-critical |
|---|---|---|
| `termination` | the reply ended by itself | yes |
| `instruction_following` | exactly `READY` | no |
| `structured_output` | JSON with exactly the two requested fields | no |
| `code_understanding` | exactly `42` | no |
| `repository_file_selection` | exactly `src/parser.rs` from a three-file fixture | no |
| `tool_selection` | exactly one call, to a tool that was offered | yes |
| `tool_arguments` | the arguments validate against that tool's schema | yes |
| `tool_choice` | the call is `read_file` on `src/parser.rs` (a model that lists the directory first is not penalised) | no |
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

A calibration records: model reference, the family adapter's revision (`qwen-v3`), Hub revision (`.pwr-revision`), the
backend's artifact digest, a fingerprint of the weight files' names and sizes,
size, quantization, format, architecture, tokenizer and chat-template
fingerprints, backend and its version (`mlx-lm <v>; mlx <v>; sidecar <hash>`),
PWR version, calibration suite version, a coarse hardware class
(`macos-arm64-apple-m2-64gb`), and the time. Unknown facts are recorded as
unknown.

`pwr_models::profile::compare` decides whether it still applies:

| What differs | Outcome |
|---|---|
| backend, format, artifact digest, weight files, size, quantization, revision, tokenizer, chat template, architecture, calibration suite, the adapter's revision (when both are recorded) | **stale** — not applied; Provisional |
| a **failure** (Limited/Incompatible) recorded under no adapter or another one than today's | **stale** (from 2026-10-01): the adapter that read the replies may have been the cause |
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
`crates/pwr-orchestrator/src/converse.rs`). It is never written to the
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

- Local calibrations on the maintainer's M2 Max, 64 GB. **2026-10-01: all 22
  models installed there pass the critical checks of Quick Calibration**
  (`quick-calibration-6`), after the adapter, template and probe fixes of the
  day; before them seven were Limited for PWR's reasons (Qwen3-Coder, Devstral,
  LFM2 24B and 2.5 8B, Granite 4.1, Gemma 4 12B/26B/e4b). This is compatibility,
  not capability.
- Small models building from scratch (`corpus/small-apps-v1.json`, four tasks,
  one trial, 2026-09-29/30): gpt-oss-20b 4/4, Qwen3-14B 3/4, Ornith-1.5-9B 3/4,
  Qwen2.5-Coder-14B 1/4 — development runs, not a comparison.
- On the app's path with hidden tests (2026-10-01, one trial per task): see
  [evaluation.md](evaluation.md#what-has-been-measured).
- Nemotron-3-Nano-4B and Kimi-Linear-48B were refused: they need code from
  their repositories to load.

None of this is evidence that PWR improves a model; see
[evaluation.md](evaluation.md).
