# Inference

**Checked against `develop` at `0776ff4f`, 2026-09-30.** How PWR runs models:
the provider boundary, PWR's own MLX engine, the experimental llama.cpp path,
the working window, and where it is slow or fragile.

## The boundary

`crates/pwr-provider` defines two traits. `ModelProvider` generates: `chat`
returns a stream of reasoning and answer deltas, tool calls and metrics, and
can be cancelled. `InferenceBackend` manages: discovery, load/unload,
residency, inspection, runtime state, capabilities, version. `pwr-runtime`
picks the backend (`--backend mlx|llama`, `PWR_BACKEND`) and produces an
immutable deployment descriptor; it holds no task or policy logic.
`pwr-compat` turns each family's tool-call and reasoning conventions into
canonical actions, so the loop never learns a family's format.

Sampling reaches the backend as a `BTreeMap<String, Value>`: an option a
backend does not understand is silently ignored (plan W2.7).

## The MLX engine

**The most distinctive part of PWR, and the only engine the app uses on a
Mac.** A Python sidecar, `crates/pwr-mlx/sidecar/pwr_mlx.py` (1,063 lines),
driven by `crates/pwr-mlx` over JSON lines on stdin/stdout — no network
endpoint at all. Libraries pinned: `mlx 0.32.0`, `mlx-lm 0.31.3`,
`mlx-embeddings 0.1.0`, `mlx-vlm 0.6.17`, in a private environment the app
installs (`apps/desktop/src-tauri/src/engine.rs`; [distribution.md](distribution.md)).

Operations: `load` (weights, with optional RoPE scaling; the vision encoder
through `mlx-vlm` when the model has one), `chat`, `attention` (whether the
model's attention is fused, and the score buffer it would need), `cancel`,
`shutdown`. The protocol is documented at the top of the sidecar.

What it owns:

- **Template and tokenizer.** It renders the model's own chat template
  (through transformers' sandboxed Jinja, `trust_remote_code` off, Hub
  offline), so PWR controls reasoning switches the template exposes.
- **Reasoning.** Tracks thinking delimiters, enforces a budget by closing the
  block once, passes `thinking_budget` or `reasoning_effort` to templates that
  read them, and ends `reasoning_unfinished` when a model reopens its thinking
  or stops without answering. Details in [models.md](models.md#reasoning-effort).
- **Prompt cache.** Reuses the KV cache across steps and messages when the new
  prompt extends the old one; a trimmable cache is cut back to the prompt after
  generation, a non-trimmable one (Qwen 3.5/3.6's hybrid layers) keeps a
  checkpoint copy. Background `aside` requests (summaries, reviews) do not
  evict the conversation's cache.
- **Prefill** in chunks whose size shrinks with the context where attention
  materialises its scores (a fixed 8,192-token step once asked for 41.9 GB at
  160k tokens), and stays full where attention is fused.
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
message behind a background summary or a goal's review (plan W5.2, W5.5).

### Where it is fragile

| Defect | Evidence | Plan |
|---|---|---|
| **Stop is not seen during prefill.** `Engine.prefill` has no cancel check, and the generation path checks `cancelled()` only per generated token, so `stream_generate`'s own prefill runs to the end. A long prefill keeps the engine busy after the app shows the turn stopped | `pwr_mlx.py:734-743`, `925` | W5.1 |
| **Background summaries occupy the engine** after every turn; one in flight is not cancelled when a prompt arrives | `serve.rs:2771-2830` | W5.2 |
| **The window is not checked against the real request.** `prepare_context` records and returns the window asked for | `crates/pwr-mlx/src/lib.rs:1474-1485` | W5.3 |
| **The embedding sidecar has no read timeout** — a hang is not a fallback | `crates/pwr-mlx/src/embed.rs:138` | W1.8 |

## llama.cpp (experimental)

`crates/pwr-llama`: reads GGUF metadata for the window, starts a managed
`llama-server` bound to `127.0.0.1`, and converts its OpenAI-compatible
stream. **Command line only**; a release build on macOS ignores
`PWR_BACKEND` and the app runs MLX.

- A server is started **per generation** (`LlamaProvider::chat`,
  `lib.rs:747-750`); keeping it alive across turns is old backlog R.4 (Later).
- Whenever tools are sent, the request sets **`tool_choice: "required"`**
  (`lib.rs:150-155`): a conversation there cannot answer in prose, which is a
  different turn semantics from MLX (plan W5.4).
- Reasoning Effort has no effect: `llama-server` takes no per-request budget.

## The working window

Computed, not probed (`crates/pwr-orchestrator/src/window.rs`; moving to
`pwr-runtime` in plan W2.5). `window::decide` keeps every ceiling and names
the one that bound:

- the model's trained length (and its RoPE extension, if the family has one);
- memory: total memory **minus a reserve of a quarter of it, at least 8 GiB**
  (`window.rs:53`), minus the weights and about 1 GiB of engine overhead,
  divided by what one token of cache costs according to the model's
  `config.json`, with a second copy of the cache for prefill;
- a person's own setting, capped by the above.

This is the window *theoretically allocatable*. It is not the window that is
stable under load, the one that is effective for a task, or the one that stays
interactive; maximising the first does not maximise the others (review §12).
The fit rating in the Model Manager uses the same arithmetic
([models.md](models.md#fit)).

## Performance: where time goes

Probably, in order: generation and reasoning; prefill and cache misses; checks
and builds; model loading; scanning large workspaces. No measurement ranks
them yet on the product path (plan W8.7). Known costs visible in the code:
full-file reads before the index size bound (W1.7), linear index cleanup
(W1.7), wiki and graph rebuilt after turns, a full conversation snapshot
written each turn (W6.3), synchronous file and SQLite work on async paths.
