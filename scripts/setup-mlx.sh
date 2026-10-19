#!/bin/sh
# Creates the Python environment PWR's MLX engine runs in, inside the
# checkout at .venv-mlx, with the versions the engine was measured on.
# The launcher (scripts/pwr) and the desktop app use it when
# PWR_MLX_PYTHON is unset.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
VENV="$ROOT/.venv-mlx"
SETUP_UV=${UV:-$(command -v uv || true)}
[ -n "$SETUP_UV" ] || { printf "%s\n" "Install uv or set UV=/path/to/uv." >&2; exit 1; }

if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
    printf '%s\n' "The MLX engine needs an Apple-silicon Mac." >&2
    exit 1
fi

"$SETUP_UV" venv --python "${PWR_MLX_PYTHON:-3.14.8}" "$VENV"
# mlx-vlm loads a model that has a vision encoder once, for text and images
# alike. Gemma runtime pins updated 2026-10-14; Python 3.14.8 in the app.
"$SETUP_UV" pip install --python "$VENV/bin/python" --require-hashes --only-binary :all: --no-deps \
    --requirement "$ROOT/apps/desktop/src-tauri/engine/requirements-macos-arm64-py314.lock"

# The encoder the semantic section ranking uses (backlog C.22), fetched once
# here so that nothing downloads at run time: the embedding sidecar runs with
# the hub offline. About 0.5 GB, MIT licence. Skip with PWR_SKIP_ENCODER=1.
if [ -z "${PWR_SKIP_ENCODER:-}" ]; then
    "$VENV/bin/python" "$ROOT/crates/pwr-mlx/sidecar/encoder_assets.py" --install
fi

printf '%s\n' "MLX engine environment ready: $VENV/bin/python"
