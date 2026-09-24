#!/bin/sh
# Puts the `uv` binary the desktop app installs its MLX engine with into
# apps/desktop/src-tauri/resources/uv, where the bundle picks it up. Run by
# `tauri build` (beforeBuildCommand); the binary is not committed.
#
# It copies the `uv` found on PATH (or named by $UV), after checking that it
# is an arm64 executable that needs only system libraries, so the copy runs
# on any Apple-silicon Mac. uv is MIT / Apache-2.0 licensed (astral-sh/uv).
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DEST="$ROOT/apps/desktop/src-tauri/resources/uv"

[ "$(uname -s)" = "Darwin" ] || { echo "bundle-uv: skipped (not macOS)"; exit 0; }

UV=${UV:-$(command -v uv || true)}
if [ -z "$UV" ]; then
    echo "bundle-uv: uv not found. Install it (brew install uv) or set UV=/path/to/uv." >&2
    exit 1
fi
UV=$(readlink -f "$UV")

if ! file "$UV" | grep -q "arm64"; then
    echo "bundle-uv: $UV is not an arm64 executable." >&2
    exit 1
fi
if otool -L "$UV" | tail -n +2 | grep -v -E '^\s*(/usr/lib/|/System/Library/)' | grep -q .; then
    echo "bundle-uv: $UV links libraries outside the system; it would not run elsewhere." >&2
    otool -L "$UV" >&2
    exit 1
fi

mkdir -p "$(dirname "$DEST")"
cp -f "$UV" "$DEST"
chmod 755 "$DEST"
echo "bundle-uv: $("$DEST" --version) -> $DEST"
