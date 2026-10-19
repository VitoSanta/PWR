# Next macOS alpha release — preparation checklist

Latest public release: **v0.4.1-alpha, 2026-10-16**, source `92cfda2b`.
[Release verification](v0.4.1-alpha-verification.md): 111 frontend tests,
4 desktop Rust tests, 1,557 workspace tests passed (6 ignored);
[hosted checks and package build](https://github.com/VitoSanta/PWR/actions/runs/38825003454)
passed. Native terminal/preview upgrade checks were run. Fresh-machine install
and historical Review restoration remain outside that verification.

The following notes are historical and are not current release gates.

**v0.4.0-alpha was released on 2026-10-15** (tag on `b4f9e630`); what was
checked for it is recorded in its
[release notes](v0.4.0-alpha-release-notes.md). The checklist below is the
one kept since v0.3.0-alpha: carry it to the next release, re-run every check
on that release's candidate commit, and do not count passes recorded here as
evidence for it. The manual native-app list was not run for v0.4.0-alpha.

## Current verification campaign

The [2026-10-19 S00 baseline](../reviews/2026-10-19-S00-baseline.md) validates the
updated frontend and current development sources locally: 182 frontend tests,
1,601 reported Rust passes (6 ignored and an explicit Docker skip), 18 native
tests, sidecar/runner/release checks and Clippy/build pass. Native visual
qualification of the final frontend, clean freeze and hosted CI remain open.
Apple membership/signing is explicitly deferred until after Windows.

The [weekly 0.5.0 sprint plan](../plan/sprint-v0.5.0.md) places development and
research before final macOS qualification, then Windows qualification. Version
and release-channel selection remain prospective. Owner update 2026-10-19: Apple membership and signing/notarization are deferred
until after Windows (S12), when available. Earlier macOS walks qualify ad-hoc
packages only; the signed package will require a separate install/upgrade smoke.
No release or milestone is approved by the schedule.

The [2026-10-18 campaign record](../reviews/2026-10-18-product-verification.md) tracks the adopted completion plan, current development-tree checks and dependency remediation. It does not select or freeze a release candidate. Historical checked boxes below remain historical; hosted CI, native qualification and package checks must be repeated for the selected release.

The [in-app updater](in-app-updates.md) is implemented locally; production signing/feed setup and an actual installed-version upgrade remain open.

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
- [ ] Record candidate commit and artifact hashes together. The script now requires the same clean source identity before/after build
  and emits `build-provenance.json`; review this unsigned local record
  alongside the actual candidate checks.
- [ ] Complete or explicitly document the third-party licence inventory,
  including dependencies and bundled uv. Apache-2.0 for PWR does not establish
  the licences of model weights or all bundled components.
- [ ] Verify the managed hash lock and Python provenance on the selected candidate.
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
| Engine transitive lock | Implemented locally; selected-candidate/hosted qualification pending |
| Publication | Not requested |


Local continuation on 2026-10-18 adds private editor recovery and an owner-authorized updater public trust anchor. See the [editor recovery contract](../plan/editor-recovery.md) and [campaign record](../reviews/2026-10-18-product-verification.md). These scoped changes do not close the signed installed-upgrade, distribution, long-task or confirmatory requirements above. Windows remains last.


The resumed ledger development screen adds a concrete semantic qualification gap: 26B passes its frozen visible checks and resumes/repairs, but a separately labelled exploratory Unicode audit violates the requested ASCII-only rule. The 12B first turn also fails its 300-second task budget. Preserve both results and do not infer all-pack coding readiness from earlier clamp or unit suites. Latest desktop-only boundaries pass 179 frontend tests and a new unsigned native build/corrupt-record walk; these remain scoped development evidence, not a clean release candidate.


The fourth development artifact now includes verified [third-party attribution material](third-party-notices.md) rather than leaving Angular's generated file outside the bundle. Its 426-record/712-text inventory retains 23 explicit review items and is not legal clearance. Five integrity regressions, 12 native tests, the optimized unsigned app build and verification of the actual bundled notices pass. Signing/notarization, distribution review and the other release gates remain open.


The [managed engine](managed-engine.md) now uses a 76-package hash lock,
transactional native cancellation/rollback and a seven-file pinned offline
encoder. Actual final-artifact cancel/retry, core startup and network-denied
numerical embedding checks pass. Native tests (18), sidecar tests (98 with one
skip), Clippy and the unsigned app build pass. CI configuration consumes the
same lock but remains unexecuted remotely. These results narrow W9.2 without
closing the semantic, Apple distribution, updater operational or other gates.


The [Harness² feasibility pilot](../reviews/2026-10-19-harness2-pilot.md) has
not demonstrated a qualified coding benefit. E4B results expose concrete
semantic gaps despite declared checks passing; optimizer editor/context
failures also limit interpretation. The 26B comparison is excluded for UI
source changes during inference. A fresh run is deferred by the owner until
UI work is finished; this research does not close a release or quality gate.
