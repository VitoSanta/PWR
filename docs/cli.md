# The command line

**Checked against `develop` at `bb49b40e`, 2026-10-14**, from the binary's own
`--help`, which remains the authority on flags. The command line is the
development and research surface; the product is the [desktop app](desktop.md),
which drives the same core through `pwr serve`.

## Set up from a checkout

```bash
sh scripts/setup-mlx.sh
```

```bash
cargo build --release -p pwr-cli
```

`setup-mlx.sh` creates `.venv-mlx` with the pinned engine packages. The
launcher `scripts/pwr` rebuilds the core when sources are newer and uses
`.venv-mlx` when `PWR_MLX_PYTHON` is unset; `sh scripts/install-pwr.sh` links
it into `~/.local/bin` (`PWR_BIN_DIR` moves it).

## Global options

`--json` (machine-readable output) and `--backend mlx|llama` (llama.cpp for
GGUF, experimental). Without a command, `pwr` opens the chat console.

## Commands

| Command | What it does |
|---|---|
| `pwr chat [--model M] [--attach PATH]… [--continue]` | Conversation and agent in the current workspace (terminal UI). `--model` also becomes the workspace's model; `--continue` resumes the latest conversation and reports what changed on disk since |
| `pwr serve --stdio` | The protocol server the app launches ([pwr-serve.md](pwr-serve.md)) |
| `pwr doctor` | Host and backend facts |
| `pwr models download-plan <artifact>` / `download <artifact>` | Plan or perform a verified download of a Gemma pack listed in `strategies/artifacts.json` |
| `pwr repo index [PATH]` | Build or update the repository index |
| `pwr repo rank "<request>" [PATH] [--content] [--semantic]` | The passages a turn would be given, with scores |
| `pwr eval suite <file> [--reports DIR] [--strict]` | Run a regression suite (areas A1–A4) |

Removed on 2026-10-14 with the Gemma-only restructure: `run`, `verify`, `report`, `diagnose`, `session`, `calibrate`, `check-corpus`, `eval run`, `eval compare`, `models inspect`, `models select`, `models certification`, `models certify`. The console's slash commands below remain.

Slash commands inside `pwr chat` and the app: `/changes`, `/verify`,
`/report`, `/diagnose`, `/doctor`.

## Environment variables

| Variable | Effect |
|---|---|
| `PWR_BACKEND` | `mlx` or `llama` (ignored by a release app on macOS, which runs MLX) |
| `PWR_MLX_PYTHON`, `PWR_MLX_SIDECAR` | The engine's interpreter and sidecar script |
| `PWR_MLX_MODELS`, `PWR_LLAMA_MODELS` | Model folders (default `~/.pwr/models`) |
| `PWR_LLAMA_SERVER`, `PWR_LLAMA_PORT` | The `llama-server` binary and port |
| `PWR_HOME` | Replaces `~/.pwr` |
| `PWR_CHAT_HOME` | Chat mode's folder (default `~/.pwr/chat`) |
| `PWR_EVIDENCE_DIR` | Local calibration records (default `~/.pwr/model-evidence`) |
| `PWR_MODEL_ARTIFACTS` | Destination root of `pwr models download` |
| `PWR_HF_BASE_URL`, `HF_TOKEN` | Hugging Face endpoint and token |
| `PWR_ALLOW_UNCONFINED=1` | Run commands unconfined where no sandbox can be built (recorded `sandboxed: false`) |
| `PWR_SEMANTIC_RETRIEVAL=1` | Experimental semantic ranking (`PWR_EMBED_PYTHON`, `PWR_EMBED_SIDECAR` for its encoder) |
| `PWR_BROWSER` | Explicit browser for `look_at`; on macOS the default prefers an installed Playwright Chromium headless shell, then Chrome/Chromium/Edge |
| `PWR_MLX_CACHE_GB` | MLX free-buffer cache limit in GiB (2 by default); not a total-RAM, weight or KV-cache limit |
| `PWR_MLX_CHECKPOINT` | `whole`: copy every layer of the cache between turns; by default (`layers`, since 2026-10-14) only Gemma's sliding layers are copied and the global ones cut back, so the context is held once ([inference.md](inference.md)) |
| `PWR_MLX_SCORES_MB` | The most a prefill step may hold in attention scores, in MB, where attention is not fused (Gemma's 512-wide global heads); unset, 1024 |
| `PWR_MLX_KV_BITS` | `8`: keep the Gemma global layers' keys and values at 8 bits (about half their bytes), where a tier's memory does not hold the full context at 16; `4` is refused (Gemma broke with it). Unset, 16 bits |
| `PWR_MLX_IMAGE_TOKENS` | Gemma 4: tokens an image takes, `70`, `140`, `280`, `560` or `1120` (Google's budgets, the high ones for text and documents); unset, the model's own (280) |
| `PWR_MLX_PROMPT_DUMP` | A folder: every chat request is written there whole with the exact prompt tokens, one file a request (holds model and repository text; never published) |
| `PWR_MLX_TRACE` | Engine request tracing: every request's last messages and the model's raw output, appended as JSON lines (holds model text; never published) |
| `PWR_EMBED_MODEL`, `PWR_EMBED_POOLING` | The embedding model and pooling for semantic retrieval |
| `PWR_HARNESS_REV` | The harness revision a campaign records (declared by the runner) |
| `PWR_SKIP_LOG` | Where the test helpers write what a test run skipped ([testing.md](testing.md)) |
| `PWR_CORE`, `PWR_WORKSPACE` | The core binary and workspace the app uses in development |
| `PWR_DOWNLOAD_FREE_BYTES` | Override of free disk space (tests) |
| `PWR_LIVE_MODEL`, `PWR_LLAMA_SMOKE_MODEL`, `PWR_UPDATE_GOLDEN` | Test-only switches ([testing.md](testing.md)) |

## A small-model campaign, as run on 2026-09-29/30

```bash
PWR_MLX_PYTHON="$HOME/Library/Application Support/ai.pwr.desktop/engine/venv/bin/python" PWR_MLX_SIDECAR="$PWD/crates/pwr-mlx/sidecar/pwr_mlx.py" target/release/pwr --json --backend mlx eval run corpus/small-apps-v1.json --model <ref> --arm b1 --seed 1 --out-dir .pwr/small-runs/<stamp>/<slug>
```

This measures the scripted loop, not the app's path (plan W2.4). The app's
path is exercised by driving `pwr serve` over ACP, as the stack-matrix runner
does ([evaluation.md](evaluation.md#the-stack-matrix)).

## The stack-matrix runner (the app's path, with hidden tests)

```bash
python3 evidence/stack-matrix/runner/run.py list
python3 evidence/stack-matrix/runner/run.py reference [TASK …]        # prove tasks sound
sh evidence/stack-matrix/runner/pin.sh                                 # pin the release binary and its sidecar
PWR_BIN="/path/to/pinned-bin/pwr-<rev>" PWR_EVIDENCE_MODEL=<ref> \
  python3 evidence/stack-matrix/runner/run.py run --run <name> [--split dev] [TASK …]
python3 evidence/stack-matrix/runner/watch.py                          # follow the newest run token by token
```

`PWR_BIN`, `PWR_EVIDENCE_MODEL`, `PWR_EVIDENCE_TASKS`, `PWR_EVIDENCE_RESULTS`,
`PWR_EVIDENCE_PROTOCOL` and `PWR_MLX_PYTHON`/`PWR_MLX_SIDECAR` configure the
runner, not the core. **One model at a time on this machine**: a second engine
beside the first overruns the GPU's working set (about 55 GB of 64 GB) and both
emit text without meaning (2026-10-01; [experiment-log.md](experiment-log.md)).
