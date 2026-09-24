# Current CLI: manual local testing

**Current as of 2026-09-23.** The command line is the development and research
surface: it makes model selection, probes, suites, runs and failures
reproducible from a terminal. The product surface is the desktop app
(`apps/desktop`, Tauri 2 + Angular), which drives the same core through
`pwr serve --stdio` ([`pwr-serve.md`](pwr-serve.md)). Every command
takes `--help`, which is the authority on its flags.

PWR runs models itself: the engines are `mlx` (the default, Apple silicon)
and `llama` (GGUF through llama.cpp, limited). Ollama and LM Studio were
removed on 2026-09-19; references to them elsewhere describe the old regime.
No cloud service is used.

## Set up once

```bash
sh scripts/setup-mlx.sh           # .venv-mlx: mlx, mlx-lm, mlx-vlm, mlx-embeddings, and the local encoder
cargo build --release -p pwr-cli
```

MLX models are folders under `~/.lmstudio/models/<publisher>/<name>` (the
folder is only storage) or under `POORAI_MLX_MODELS`. `POORAI_MLX_PYTHON`
names another interpreter; without it the launcher and the app look for
`.venv-mlx`, then the engine spike's environment on the maintainer's machine.

## A conversation

```bash
cd /path/to/the/workspace
/path/to/PWR/target/release/pwr chat --model lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit
```

The model, window and settings are saved in the workspace
(`.poorai/chat-config.json`), so the next `pwr chat` reuses them. The
window is computed from the model's metadata and this host's memory; a
capability probe is optional for a conversation.

**Permissions.** A conversation runs in **Ask** mode by default -- it asks
before changing dependencies, reaching the network, installing toolchains,
rewriting Git history and publishing -- or in **Auto**, which grants all of
them. The app's **Auto-approve** switch is Auto when on; over the protocol it is `_poorai/approvals`
with `mode`. Commands are confined to the workspace by macOS's sandbox; where
no sandbox applies they are refused unless `POORAI_ALLOW_UNCONFINED=1`. See
[SECURITY.md](../SECURITY.md).

**Images.** A model whose `config.json` declares a vision encoder (Qwen3.6-35B-A3B,
Qwen3.8-27B on the maintainer's host) is loaded once through `mlx-vlm` and
reads images attached in the app -- dropped like any file -- or sent as ACP
`image` blocks. Other models refuse a message with an image. The app marks the
models that see. (Backlog C.25.)

**Chat mode.** In the app, "Chat without a workspace" opens a conversation
with no project: the model reads only what is attached -- files, folders,
images -- and cannot edit or run anything. It lives in `~/.poorai/chat`
(`POORAI_CHAT_HOME`). (Backlog C.26.)

## An unattended run

```bash
pwr models inspect lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit --probe   # once per model
pwr run "the task, stated fully" --model lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit --max-actions 40
```

`run` requires a probed capability artifact for the deployment: without one
it stops with `incompatible_model`. **The newest artifact is the one read**,
so an `inspect` without `--probe` after a probe hides the probe's
observations -- probe again, or leave the probed artifact the newest. A run
grants only what `--approve` names, honours `.poorai/protected.json`
(`{"protected": ["path", ...]}`; an unreadable file stops the run), and ends
`verified` only when the workspace's own checks pass. `pwr report <id>`
and `pwr diagnose <id>` read what happened, including failed generations.

## Evaluation

```bash
sh suites/run.sh a3                       # the editing suite on the MLX engine
pwr eval run <corpus.json> --model <ref> --arm b0 --seed 1   # the fixed tool-loop baseline
pwr eval run <corpus.json> --model <ref> --arm b1 --seed 1   # PWR
```

`--arm b0` is the conventional loop -- same tools, parser and policy, no
harness management -- which is the control for any uplift claim (backlog
R.10). Evaluations always rank context lexically, so a measured regime does
not change with what is installed.

## Experimental switches

- `POORAI_SEMANTIC_RETRIEVAL=1` -- rank document sections by meaning as well
  as by words, with a small local encoder (backlog C.22). Needs the encoder
  cached (`setup-mlx.sh` fetches it) and `mlx-embeddings`; without them the
  ranking is lexical and stderr says why. `POORAI_EMBED_PYTHON` names its
  interpreter.
- `pwr repo rank "<request>" [--semantic] [--content]` -- print the
  passages a turn would be given, with scores and reasons.
- `search` with `in_dependencies: true` is the model's tool for reading the
  project's installed dependencies; it has no flag.

## GGUF through llama.cpp (limited)

```bash
export POORAI_LLAMA_MODELS="$HOME/.poorai/artifacts"
export POORAI_LLAMA_SERVER="$(command -v llama-server)"
PWR --backend llama chat --model <publisher>/<repo>/<file>.gguf
```

The engine starts and owns `llama-server`, and constrains a tool call to the
offered catalogue. **The server is started per generation** (backlog R.4), so
each turn pays the model load and loses the KV cache: usable for a watched
check, not for performance.

## The terminal launcher

`scripts/install-pwr.sh` installs the `pwr` CLI in `~/.local/bin`. Run it
from a workspace to start the terminal coding agent. The supported desktop
client is the Tauri app in `apps/desktop`; build it with `npx tauri build`.
