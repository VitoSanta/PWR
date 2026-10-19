#!/bin/sh
# Build the public Apple-silicon DMG locally. This script never publishes.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUTPUT_DIR=${OUTPUT_DIR:-"$ROOT/dist/release"}
APP_DIR="$ROOT/apps/desktop"
TAURI_DIR="$APP_DIR/src-tauri"
DMG_DIR="$TAURI_DIR/target/release/bundle/dmg"
EXPECTED_VERSION=${EXPECTED_VERSION:-$(node -p "require(process.argv[1]).version" "$APP_DIR/package.json")}

fail() { printf 'release-macos: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "must run on macOS"
[ "$(uname -m)" = arm64 ] || fail "must run on Apple silicon (arm64)"
command -v cargo >/dev/null || fail "cargo is required"
command -v npm >/dev/null || fail "npm is required"
command -v uv >/dev/null || fail "uv is required (brew install uv)"
command -v hdiutil >/dev/null || fail "hdiutil is required"
command -v shasum >/dev/null || fail "shasum is required"
command -v python3 >/dev/null || fail "python3 is required"

# Refuse dirty sources before cargo clean/npm/build can change local outputs.
PROVENANCE_RECORD=$(mktemp "${TMPDIR:-/tmp}/pwr-provenance.XXXXXX")
trap 'rm -f "$PROVENANCE_RECORD"' EXIT HUP INT TERM
if [ -n "${GITHUB_REF_NAME:-}" ]; then
  python3 "$ROOT/scripts/release-provenance.py" start --root "$ROOT" \
    --expected-ref "$GITHUB_REF_NAME" --record "$PROVENANCE_RECORD"
else
  python3 "$ROOT/scripts/release-provenance.py" start --root "$ROOT" \
    --record "$PROVENANCE_RECORD"
fi

node -e 'const fs=require("node:fs"); const expected=process.argv.at(-1); for (const f of process.argv.slice(1,-1)) { const d=JSON.parse(fs.readFileSync(f,"utf8")); const v=f.endsWith("package-lock.json") ? d.packages[""].version : d.version; if (v!==expected) { console.error(`${f}: expected ${expected}, found ${v}`); process.exitCode=1; } }' \
  "$APP_DIR/package.json" "$APP_DIR/package-lock.json" "$TAURI_DIR/tauri.conf.json" "$EXPECTED_VERSION" || fail "desktop release versions are inconsistent"
grep -Fqx "version = \"$EXPECTED_VERSION\"" "$ROOT/Cargo.toml" || fail "workspace version is not $EXPECTED_VERSION"
grep -Fqx "version = \"$EXPECTED_VERSION\"" "$TAURI_DIR/Cargo.toml" || fail "Tauri package version is not $EXPECTED_VERSION"

# An unsigned/unconfigured development build is still supported. A partially
# configured release fails rather than shipping a broken update channel.
UPDATES_ENABLED=0
if [ -n "${PWR_UPDATER_PUBLIC_KEY:-}${PWR_UPDATER_ENDPOINT:-}${PWR_UPDATER_CHANNEL:-}${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  PWR_UPDATER_PUBLIC_KEY=${PWR_UPDATER_PUBLIC_KEY:-$(cat "$TAURI_DIR/updater.pub")}
  export PWR_UPDATER_PUBLIC_KEY
  [ -n "$PWR_UPDATER_PUBLIC_KEY" ] || fail "missing updater public key"
  [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] || fail "missing updater signing key"
  case "${PWR_UPDATER_ENDPOINT:-}" in https://*) ;; *) fail "updater endpoint must use HTTPS" ;; esac
  case "${PWR_UPDATER_CHANNEL:-}" in stable|prerelease) ;; *) fail "updater channel must be stable or prerelease" ;; esac
  if [ "$PWR_UPDATER_CHANNEL" = stable ]; then
    case "$EXPECTED_VERSION" in *-*) fail "a stable feed cannot distribute a prerelease" ;; esac
  fi
  export TAURI_CONFIG='{"bundle":{"createUpdaterArtifacts":true}}'
  UPDATES_ENABLED=1
fi

printf '%s\n' "release-macos: cleaning Rust build outputs"
cargo clean --manifest-path "$ROOT/Cargo.toml"
cargo clean --manifest-path "$TAURI_DIR/Cargo.toml"

printf '%s\n' "release-macos: installing locked frontend dependencies"
(cd "$APP_DIR" && npm ci)

printf '%s\n' "release-macos: building Tauri DMG"
(cd "$APP_DIR" && npx tauri build --bundles app,dmg)

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
  rm -f "$PROVENANCE_RECORD"
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
python3 "$ROOT/scripts/verify-notices.py" "$APP/Contents/Resources/notices" \
  --target aarch64-apple-darwin --uv "$APP/Contents/Resources/uv"
file "$APP/Contents/Resources/pwr" | grep -q 'arm64' || fail "bundled PWR core is not arm64"

cd "$OUTPUT_DIR"
if [ "$UPDATES_ENABLED" -eq 1 ]; then
  UPDATER_DIR="$TAURI_DIR/target/release/bundle/macos"
  test -s "$UPDATER_DIR/PWR.app.tar.gz" || fail "missing updater archive"
  test -s "$UPDATER_DIR/PWR.app.tar.gz.sig" || fail "missing updater signature"
  cp "$UPDATER_DIR/PWR.app.tar.gz" "$UPDATER_DIR/PWR.app.tar.gz.sig" "$OUTPUT_DIR/"
  python3 "$ROOT/scripts/updater-manifest.py" --version "$EXPECTED_VERSION" \
    --notes "$ROOT/docs/release/v$EXPECTED_VERSION-release-notes.md" \
    --archive "$OUTPUT_DIR/PWR.app.tar.gz" --signature "$OUTPUT_DIR/PWR.app.tar.gz.sig" \
    --repository "${GITHUB_REPOSITORY:-VitoSanta/PWR}" --output "$OUTPUT_DIR/latest.json"
fi
if [ "$UPDATES_ENABLED" -eq 1 ]; then
  set -- --artifact PWR-macOS-arm64.dmg --artifact PWR.app.tar.gz --artifact PWR.app.tar.gz.sig --artifact latest.json
else
  set -- --artifact PWR-macOS-arm64.dmg
fi
python3 "$ROOT/scripts/release-provenance.py" finish --root "$ROOT" \
  --record "$PROVENANCE_RECORD" --directory "$OUTPUT_DIR" "$@" \
  --output "$OUTPUT_DIR/build-provenance.json"
if [ "$UPDATES_ENABLED" -eq 1 ]; then
  shasum -a 256 PWR-macOS-arm64.dmg PWR.app.tar.gz PWR.app.tar.gz.sig latest.json build-provenance.json > SHA256SUMS.txt
else
  shasum -a 256 PWR-macOS-arm64.dmg build-provenance.json > SHA256SUMS.txt
fi
shasum -a 256 -c SHA256SUMS.txt
printf '\n%s\n' "release-macos: ready: $ARTIFACT" "$OUTPUT_DIR/SHA256SUMS.txt"
