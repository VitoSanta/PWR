# Building and distributing

**Checked against `develop` at `0776ff4f`, 2026-09-30.**

## What ships

A DMG for Apple-silicon Macs (`PWR-macOS-arm64.dmg`) with `SHA256SUMS.txt`,
published as a GitHub prerelease. The app is **ad-hoc signed, not notarized**:
macOS may block the first open, and the README gives Apple's documented
*Open Anyway* procedure. Latest: **v0.2.0-alpha** (tag `v0.2.0-alpha`,
2026-09-28); release notes in [release/](release/).

On first launch the app installs its engine: a private Python environment with
the pinned packages, about 1.2 GB, in
`~/Library/Application Support/ai.pwr.desktop/engine`, using the `uv` bundled
in the app (`apps/desktop/src-tauri/src/engine.rs`). No system Python or Xcode
tools are needed.

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
writes `dist/release/`. It publishes nothing.

The workflow `.github/workflows/release-macos.yml`, on the tag, runs that
script and creates a draft prerelease.

## Reproducibility gaps

| Gap | Evidence | Plan |
|---|---|---|
| The release workflow runs no tests; nothing ties the artifact to a commit that passed CI | `release-macos.yml`, `scripts/release-macos.sh` | W9.1 |
| The workflow hard-codes the tag `v0.2.0-alpha` and its notes file | `release-macos.yml` | W9.1 |
| The engine installs its four direct pins (`mlx==0.32.0`, `mlx-lm==0.31.3`, `mlx-embeddings==0.1.0`, `mlx-vlm==0.6.17`) with no lock of their transitive dependencies and no hashes | `engine.rs:20`, `scripts/setup-mlx.sh:21` | W9.2 |
| Python is `3.11`, not a patch release | `engine.rs:19` | W9.2 |
| CI's sidecar job installs only `mlx` and `mlx-lm` | `ci.yml` | W9.2 |
| No licence or supply-chain inventory ships with the release | — | W9.3 |
| Not notarized | — | W9.4 |

Cargo and npm dependencies are locked (`Cargo.lock`, `apps/desktop/package-lock.json`).

## Branches

Work lands on `develop`; a release goes `develop` → `stage` → `main` and is
tagged. CI runs on all three.
