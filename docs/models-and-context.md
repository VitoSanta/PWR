# Context, hardware and the Model Manager

**Written 2026-09-23** against the revision that introduced them. This
describes what is implemented and what it does not do; it makes no claim about
model quality or speed.

## Where things are in the app

The conversation keeps the centre. Technical runtime state moved out of the
sidebar to the top bar of the active conversation:

- **Context indicator** — `Context 42% · 54k / 128k`. Click for the context
  panel: used / window / remaining, what fills the window, the auto-compaction
  threshold, the last compaction, and **Compact now**.
- **Model chip** — the model in use. Click to switch between the models the
  engine found, change the working window (−/＋), and open the **Model
  Manager**. With no model chosen, the empty conversation offers the Model
  Manager directly.

The sidebar holds the workspace and the conversations only.

The model popover also shows the selected model's compatibility status
(Verified, Locally calibrated, Provisional, Limited, Incompatible), Quick
Calibration and the Reasoning Effort control -- described in
[`model-compatibility.md`](model-compatibility.md) (2026-09-24).

## Context

### What is reported

`_pwr/context` (see [`pwr-serve.md`](pwr-serve.md)) returns, for a
session:

| Field | Source | Exact? |
|---|---|---|
| `window` | the working window in force (computed for the host, or the person's setting, capped) | exact: it is what the engine is asked to serve |
| `used` | the engine's own prompt + generated count after the last reply (`usedSource: "engine"`); before any reply, the estimate (`usedSource: "estimate"`) | engine count is exact for that request; it is one reply stale |
| `composition` | the session's messages, grouped: instructions and harness rules, conversation, repository and retrieved code (file reads, searches, retrieved passages), other tool results, task state (ledgers), compacted memory | **estimate**: characters divided by four |
| `autoCompact` | threshold in percent and tokens, its bounds, whether it is the default | exact |
| `lastCompaction` | the last `context.compacted` event of the conversation | exact |

The interface labels the composition as an estimate and says which "used"
figure it shows. Tool schemas and the chat template are in the engine's count
but not in the composition, so the two differ by that overhead.

### Automatic and manual compaction: one implementation

`pwr_orchestrator::compaction::compact` is the only conversation
compaction. The turn calls it when the prompt passes the threshold between two
actions (default 75 % of the window, settable per workspace from 50 % to 90 %
in the context panel, saved as `compact_at_percent` in
`.pwr/chat-config.json`). **Compact now** (`_pwr/compact`) calls the same
function between turns; the only difference is how much recent conversation
it keeps verbatim (a quarter of the current conversation, never more than the
automatic tail).

The summary is mechanical, not written by a model. It keeps:

- the system prompt (never folded) — PWR's instructions and harness rules;
- the first request verbatim (bounded to 800 characters) and every later
  request as a line; the newest messages verbatim in the tail;
- the actions taken, with their paths or commands, and every path worked on;
- what the model said (its decisions and where it left off);
- every file the conversation changed, with the content hash it now has — from
  the checkpoint, the audit's account, not from the messages;
- failures not followed by a success of the same action: the errors still
  open, with the last line of stderr for a failed command;
- what the repository's checks said last.

It drops tool output (already consumed), superseded ledgers and retrieved
passages (both rebuilt on demand) and says how many of each. A later
compaction merges the earlier record rather than nesting it. Compaction
changes only what the model is sent: the workspace, the repository index and
the retrieval index are never touched.

Both triggers record `context.compacted` in `.pwr/state.sqlite` with
`trigger` (`manual` / `automatic`), `session_id`, `model`, `window`,
`estimated_tokens_before`, `estimated_tokens_after` and `estimate_basis`; the
event's own timestamp is the time. A manual compaction also writes a
conversation snapshot, so reopening the conversation continues from the
compacted state. The app shows each compaction in the conversation.

Tests: `crates/pwr-orchestrator/tests/context_compaction.rs` (what is
preserved, both triggers produce identical prompts, a second compaction keeps
the first's record, well-formed prompts, the audit event, the workspace and
index untouched), the serve protocol tests in `crates/pwr-cli/src/serve.rs`,
and `two_loops.rs` (automatic compaction through a real `take_turn`).

**Not unified yet:** the scripted run (`pwr run`, the research loop) still
has its own ledger compaction; backlog C.1.

## Hardware detection

`pwr_runtime::host::detect_host` returns a normalized `HostProfile`:
platform, OS name and version, architecture (`arm64` / `x86_64`), CPU, Apple
chip (name, generation, tier), memory (total, available, whether unified),
GPUs, and free disk space on the drive that holds the models folder. Facts
that could not be read are `null` and named in `unknown`.

- Memory, OS, CPU and disks come from the `sysinfo` library, not from parsing
  command output. On macOS "available" memory is the OS's estimate
  (compression and reclaim make it elastic) and is flagged so.
- Apple Silicon GPUs share the machine's memory; no separate VRAM is claimed.
- NVIDIA VRAM comes from `nvidia-smi` (exact, in MiB). Other GPUs on Windows
  are listed by name from `Win32_VideoController`, but their memory is left
  unknown: that class's `AdapterRAM` is 32-bit and wraps above 4 GiB.
- Each engine's readiness is checked by running it: MLX needs Apple Silicon and
  a Python that imports `mlx_lm`; llama.cpp needs `llama-server`. A missing
  engine is reported with the step that fixes it.

The older `hardware::probe_hardware` is unchanged: calibrations are keyed on a
hash of its fields, and changing it would invalidate them.

## Compatibility estimates

`pwr_models::fit::estimate` rates one variant on one machine. It answers
"can this be loaded here with a useful context?", not "how fast is it". It
uses the same arithmetic PWR uses to choose the working window after a
model is chosen (`window::decide`), so a rating and the window the app later
computes agree.

Inputs: file size of the variant; the memory one token of context costs, from
the model's `config.json` (the repository's own for MLX, the base model's for
GGUF); total memory; exact dedicated VRAM where known.

Assumptions, stated in every estimate:

- the weights occupy their file size once loaded;
- the engine costs about 1 GiB beyond the weights;
- the host keeps a quarter of its memory, at least 8 GiB, for the system and
  other apps (the window module's reserve);
- prefill holds a second copy of the context cache (MLX, as PWR's engine
  reports for fused attention) or four times the cache (llama.cpp, which
  reports nothing, the conservative rule);
- a model larger than a discrete GPU's memory is split with system memory by
  llama.cpp, which is much slower, so it is never rated better than "Should
  fit".

| Rating | Meaning |
|---|---|
| Recommended | a window of at least 32k tokens fits, and the weights take at most 60 % of what is left after the reserve |
| Should fit | a window of at least 16k fits |
| Tight fit | 8k–16k fits: usable, compacts often |
| Not recommended | the weights do not fit, or less than 8k of context would |
| Incompatible | no engine here runs the format (MLX off Apple Silicon), or it needs repository code |
| Unknown | the machine's memory or the model's size could not be read |

Without a `config.json` the context cost is unknown; the rating then uses the
share of memory the weights take and says the context was not estimated.
Nothing is tuned for the development machine: the same rules rate a 16 GB Mac
and a 128 GB one, and the tests pin both.

## The Model Manager

Opened from the model chip ("Manage models…") or from an empty conversation.

**Source.** The Hugging Face Hub's JSON API only (`/api/models`,
`/api/models/{repo}`, `/api/models/{repo}/tree/{revision}`,
`resolve/{revision}/config.json`). No HTML is read. `PWR_HF_BASE_URL`
points it elsewhere; `HF_TOKEN` is sent for gated repositories.

**Formats and engines.** MLX (PWR's engine, Apple Silicon) and GGUF
(llama.cpp). A search is per format; the app starts on the format of the
engine it runs. An MLX variant is the repository's config, weights, tokenizer
and template files (never `.py`); a GGUF variant is one quantization, all
shards of a split file, without projectors or importance matrices.
Repositories whose config needs custom code (`auto_map`) are marked
incompatible: PWR never runs repository code and never enables
`trust_remote_code`.

**Cards** show name, author, base model, parameters, architecture, context
length, license, format, engine, downloads and likes as the Hub reports them,
each labelled with its source where it matters. Missing values read
"unknown". GGUF quantizations come from the file name and say so; MLX
quantization comes from the model's config. Every variant carries its size,
its rating (click for the explanation and assumptions), whether it is on disk
and whether the engine already lists it.

**Filters** (applied in the core): fits this machine, parameter range, context
length, download size, quantization, family (name or base model).

**Downloads** go to the engine's models folder — `PWR_MLX_MODELS` /
`PWR_LLAMA_MODELS`, by default `~/.lmstudio/models` — as
`<owner>/<name>/<files>`, which is where the engines look, so a finished MLX
download is selectable at once (it appears in the model chip). The client
names only repository, commit, variant and format; the core re-reads the file
list, sizes and checksums from the Hub at that commit. Then:

1. every file must have a size and a checksum — LFS SHA-256 for large files,
   the git blob SHA-1 for small ones — or the download is refused;
2. paths must stay inside the model folder;
3. the disk must hold what is left to download plus 5 GiB;
4. an existing file that matches is kept (`already_present`); one that does
   not is never overwritten — the download stops and names the file to move;
5. bytes go to `<file>.part`, verified, then renamed; an interrupted download
   resumes from its `.part` (HTTP range); a complete but wrong `.part` is
   removed;
6. progress, verification, completion, failure (with its kind: disk,
   conflict, network, verification) and cancellation are sent as
   `_pwr/download_progress` with the state the core's state machine holds.

The CLI's `pwr models download <artifact>` uses the same downloader.

**States the manager shows:** loading, no results, Hub unreachable or
rate-limited (with retry), engine unavailable (with the fix), incompatible,
downloading (bytes, cancel), verifying, failed (with the reason, retry),
paused (bytes kept, resume), downloaded, available (use).

**Models on this Mac.** The "On this Mac" tab lists every model in the
engines' folders (MLX folders and GGUF files, one entry per split, unfinished
downloads marked), with its size and whether this workspace uses it. **Delete**
(also on a downloaded variant in Discover) asks for confirmation, then removes
the model's files permanently (`_pwr/model_delete`). It is refused for the
model this workspace uses, for anything that resolves outside the models
folder (symlinks included), for a folder with no `config.json`, and for a model
folder that contains another folder; empty parent folders are removed up to,
never including, the models folder.

When a download is for an engine the app is not running (a GGUF while the app
runs MLX), it completes and says the remaining step: start the app with
`PWR_BACKEND=llama`. PWR does not switch engines in a running app.

## Limitations

- Composition and "estimated" figures use four characters per token; code and
  JSON tokenize denser, so they understate.
- The fit estimate does not model speed, quantization quality or MoE sparsity
  (an MoE's full weights must still be resident).
- The GGUF context cost is read from the base model's config; a GGUF whose
  base model is missing or gated gets the "context not estimated" rating.
- Windows: hardware detection and GGUF downloads are implemented but have not
  been run on Windows; command sandboxing there is still missing (see
  [SECURITY.md](../SECURITY.md)).
- Discover loads 20 Hub repositories per page in download order. **Load more
  models** follows the Hub's next-page cursor until the catalog is exhausted;
  the parameter range is sent to the Hub, while fit and variant filters apply
  to each page and can leave a page empty. Enriching a page makes
  additional tree and config requests, so anonymous use is subject to the Hub's
  rate limits.
- Downloads are not listed across app restarts except through the files on
  disk (a `.part` shows as "downloaded N MB · Resume").
