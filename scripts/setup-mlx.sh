#!/bin/sh
# Creates the Python environment PWR's MLX engine runs in, inside the
# checkout at .venv-mlx, with the versions the engine was measured on.
# The launcher (scripts/pwr) and the desktop app use it when
# PWR_MLX_PYTHON is unset.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
VENV="$ROOT/.venv-mlx"
PYTHON=${PYTHON:-python3}

if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
    printf '%s\n' "The MLX engine needs an Apple-silicon Mac." >&2
    exit 1
fi

"$PYTHON" -m venv "$VENV"
"$VENV/bin/python" -m pip install --upgrade pip
# mlx-vlm loads a model that has a vision encoder once, for text and images
# alike (backlog C.25); measured with 0.6.17, the version these pins resolve.
"$VENV/bin/python" -m pip install "mlx==0.32.0" "mlx-lm==0.31.3" "mlx-embeddings==0.1.0" "mlx-vlm==0.6.17"

# The encoder the semantic section ranking uses (backlog C.22), fetched once
# here so that nothing downloads at run time: the embedding sidecar runs with
# the hub offline. About 0.5 GB, MIT licence. Skip with PWR_SKIP_ENCODER=1.
if [ -z "${PWR_SKIP_ENCODER:-}" ]; then
    "$VENV/bin/python" -c "from huggingface_hub import snapshot_download; snapshot_download('intfloat/multilingual-e5-small')"
fi

printf '%s\n' "MLX engine environment ready: $VENV/bin/python"
