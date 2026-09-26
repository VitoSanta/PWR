#!/bin/sh
# Builds the release binary and pins it, with the MLX sidecar it reads, under
# the current revision: runs name a pinned pair, so a rebuild or an edit to
# the sidecar during a campaign does not change what is being measured.
set -eu
repo=$(cd "$(dirname "$0")/../../.." && pwd)
out=${PWR_EVIDENCE_BIN:-$HOME/Desktop/pwr-evidence/bin}
rev=$(git -C "$repo" rev-parse --short HEAD)
if [ -n "$(git -C "$repo" status --porcelain -- crates apps)" ]; then
  echo "the source tree has uncommitted changes; commit them first" >&2
  exit 1
fi
(cd "$repo" && cargo build --release -p pwr-cli)
mkdir -p "$out"
cp "$repo/target/release/pwr" "$out/pwr-$rev"
rm -rf "$out/sidecar-$rev"
cp -R "$repo/crates/pwr-mlx/sidecar" "$out/sidecar-$rev"
echo "$out/pwr-$rev"
