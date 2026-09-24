#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BIN_DIR=${POORAI_BIN_DIR:-"$HOME/.local/bin"}

cargo build --manifest-path "$ROOT/Cargo.toml" --release -p pwr-cli

mkdir -p "$BIN_DIR"
ln -sf "$ROOT/scripts/pwr" "$BIN_DIR/pwr"

printf '%s\n' "Installed pwr in $BIN_DIR/pwr"
printf '%s\n' "Run 'pwr' from a workspace to start the terminal coding agent."
case ":${PATH}:" in
    *:"$BIN_DIR":*) ;;
    *) printf '%s\n' "Add $BIN_DIR to PATH in ~/.zshrc if it is not already present." ;;
esac
