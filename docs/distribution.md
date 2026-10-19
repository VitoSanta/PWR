# Building and distributing

**Packaging source reviewed 2026-10-07 at `develop` `90fe0dd4`.**
The next candidate is not frozen; see [release readiness](release/next-release-readiness.md).

## What ships

A DMG for Apple-silicon Macs (`PWR-macOS-arm64.dmg`) with `SHA256SUMS.txt`,
published as a GitHub prerelease. The app is **ad-hoc signed, not notarized**:
macOS may block the first open, and the README gives Apple's documented
*Open Anyway* procedure. Latest: **v0.4.0-alpha** (tag `v0.4.0-alpha`,
2026-10-15); release notes in [release/](release/).

On first launch the app installs its engine: a private Python environment with
the pinned packages, about 1.2 GB, in
`~/Library/Application Support/ai.pwr.desktop/engine`, using the `uv` bundled
in the app (`apps/desktop/src-tauri/src/engine.rs`). No system Python or Xcode
tools are needed.

The Browser is the system's own web engine (WebKit), embedded in the PWR window
as a native web view: no extra bundle, process or app. Cookies and site data are
kept by WebKit in the app's data store, protected by macOS.

## Building

From a checkout (Rust 1.88+, Node.js with npm, `python3`, `uv`):

```bash
sh scripts/setup-mlx.sh
```

```bash
cargo build --release
```

```bash
cd apps/desktop && npm install && npx tauri build --bundles app
```

`npx tauri dev` runs the app from the checkout (`PWR_CORE` names the core
binary, `PWR_WORKSPACE` the workspace).

The release DMG: `sh scripts/release-macos.sh`. It checks it runs on Apple
silicon with the tools it needs, checks the version is the same in
`package.json`, `package-lock.json`, `tauri.conf.json` and both `Cargo.toml`s,
cleans the Rust outputs, `npm ci`, builds the DMG with Tauri, verifies it
(`hdiutil verify`, mounts it, checks the bundle's version and executable) and
writes `dist/release/`. It publishes nothing. Tauri uses the versioned PWR
background at `apps/desktop/src-tauri/icons/dmg-background.png` and places the
app and Applications shortcut side by side. To revise the background, edit
`scripts/generate-dmg-background.swift` and run it from the repository root.

The workflow `.github/workflows/release-macos.yml`, on a `v*` tag, first reuses
the whole CI workflow (`checks`), and only then runs that script, taking the
version from the tag and the notes from `docs/release/<tag>-release-notes.md`,
and creates a draft prerelease. Its execution must be verified for the
selected candidate; historical CI is not new artifact evidence.

## Reproducibility and remaining qualification

The release script refuses tracked/untracked source changes before cleaning or
building. A hosted tag must resolve to HEAD. Before checksums are emitted it
requires the same clean commit/tree again. It records tool versions, source
identity and artifact sizes/SHA256 in `build-provenance.json`, included in the
checksum set and draft workflow attachments. This is an unsigned local build
record, not a cryptographic attestation, hermetic build or proof that gates ran.
Ignored build outputs are excluded from Git cleanliness checks. No current
uncommitted development tree is eligible for release packaging.

Cargo and npm dependencies are locked. The [managed engine](release/managed-engine.md)
now pins Python 3.14.8 and 76 hash-locked Python packages in native setup,
the shell helper and sidecar CI configuration. Actual native cancel/retry and
pinned offline encoder execution pass on the development artifact. Other
Python overrides, Python distribution provenance and older hosts remain outside
that qualification; hosted CI has not run on the current changes.

[Bundled notices](release/third-party-notices.md) contain 426 Cargo records and
712 texts, with 23 unresolved review items. They do not establish legal
clearance or full engine/model attribution. Apple signing/notarization,
selected-candidate hosted checks and operational updater qualification remain
open. See the [release checklist](release/next-release-readiness.md).

## Branches

Work lands on `develop`; a release goes `develop` → `stage` → `main` and is
tagged. CI runs on all three.
