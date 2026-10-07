# Next macOS alpha release — preparation checklist

**Source review: 2026-10-07, `develop` at `90fe0dd4`. Status: preparation,
not ready to tag.** No new version, candidate commit, DMG or release is
designated by this document. The three uncommitted interface files present
during the review must be reviewed together with subsequent changes before
freezing a candidate. Historical suite passes are not candidate evidence.

## Public repository

- [x] Separate personal editorial documents and session handoffs from the
  public checkout; retain their originals outside the repository.
- [x] Keep public technical findings and negative results, with personal
  paths/profile details removed and unpublished evidence labelled.
- [x] Add a public product overview with release/development distinctions.
- [x] Correct stale current claims about compaction, engine upgrade detection,
  model profiles and desktop diagnostics.
- [x] Add a local/CI public-documentation check for links and personal material.
- [ ] Review a clean candidate, including all current interface changes.
- [ ] Review replacement screenshots of the candidate. The retained v0.1.x
  screenshots are historical and do not demonstrate the next interface.

## Candidate identity and checks

Record the exact commit, clean-tree status, tool versions and every exit code.
Do not promote a historical audit's counts into a current test result.

- [x] Choose the release version and update Cargo workspace/Tauri manifests,
  both Cargo locks, npm manifest/lock and Tauri configuration consistently:
  `0.3.0-alpha`.
- [x] Prepare `docs/release/<tag>-release-notes.md` for that selected version:
  [v0.3.0-alpha](v0.3.0-alpha-release-notes.md), a candidate text.
- [ ] Run formatting, Clippy, the full Rust suite and explicit skip reporting.
- [ ] Run a clean `npm ci`, desktop unit tests and production build.
- [ ] Run sidecar and stack-matrix runner tests without a live model.
- [ ] Run `python3 scripts/check-public-docs.py` and verify the generated
  roadmap matches `docs/milestones.json`.
- [ ] Obtain hosted macOS CI for the same candidate commit; identify skips
  and investigate failures rather than counting skipped cases as exercised.

Recorded 2026-10-07 on `14f532ae`, the commit before the version change (the
version commit changes manifests, locks and release notes only): full Rust
suite 1,585 passed, 0 failed, 5 ignored in 107 binaries; hosted macOS CI green
on attempt 2 after one sandbox test failed on attempt 1 and passed unchanged.
On the version commit: formatting, the public-docs check, the roadmap check, a
workspace build, a clean `npm ci`, 109 desktop tests and the production build.
Also on the version commit (`fb59195a`): Clippy clean, the sidecar's 87 tests,
and `scripts/release-macos.sh` built and verified `PWR-macOS-arm64.dmg`
(SHA-256 `16f683382dcc8a06e1295fa052fc5f98012b7bb5723fcc0d6a2141b532d865c3`,
a local build; the published artifact is built by the tag workflow and has
its own checksum). The stack-matrix runner tests were not run. The manual
native-app list below was exercised only in part on 2026-10-07: browser
steps, screenshot in chat, tool grid, Full access and a goal without declared
acceptance; fresh install, upgrade, Stop and session resume were not.

Commands and interpretation are in [testing](../testing.md) and
[CONTRIBUTING](../../CONTRIBUTING.md). No full candidate suite, hosted CI,
build or inference is recorded as passed by this preparation checklist.

## Manual native-app verification

- [ ] Fresh install: bundled core/uv, engine setup, model download and workspace trust.
- [ ] Upgrade: previous managed-engine pins trigger the installation offer;
  cancel/retry does not present an incomplete engine as ready.
- [ ] Model compatibility: calibration uses the candidate adapter/suite;
  text, reasoning and tool channels render correctly.
- [ ] Repository task: read/edit, conflict with a manual edit, diff and Revert.
- [ ] Verification: passing/failing baseline, nested project, absent/zero tests,
  frozen acceptance change, goal without declared acceptance.
- [ ] Permissions: Protected, Standard, Full access and a one-command escape;
  unconfined effects are disclosed.
- [ ] Stop: prefill, generation, command, permission wait and checks.
- [ ] Browser: text and vision paths, local-server form steps, option selection,
  scroll, screenshot in chat, failed step and refused framing.
- [ ] Long task: memory telemetry and repeated-repair notice. Record actual
  behaviour; do not claim a fix's effectiveness without a repeat measure.
- [ ] Metal interactivity: confirm the startup driver setting and exercise
  long-context text/image requests while the desktop/browser is active;
  verify failure diagnostics and fresh-engine recovery if a fault occurs.
- [ ] Desktop: terminal tabs, resize, fullscreen, narrow window, folding,
  streaming scroll, session resume and interrupted-turn recovery.

## Packaging and distribution

The release script builds and verifies a DMG but does not publish it. Run it
only after the candidate is frozen: it cleans Rust build outputs. Review its
preconditions and retain the exact output as release evidence.

- [ ] Build `PWR-macOS-arm64.dmg` from the selected candidate using
  `scripts/release-macos.sh` and the chosen `EXPECTED_VERSION`.
- [ ] Verify the app/core arm64 binaries, sidecar resources, bundled uv,
  signature, bundle version and `SHA256SUMS.txt`.
- [ ] Mount/install the built artifact and repeat essential native flows.
- [ ] Record candidate commit and artifact hashes together. The current
  script checks versions but does not enforce a clean Git tree or emit full
  build provenance; its version check alone is insufficient.
- [ ] Complete or explicitly document the third-party licence inventory,
  including dependencies and bundled uv. Apache-2.0 for PWR does not establish
  the licences of model weights or all bundled components.
- [ ] State direct engine pins and the remaining transitive-lock gap.
- [ ] Retain the ad-hoc signing/notarization disclosure unless the candidate
  demonstrably changes that distribution status.
- [ ] Review the draft prerelease generated by the tag workflow after CI;
  publication remains a separate action.

See [distribution](../distribution.md). This document does not authorise a
tag, push or release, and the preparation work does not run the packaging script.

## Evidence record for the selected candidate

Fill this when an actual candidate exists; keep machine-local paths in private
working notes and publish usable artifact/CI references or anonymised IDs.

| Field | Candidate evidence |
|---|---|
| Version / tag | Not selected |
| Commit / clean-tree status | Not frozen |
| Rust, desktop, sidecar and runner checks | Pending for the candidate |
| Skipped / ignored cases | Pending |
| Hosted CI | Pending for the candidate |
| Native walk and upgrade test | Pending |
| DMG and checksum | Not built |
| Licence inventory | Open |
| Engine transitive lock | Open |
| Publication | Not requested |
