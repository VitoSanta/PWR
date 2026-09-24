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
# Resolve symlinks with the macOS/BSD readlink interface (which has no -f).
while [ -L "$UV" ]; do
    UV_DIR=$(CDPATH= cd -P -- "$(dirname -- "$UV")" && pwd)
    UV_LINK=$(readlink "$UV")
    case "$UV_LINK" in
        /*) UV=$UV_LINK ;;
        *) UV=$UV_DIR/$UV_LINK ;;
    esac
done
UV_DIR=$(CDPATH= cd -P -- "$(dirname -- "$UV")" && pwd)
UV="$UV_DIR/$(basename -- "$UV")"

if ! file "$UV" | grep -q "arm64"; then
    echo "bundle-uv: $UV is not an arm64 executable." >&2
    exit 1
fi
if otool -L "$UV" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/Library/)' | grep -q .; then
    echo "bundle-uv: $UV links libraries outside the system; it would not run elsewhere." >&2
    otool -L "$UV" >&2
    exit 1
fi

mkdir -p "$(dirname "$DEST")"
cp -f "$UV" "$DEST"
chmod 755 "$DEST"
echo "bundle-uv: $("$DEST" --version) -> $DEST"
