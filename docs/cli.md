# The command line

**Checked against `develop` at `aa1d0707`, 2026-10-01**, from the binary's own
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
| `pwr run "<task>"` | An unattended scripted run (the research loop; see [agent-loop.md](agent-loop.md#the-scripted-run)). Flags: `--model`, `--profile`, `--dry-run`, `--approve <grants>` (`dependency-change`, `history-rewrite`, `publish`, `network-access`, `local-service`, `toolchain-install`, `container-engine`, `verifier-proposal`, `outside-sandbox`, `outside-workspace`), `--turn-timeout-secs` (900), `--session <name>`, `--plan`, `--provision` (toolchain installs: grants any program **and** the network), `--max-actions` |
| `pwr verify [RUN_ID] [--scope targeted\|full]` | Run the repository's checks |
| `pwr doctor` | Host and backend facts |
| `pwr report <ID> [--format json\|md\|jsonl]` | A run's report from its log; `jsonl` also verifies the hash chain |
| `pwr diagnose <ID>` | Known failure patterns found in a run's log |
| `pwr session list \| show <name>` | Named sessions of the scripted loop, reconstructed from the log |
| `pwr models inspect <M> [--probe]` | Inspect a model's metadata; `--probe` runs capability trials (`--probe-trials`, default 3) |
| `pwr models select` | Explain which deployment automatic selection would choose and why (loads nothing) |
| `pwr models download-plan <artifact>` / `download <artifact>` | Plan or perform a verified download of an artifact listed in `strategies/artifacts.json` |
| `pwr models certification <M>` / `certify <M> --level … --rationale …` | Read or record a certification level (`unsupported`, `experimental`, `compatible`, `certified`); under audit in plan W10.1 |
| `pwr calibrate <M> [--ladder 2048,4096,8192]` | The old context-ladder calibration (research; the app computes the window instead) |
| `pwr repo index [PATH]` | Build or update the repository index |
| `pwr repo rank "<request>" [PATH] [--content] [--semantic]` | The passages a turn would be given, with scores |
| `pwr eval run <suite> --model M` | Run a frozen corpus; see [evaluation.md](evaluation.md). Flags include `--arm`, `--mode`, `--seed`, `--only <task>`, `--resume`, `--oracle-context`, `--context-policy`, and `--reasoning-effort low\|medium\|high\|off` (default **medium** since 2026-09-30; `off` is what every earlier campaign measured and must be declared as a treatment) |
| `pwr eval compare <control> <treatment> [--strict] [--declare FIELD]…` | Pair two campaigns (permissive by default today; plan W8.1) |
| `pwr eval suite <file> [--reports DIR] [--strict]` | Run a regression suite (areas A1–A4) |
| `pwr check-corpus <suite>` | Check a corpus is fair before measuring on it |

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
