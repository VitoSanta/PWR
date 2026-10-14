# Models

**Rewritten 2026-10-14 for the Gemma 4 line (`0a3c5a64`).** PWR runs Google's
Gemma 4 and nothing else: one pack per memory tier, each with its trained
262,144-token context. What is fixed about the packs is written once, in
`crates/pwr-domain/src/gemma.rs`; nothing about a model is discovered,
calibrated or read from a model card at run time. Code: `pwr_domain::gemma`
(the line), `crates/pwr-compat` (Gemma's reply protocol), `crates/pwr-models`
(formats, verified downloads, the models folder).

Removed on 2026-10-14 with the idea of running any downloaded model: Quick
Calibration, the compatibility assessment and its statuses, certification,
Hub search and fit ratings, sampling read from model cards, model profiles at
run time, per-model sampling settings, and graded Reasoning Effort. Their
history is in [experiment-log.md](experiment-log.md) and the benchmark's
`STORICO-EVIDENZE.md`.

## The line

| Tier | Pack | Weights | Notes |
|---|---|---|---|
| 16 GB | `mlx-community/gemma-4-12B-it-4bit` | 6.7 GB | Gemma 4 12B "unified" (vision without a separate encoder) |
| 24–32 GB | `mlx-community/gemma-4-26b-a4b-it-4bit` | 15.3 GB | MoE, 3.8B active; QAT 4-bit (15.6 GB) and 6-bit (21.6 GB) packs also listed |
| 64 GB+ | `mlx-community/gemma-4-31b-it-8bit` | 33.8 GB | dense |

Each pack has a small "assistant" model Google publishes for speculative
decoding; it is kept for that and is never offered as a model to run.
The experimental engine integration finds the matching assistant only after
its weights are fully downloaded. It is **off by default**: the 2026-10-14
short comparisons failed the exact-text and/or 1.4× speed gates. Explicit
`PWR_MLX_ASSISTANT=on` enables research runs; penalties and 8-bit KV use normal
decoding. No pack has passed the full assistant gate yet.

**Context and memory.** The window is the full 262,144 tokens unless the
person asks for less or this Mac's memory holds less for the pack
(`gemma::window`). The account (`gemma::memory_for`): weights, the global
layers' cache (16, 20 and 80 KB a token at 16 bits for 12B, 26B and 31B: only
8, 5 and 10 of their layers attend over the whole context), the sliding layers
(1,024 tokens each), 1 GiB for a prefill step's attention scores, an engine
allowance, and a 6 GiB reserve on every tier. By that account the 12B does
not hold 262k on 16 GB nor the 26B on 24 GB at 16 bits; an 8-bit cache for
the global layers (`PWR_MLX_KV_BITS=8`) is the lever, its quality to be
measured. The measured figures replace the estimate where they differ
(benchmark `audits/linea-gemma4/prove-finali`).
At 16-bit KV the estimate also conservatively reserves the matching bf16
assistant weights (0.84–0.94 GB), even while the experimental feature is off.
The 8-bit path does not load or reserve an assistant. These are estimates,
not certification of a memory tier.

## Sampling

Google's for Gemma 4, for all use cases: temperature 1.0, top_p 0.95, top_k 64
(`gemma::sampling`), sent with every request.

## Thinking

Gemma 4 has thinking on or off (`<|think|>` in the system turn), nothing in
between. The app has one switch, **Thinking**, off by default; with it on, the
engine closes a thought at 4,096 tokens (`gemma::THOUGHT_CAP`), a safety cap
against the thought that never ends (google-deepmind/gemma#727), not a
quality setting.

## Families and adapters

PWR runs the Gemma 4 family only (2026-10-14). `pwr-compat` reads Gemma 4's
protocol into canonical actions: calls written as
`<|tool_call>call:name{key:<|"|>value<|"|>,…}<tool_call|>` (several in one
reply; an argument written as an ordinary quoted string is read too) and the
thought channel `<|channel>thought … <channel|>` (`gemma4-v3`). A deployment
that is not recognisably Gemma 4 -- llama.cpp's server, which parses calls
itself, or a scripted test model -- gets the native adapter, which changes
nothing. The adapters for Qwen, GLM, Seed-OSS, gpt-oss, Granite, Mistral and
Liquid were removed with the families.

A call a model wrote whole but could not finish (cut off inside the arguments)
is never guessed at: it is reported as cut off, and the turn retries smaller.
A call that could not be read is answered with Gemma's own form,
`<|tool_call>call:NAME{KEY:<|"|>VALUE<|"|>}<tool_call|>`.

**What the request carries for the chat template** (`template_messages`): every
tool call has an id and every tool result the call's id and the tool's name
(Gemma 4's template names a result from the id; without it the template failed
with `TypeError`); a `tool` message that answers no call is sent as a note from
PWR in a user message.

Profiles for known models (`strategies/models.json`) carry vendor sampling
and a prompt suffix; a profile names an exact artifact or deployment, never a
bare family or tag. Adding a profile requires updating the counts in
`crates/pwr-domain/tests/declared_profiles.rs`.

## Known issues of a family

Said beside the model in the compatibility panel, whatever its status (a model that passes Quick Calibration is not thereby free of them): **Gemma 4** (12B, 26B and 31B) can fall into a loop that writes "thought" or its channel markers over and over on long prompts with many tools — reported upstream at full precision as well, so it is the weights and not the quantization, and no sampling setting removes it ([google-deepmind/gemma#622](https://github.com/google-deepmind/gemma/issues/622), [#727](https://github.com/google-deepmind/gemma/issues/727), [the 12B discussion](https://huggingface.co/google/gemma-4-12B-it/discussions/41): about 44–60 % of trials on agent prompts of 8,000–23,000 tokens). Measured in the app on 2026-10-01 (12B, 26B, 31B, prompts of 11,000–22,000 tokens): replies of `<|channel>thought` repeated, or of one sentence repeated, stopped by the engine's repetition guard. PWR retries (with a presence penalty after the first), compacts above 20,000 tokens and says so. Since 2026-10-14 PWR runs Gemma 4 only; the core changes made against Google's own description of the family, and their measurement, are in [inference.md](inference.md).


## The Model Manager

Opened from the model chip. It lists the line: each pack with its tier, its
weights, whether it is on this Mac, and the context this Mac holds for it
(`_pwr/local_models`). **Download** fetches the pack from the registry,
**Use** selects it for the workspace, **Delete** removes it. There is no
search: other models are not offered.

## Downloads

Into `~/.pwr/models/<owner>/<name>/` (or `PWR_MLX_MODELS`), and only packs
of the line: the client names a registry entry, and the core takes the
repository, commit, files, sizes and checksums from `strategies/artifacts.json`,
then: refuses any file without a size and a
checksum (LFS SHA-256, or the Git blob SHA-1 for small files); keeps paths
inside the model folder; requires the space left to download plus 5 GiB;
keeps a matching existing file and never overwrites a different one; writes
`<file>.part`, verifies, renames; resumes from `.part`. Progress, verification,
completion, failure (disk, conflict, network, verification) and cancellation
arrive as `_pwr/download_progress`. `pwr models download` accepts only
artifacts listed in `strategies/artifacts.json`: since 2026-10-14 the five
Gemma 4 MLX packs, each pinned to its revision with every file's size and
SHA-256 (computed from PWR's verified download and matched against the Hub's
LFS hashes). The LM Studio and Qwen/Nemotron GGUF entries were removed;
Gemma GGUF entries come with the Windows engine.

A gated repository currently fails as a generic download error (plan W7.7).

**Delete** asks, then removes the files — refused for the model the workspace
uses, anything outside the models folder (symlinks included), a folder with no
`config.json`, and a folder containing another model.

