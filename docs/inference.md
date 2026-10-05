# Inference

**Checked against `develop` at `bff93062`, 2026-10-01.** How PWR runs models:
the provider boundary, PWR's own MLX engine, the experimental llama.cpp path,
the working window, what the engine's libraries are known to get wrong, and
where it is slow or fragile.

## The boundary

`crates/pwr-provider` defines two traits. `ModelProvider` generates: `chat`
returns a stream of reasoning and answer deltas, tool calls and metrics, and
can be cancelled. `InferenceBackend` manages: discovery, load/unload,
residency, inspection, runtime state, capabilities, version. `pwr-runtime`
picks the backend (`--backend mlx|llama`, `PWR_BACKEND`) and produces an
immutable deployment descriptor; it holds no task or policy logic.
`pwr-compat` turns each family's tool-call and reasoning conventions into
canonical actions, so the loop never learns a family's format
([models.md](models.md#families-and-adapters)).

Sampling reaches the backend as a `BTreeMap<String, Value>`: an option a
backend does not understand is silently ignored (plan W2.7). How the values are
chosen is in [models.md](models.md#sampling).

A **chunk** of a reply can also say how far the engine has read the prompt
(`prefill: {processed, total}`); the core forwards it as `_pwr/model_progress`
and the app shows it ([pwr-serve.md](pwr-serve.md)).

## The MLX engine

**The most distinctive part of PWR, and the only engine the app uses on a
Mac.** A Python sidecar, `crates/pwr-mlx/sidecar/pwr_mlx.py` (about 1,080
lines), driven by `crates/pwr-mlx` over JSON lines on stdin/stdout — no network
endpoint at all. Libraries pinned: `mlx 0.32.3` (0.32.0 until 2026-10-01), `mlx-lm 0.31.3`,
`mlx-embeddings 0.1.0`, `mlx-vlm 0.7.2` (0.6.17 until 2026-10-05; 0.7.2 loads 1-bit weights), in a private environment the app
installs (`apps/desktop/src-tauri/src/engine.rs`; [distribution.md](distribution.md)).

Operations: `load` (weights, with optional RoPE scaling; the vision encoder
through `mlx-vlm` when the model has one), `chat`, `attention` (whether the
model's attention is fused, and the score buffer it would need), `cancel`,
`shutdown`. The protocol is documented at the top of the sidecar.

What it owns:

- **Template and tokenizer.** It renders the model's own chat template
  (through transformers' sandboxed Jinja, `trust_remote_code` off, Hub
  offline), so PWR controls reasoning switches the template exposes. The
  messages it is given are shaped for that template by the core
  ([models.md](models.md#families-and-adapters): call ids, result names, roles).
- **Reasoning.** Tracks thinking delimiters, enforces a budget by closing the
  block once, passes `thinking_budget` or `reasoning_effort` to templates that
  read them, and ends `reasoning_unfinished` when a model reopens its thinking
  or stops without answering. Details in [models.md](models.md#reasoning-effort).
- **Sampling.** `make_sampler` (temperature, top_p, top_k, min_p) and
  `make_logits_processors` (presence and repetition penalty, with the
  `presence_context_size` the caller may widen; the library's own window is 20
  tokens).
- **Prompt cache.** Reuses the KV cache across steps and messages when the new
  prompt extends the old one; a trimmable cache is cut back to the prompt after
  generation, a non-trimmable one (Qwen 3.5/3.6's hybrid layers) keeps a
  checkpoint copy. Background `aside` requests (summaries, reviews) do not
  evict the conversation's cache. The cache is **per model**: a model switch is
  a full cold prefill of the conversation.
- **Prefill** in chunks bounded **in time**: the first is 512 tokens, then as
  many (at most 8,192, at least 256) as take about 3 s at the speed just
  measured, so a Stop is seen within seconds (it waited 33 s on Gemma 4 12B and
  minutes on a 31B behind one 8,192-token chunk; measured 2026-10-01: 32.7 s →
  2.3 s). The chunk also shrinks with the context where attention materialises
  its scores (a fixed 8,192-token step once asked for 41.9 GB at 160k tokens). It reports `prefill` events as it goes, and **observes a
  cancel between chunks** (plan W5.1). Measured 2026-10-01 on Gemma 4 12B: a
  4,976-token prompt took about 26 s with 8,192-token steps and 25 s with 1,024:
  prefill is compute-bound on this machine (about 200 tokens/s for a 12B dense
  model), not a matter of step size.
- **Stops**: `stop`, `length`, `repetition` (a loop detector over the last
  output), `cancelled`, `reasoning_unfinished`; a model's closing marker for a
  tool call ends generation (Gemma 4).
- **Metrics**: prompt, cached, generated and reasoning tokens by the model's
  tokenizer; prefill and generation times; peak memory.

Library diagnostics go to stderr so they cannot corrupt the protocol on stdout
(a Gemma 4 failure of 2026-09-29).

### One engine, one generation at a time

The Rust side serialises generations: the lock travels with the stream, so a
slow generation blocks every other request, including a person's next
message behind a background summary or a goal's review (plan W5.5). If an
abandoned reply does not finish draining within 300 s the engine is restarted,
which costs a model load and a cold prefill (3–4 minutes at 40,000 tokens).

**One engine at a time on the machine.** The GPU's working set on the
maintainer's M2 Max is 55.7 GB of 64 GB (`mx.device_info()`,
`max_recommended_working_set_size`; largest single buffer 41.7 GB). Three
models loaded together (about 45 GB of weights plus caches) made Gemma 4 26B and
Devstral emit text with no meaning from their first reply, with no error;
two (about 30 GB) did not. Within the app only one sidecar runs.

### What the engine's libraries are known to get wrong

Read, not tested; none is proven to be the cause of anything PWR saw.

| Finding | Source | Consequence here |
|---|---|---|
| A **silent KV-cache corruption on natural-language prompts of about 60k tokens and more**: generation collapses into token id 0 (`!`), probabilistically, dense and MoE alike, on `mlx 0.32.0` / `mlx-lm 0.31.3` / `mlx-vlm 0.6.3` (the report predates the bump) | [jundot/omlx#3777](https://github.com/jundot/omlx/issues/3777) | A default compaction ceiling of 32,768 tokens guarded against it until 2026-10-02; the owner withdrew it ([D-2026-10-02-3](decisions.md)), so conversations now reach 75 % of the granted window and a workspace that sees this collapse sets `compact_ceiling_tokens`. PWR saw a similar collapse at 27k tokens of failing-and-retrying history (Qwen3-Coder-30B), which a synthetic 40-turn chain to 32k and a fresh 30k prompt did not reproduce |
| mlx-lm's **presence and repetition penalties look at the last 20 tokens** (`*_context_size=20`) | `mlx_lm/sample_utils.py` | A vendor's presence penalty is not what the engine applies; the anti-loop path widens it |
| mlx-lm sets the **wired limit only around `stream_generate`** (`wired_limit`), not around PWR's own chunked prefill; without wiring the first GPU command after an idle gap stalls about 0.9 s | `mlx_lm/generate.py`; [jundot/omlx#4040](https://github.com/jundot/omlx/issues/4040) | A fraction of a second per turn; not acted on |
| **Quantizing the KV cache** (`kv_bits`) has reported silent corruption of the prefilled context in `mlx-vlm` 0.6.1, and `quantized_kv_start` defaults differ between `generate_step` (0) and the CLI (5000) | [Blaizzy/mlx-vlm#1310](https://github.com/Blaizzy/mlx-vlm/issues/1310), [ml-explore/mlx-lm#1651](https://github.com/ml-explore/mlx-lm/issues/1651) | PWR does not use it |
| Prefix reuse is **disabled for hybrid models** (sliding window, SSM/linear attention) in several servers; mlx-lm 0.31.2 added caching for non-trimmable caches | [ml-explore/mlx-lm#980](https://github.com/ml-explore/mlx-lm/issues/980) | PWR keeps a checkpoint copy for them |
| `mlx` **0.32.1–0.32.3** (to 2026-09-29) fix a GQA decode kernel's batch offset, state corruption when a primitive throws during eval, and a quantized-matmul corruption when the quantized dimension is not a multiple of 32; `mlx-lm` 0.32.0 is out (2026-10-01) | PyPI, the MLX release notes | `mlx` **0.32.3 adopted on 2026-10-01** after the sidecar's tests, a Gemma 4 prefill comparison and a calibration of four architectures; `mlx-lm` 0.32.0 not adopted ([distribution.md](distribution.md)). Whether it removes the long-conversation collapse is not known |
| mlx-lm's own `qwen3_coder` tool parser **falls back to `ast.literal_eval`** for malformed JSON, and fixes Mistral and Gemma 4 parsers | mlx-lm 0.31.2, 0.31.3 release notes | The same readings exist in `pwr-compat` |

### Where it is fragile

| Defect | Evidence | Plan |
|---|---|---|
| **The window is not checked against the real request.** `prepare_context` records and returns the window asked for | `crates/pwr-mlx/src/lib.rs` | W5.3 |
| **Switching model in a long conversation re-reads everything**: 32 minutes measured for a 30B at about 160k tokens; the app shows progress but does not warn first | 2026-09-30 manual pass | a warning |
| **Every other request waits behind a generation the person stopped** — a model switch, the context panel, a calibration — until the engine sees the stop; with time-bounded prefill chunks that is seconds, but the app still shows nothing while it waits (measured 2026-10-01: a model switch that took 100 s) | `crates/pwr-mlx` (one sidecar lock) | W5.5 |
| **An abandoned reply that does not drain restarts the engine** (300 s of silence) | `ENGINE_SILENCE` | — |
| **Background summaries** occupy the engine only when the person turned them on; one in flight is pre-empted by a prompt | plan W5.2 (done) | — |
| **The embedding sidecar** has a 30 s deadline and a lexical fallback (plan W1.8, done) | — | — |

## llama.cpp (experimental)

`crates/pwr-llama`: reads GGUF metadata for the window, starts a managed
`llama-server` bound to `127.0.0.1`, and converts its OpenAI-compatible
stream. **Command line only**; a release build on macOS ignores
`PWR_BACKEND` and the app runs MLX.

- A server is started **per generation** (`LlamaProvider::chat`); keeping it
  alive across turns is old backlog R.4 (Later).
- Whenever tools are sent, the request sets **`tool_choice: "required"`**: a
  conversation there cannot answer in prose, which is a different turn
  semantics from MLX (plan W5.4).
- Reasoning Effort has no effect: `llama-server` takes no per-request budget.

## The working window

Computed, not probed (`crates/pwr-runtime/src/window.rs`). `window::decide`
keeps every ceiling and names the one that bound:

- the model's trained length (and its RoPE extension, if the family has one);
- memory: total memory **minus a reserve of a quarter of it, at least 8 GiB**
  (`default_reserve_bytes`), minus the weights and about 1 GiB of engine
  overhead, divided by what one token of cache costs according to the model's
  `config.json`, with a second copy of the cache for prefill;
- a person's own setting, capped by the above.

This is the window *theoretically allocatable* (262,144 tokens for a 35B on
the maintainer's Mac). It is not the window that is stable under load, the one
that is effective for a task, or the one that stays interactive; maximising the
first does not maximise the others (review §12). **A conversation compacts at
75 % of the granted window** (196,608 of 262,144 tokens), unless the workspace
set a threshold or a ceiling ([context.md](context.md#compaction)). The fit rating in the
Model Manager uses the same arithmetic ([models.md](models.md#fit)). The GPU's
own limit (`max_recommended_working_set_size`) is not read: on a 64 GB Mac the
reserve rule is the tighter of the two (48 GB against 55.7 GB).

## Performance: where time goes

Measured on the product path (2026-10-01, one model at a time unless noted):
prefill is compute-bound (about 200 tokens/s for a 12B dense model, several
times that for a 3B-active MoE); a 42,000-token cold prefill took 179–209 s for
a 30B MoE with a second engine running beside it; generation of a thinking model
dominates a turn. No measurement ranks the rest on the product path (plan W8.7).
Known costs visible in the code: wiki and graph rebuilt after turns, a full
conversation snapshot written each turn (W6.3), synchronous file and SQLite work
on async paths.
