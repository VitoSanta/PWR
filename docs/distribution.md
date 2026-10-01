# Building and distributing

**Checked against `develop` at `bff93062`, 2026-10-01.**

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

The workflow `.github/workflows/release-macos.yml`, on a `v*` tag, first reuses
the whole CI workflow (`checks`), and only then runs that script, taking the
version from the tag and the notes from `docs/release/<tag>-release-notes.md`,
and creates a draft prerelease. It has not run since it was changed.

## Reproducibility gaps

| Gap | Evidence | Plan |
|---|---|---|
| The release workflow ran no tests; nothing tied the artifact to a commit that passed CI | now `needs: checks` | W9.1 — implemented, never run |
| The engine installs its four direct pins (`mlx==0.32.0`, `mlx-lm==0.31.3`, `mlx-embeddings==0.1.0`, `mlx-vlm==0.6.17`) with no lock of their transitive dependencies and no hashes | `engine.rs`, `scripts/setup-mlx.sh` | W9.2 |
| **Newer engine releases exist**: `mlx` 0.32.3 (2026-09-29) and `mlx-lm` 0.32.0 (2026-10-01) on PyPI. 0.32.1–0.32.3 carry attention and quantized-matmul fixes (a GQA decode kernel's batch offset, state corruption when a primitive throws during eval, a quantized matmul corruption when the quantized dimension is not a multiple of 32). The pinned `mlx` 0.32.0 / `mlx-lm` 0.31.3 are the versions a reported silent KV-cache corruption near 60k tokens names (jundot/omlx#3777). **Not upgraded**: each bump needs a calibration of the installed models and a long-conversation check first | [experiment-log.md](experiment-log.md) | W9.2 |
| Python is `3.11`, not a patch release | `engine.rs` | W9.2 |
| CI's sidecar job installs only `mlx` and `mlx-lm` | `ci.yml` | W9.2 |
| No licence or supply-chain inventory ships with the release | — | W9.3 |
| Not notarized | — | W9.4 |

Cargo and npm dependencies are locked (`Cargo.lock`, `apps/desktop/package-lock.json`).

## Branches

Work lands on `develop`; a release goes `develop` → `stage` → `main` and is
tagged. CI runs on all three.
