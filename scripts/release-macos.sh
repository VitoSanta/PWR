#!/bin/sh
# Build the public Apple-silicon DMG locally. This script never publishes.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUTPUT_DIR=${OUTPUT_DIR:-"$ROOT/dist/release"}
APP_DIR="$ROOT/apps/desktop"
TAURI_DIR="$APP_DIR/src-tauri"
DMG_DIR="$TAURI_DIR/target/release/bundle/dmg"
EXPECTED_VERSION=0.1.0-alpha

fail() { printf 'release-macos: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "must run on macOS"
[ "$(uname -m)" = arm64 ] || fail "must run on Apple silicon (arm64)"
command -v cargo >/dev/null || fail "cargo is required"
command -v npm >/dev/null || fail "npm is required"
command -v uv >/dev/null || fail "uv is required (brew install uv)"
command -v hdiutil >/dev/null || fail "hdiutil is required"
command -v shasum >/dev/null || fail "shasum is required"

node -e 'const fs=require("node:fs"); const expected=process.argv.at(-1); for (const f of process.argv.slice(1,-1)) { const d=JSON.parse(fs.readFileSync(f,"utf8")); const v=f.endsWith("package-lock.json") ? d.packages[""].version : d.version; if (v!==expected) { console.error(`${f}: expected ${expected}, found ${v}`); process.exitCode=1; } }' \
  "$APP_DIR/package.json" "$APP_DIR/package-lock.json" "$TAURI_DIR/tauri.conf.json" "$EXPECTED_VERSION" || fail "desktop release versions are inconsistent"
grep -Eq '^version = "0\.1\.0-alpha"$' "$ROOT/Cargo.toml" || fail "workspace version is not $EXPECTED_VERSION"
grep -Eq '^version = "0\.1\.0-alpha"$' "$TAURI_DIR/Cargo.toml" || fail "Tauri package version is not $EXPECTED_VERSION"

printf '%s\n' "release-macos: cleaning Rust build outputs"
cargo clean --manifest-path "$ROOT/Cargo.toml"
cargo clean --manifest-path "$TAURI_DIR/Cargo.toml"

printf '%s\n' "release-macos: installing locked frontend dependencies"
(cd "$APP_DIR" && npm ci)

printf '%s\n' "release-macos: building Tauri DMG"
(cd "$APP_DIR" && npx tauri build --bundles dmg)

set -- "$DMG_DIR"/*.dmg
[ -f "$1" ] || fail "Tauri did not produce a DMG in $DMG_DIR"
[ "$#" -eq 1 ] || fail "expected one Tauri DMG, found $#"
BUILT_DMG=$1

mkdir -p "$OUTPUT_DIR"
ARTIFACT="$OUTPUT_DIR/PWR-macOS-arm64.dmg"
cp "$BUILT_DMG" "$ARTIFACT"

printf '%s\n' "release-macos: verifying DMG and bundled application"
hdiutil verify "$ARTIFACT"
MOUNT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/pwr-release.XXXXXX")
cleanup() {
  hdiutil detach "$MOUNT_DIR" -quiet >/dev/null 2>&1 || true
  rmdir "$MOUNT_DIR" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM
diskutil image attach --readOnly --mountOptions nobrowse --mountPoint "$MOUNT_DIR" "$ARTIFACT" >/dev/null
APP="$MOUNT_DIR/PWR.app"
[ -d "$APP" ] || fail "mounted DMG does not contain PWR.app at its root"
APP_VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
[ "$APP_VERSION" = "$EXPECTED_VERSION" ] || fail "bundled app version is $APP_VERSION, expected $EXPECTED_VERSION"
EXECUTABLE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")
[ -x "$APP/Contents/MacOS/$EXECUTABLE" ] || fail "app executable is missing"
file "$APP/Contents/MacOS/$EXECUTABLE" | grep -q 'arm64' || fail "app executable is not arm64"
codesign --verify --deep --strict "$APP"
[ -x "$APP/Contents/Resources/uv" ] || fail "bundled uv executable is missing"
[ -x "$APP/Contents/Resources/pwr" ] || fail "bundled PWR core executable is missing"
file "$APP/Contents/Resources/pwr" | grep -q 'arm64' || fail "bundled PWR core is not arm64"

cd "$OUTPUT_DIR"
shasum -a 256 PWR-macOS-arm64.dmg > SHA256SUMS.txt
shasum -a 256 -c SHA256SUMS.txt
printf '\n%s\n' "release-macos: ready: $ARTIFACT" "$OUTPUT_DIR/SHA256SUMS.txt"
