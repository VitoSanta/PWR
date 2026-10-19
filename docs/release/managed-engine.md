# Managed MLX environment

The macOS arm64 installer pins Python 3.14.8 and installs the 76 packages in
`apps/desktop/src-tauri/engine/requirements-macos-arm64-py314.lock` with hashes,
binary distributions only and no dependency re-resolution. The four direct
versions remain MLX 0.32.3, mlx-lm 0.32.0, mlx-embeddings 0.1.0 and mlx-vlm
0.7.8. The lock preserves the observed development environment's versions.
The app, setup script and sidecar CI configuration consume this same lock;
hosted CI has not run on this uncommitted candidate.

Resolution used macOS 14 as its minimum wheel target. uv otherwise defaults
to macOS 13 for this platform; see its [CLI reference](https://docs.astral.sh/uv/reference/cli/).
This is a resolver setting, not qualification on macOS 14 hardware. The actual
native walks ran on macOS 27.0.1. Python distribution provenance and broader
platform qualification remain separate requirements.

## Installation and recovery

A managed-ready marker must match both the compiled requirements hash and the
encoder manifest hash. Older markers offer setup again. An explicit external
Python override is not covered by this managed-install qualification.

Installation retains the previous environment and marker in a private backup,
creates the replacement at its final prefix and restores the originals on
ordinary failure, cancellation or unwinding. It does not relocate Python
virtual environments. Unix cancellation stops and reaps the step's process
group before restoring files. Installation shares the native activity guard
with core startup and updating. Successful installation removes its internal
backup; restoration/cleanup failures retain recovery material and report errors.

SIGKILL, power loss and automatic startup recovery of an abandoned transaction
are not qualified. The local setup shell script uses the lock but does not
provide the native installer's transaction contract. Windows remains untested.

## Encoder assets and offline execution

`crates/pwr-mlx/sidecar/encoder.json` pins multilingual-e5-small revision
`614241f622f53c4eeff9890bdc4f31cfecc418b3` and seven required files, each with
size and SHA256. Their total payload is 493,292,184 bytes. Explicit setup may
download these files; runtime resolves only cached assets and checks their
integrity. Existing unrelated cache files are left intact.

The embedding sidecar forces Hugging Face and Transformers offline before
imports and disables bytecode writes in the signed bundle. The default model
cannot be replaced by a same-named directory in the working directory.
Logical E5 query/passage prefixes are preserved. Cache identity includes the
model revision, pooling, length and prefix settings so previous vectors cannot
silently satisfy a different encoder configuration.

## Observed development checks

Private evidence: `product-verification-20261018-r4/engine-lock`.

- A fresh isolated environment installs all 76 locked versions; dependency
  checks/imports pass. A deliberately incorrect hash is rejected.
- Two separately identified native artifacts undergo install/cancel/retry.
  On the final artifact, cancellation restores all 14,198 prior environment
  entries and the marker exactly; retry completes, core startup and clean
  application exit succeed. Prior workspace/trust settings are restored.
- A cold isolated cache downloads only the seven pinned encoder files.
- The final bundled embedding sidecar runs under operating-system network
  denial: ready in 1.975 seconds, three finite normalized 384-dimensional
  vectors, identical duplicate vectors and clean EOF exit. Bundle resources
  remain unchanged and no engine process remains.
- 18 native tests and native Clippy pass; 98 sidecar tests complete with one
  explicit skip; the targeted Rust embedding IPC deadline regression passes.

These checks do not establish retrieval quality, coding quality, older macOS
support, signed distribution or Windows compatibility. Earlier failures and
artifact identities remain in the [campaign record](../reviews/2026-10-18-product-verification.md).
