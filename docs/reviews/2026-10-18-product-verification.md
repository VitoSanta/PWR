# Product verification campaign — 2026-10-18

Status: IN PROGRESS. Implements phase 0 and initial integrity checks of the [adopted plan](../plan/piano-campagne-gemma4-e-windows.md). No release or model-quality claim is established.

## Candidate identity

PWR HEAD: `b9660013573bb45409d9b536c59ad8895701d626`, with preexisting tracked and untracked changes. HEAD alone does not identify the tested source. A private working-tree patch and file-hash inventory identify the initial snapshot. Documentation added after that snapshot is recorded separately; a clean release candidate still needs freezing.

Private evidence ID: `product-verification-20261018`, under the established Gemma audit store. It contains repository status, patches, file inventories, registry snapshot, environment and command logs. Evidence is unpublished and is not linked to a machine-specific path. Existing sealed runs are unchanged.

## Phase 0

- PWR and the nested benchmark repository located; both have local modifications. The benchmark repository also has many preexisting deleted tracked files. These are preserved as found; historical evidence cannot be assumed complete.
- No model inference or held-out task access initiated.
- Artifact registry copied with revisions and declared file checksums. Local integrity, actual context window and quality remain unverified.
- G1 and G2 require candidate evidence; G3/F6 are not passed. Windows sequencing and Gemma-only/F6 conflicts are recorded in the [decision log](../decisions.md#reconciliation-opened-2026-10-18--product-completion-campaign).
- Product runner / W8.3 validation remains pending. Offline runner tests establish infrastructure behavior, not model capability.

## Initial integrity checks

Results will be recorded with command exit codes and evidence IDs. Hosted CI, native Windows, native app walkthrough, packaging and live model tests are `not run` for this candidate.

## Registry and platform inventory

The registry contains **8 base agent packs and 5 speculative assistants**. All 13 expected local directories exist. Small-file checksums and file sizes were checked; complete base-pack checksum verification is being recorded separately. Presence and integrity do not establish loading or agent quality. Configured context is not a measurement of effective context.

The desktop engine selects MLX on macOS and `llama` otherwise (`apps/desktop/src-tauri/src/engine.rs`). The terminal contains Windows-specific branches. This is static inventory only: Windows build, WebView2, process cancellation, sandbox and installer remain `not run` on Windows.

## Remaining inspection scope

The [historical hard inspection](2026-10-16-hard-inspection.md) is input to this campaign, not a fresh closure of HI-01–HI-37. Native browser/frame and lifecycle checks, PTY saturation/backpressure, DOM progress, HI-28 outcome attribution and HI-34 metadata-cache limitations remain open for candidate reconciliation. No critical finding is classified as accepted risk by this record.

## Phase 0 integrity result

All registered files of the **8 base agent packs** match both declared byte size and SHA-256 (private evidence `base-pack-checksums.json`). This checks current local files against the frozen registry; no inference or quality promotion follows. The 5 assistants have only presence, size and small-file checksum checks in this phase; complete weight hashing and decoding parity remain pending.

The existing local debug/release executables and source sidecar/runner were hashed. Their correspondence to a rebuilt working-tree candidate is **unknown**; they must not be used as candidate capability evidence until build provenance is established.

Static Windows observations: terminal closure uses `taskkill /T /F`; a launch requiring a sandbox is refused outside macOS. These branches require native validation. Current CI defines macOS desktop, Rust and sidecar jobs, with no Windows job. Therefore the multiplatform CI requirement is open.

## Preflight disposition

Phase 0 has produced an identified development snapshot, registry inventory, complete base-pack integrity checks and environment/executor identities. It is **not a release freeze**: the working trees are dirty, executables are not yet attributed to the snapshot, and W8.3 minimal-control equivalence still requires validation. Confirmatory inference remains blocked by these prerequisites and unresolved G1/G2/F6 evidence, not by an assumed model failure. Development integrity checks continue independently.

No release version, tag, model default, permissions, gate or persistent lessons were changed. The next work is candidate integrity/HI reconciliation and deterministic protocol diagnosis; native Windows qualification and publication require their own evidence.

## Completed local checks

| Check | Result | Limits |
|---|---|---|
| Public documentation | pass | 127 Markdown files; local links and personal-material checks |
| Generated roadmap | pass | Regeneration produces identical content |
| Rust formatting | pass | `cargo fmt --all -- --check` |
| Clippy | pass | Workspace/all-targets, warnings denied |
| Browser bridge | pass | Node tests; no WebKit/native proof |
| Product-path runner infrastructure | pass | 24 offline tests; stand-in core, no model |
| MLX sidecar | pass with skip | 85 discovered, 1 skipped: historical trace absent; first import failure retained, corrected with CI PYTHONPATH |
| Rust workspace | pass with unexercised cases | 1588 reported passed, 0 failed, 6 ignored; Docker-dependent case skips because no engine socket (included in reported successes) |

Counts do not imply native application coverage, live-model quality, Windows qualification or release readiness. Test logs and explicit skips are retained in private campaign evidence.

## Dependency remediation — revised development candidate

The initial clean npm install exposed 8 audit findings (2 critical, 4 high, 2 low). These are dependency advisories, not evidence of an exploited PWR boundary. The first compatible audit-fix attempt left 5 findings. Updating Angular dependencies together to 22.2.3 and DOMPurify to 3.4.17, with a scoped Monaco sanitizer override, resolves the audited tree without a Monaco downgrade or major Angular upgrade. The lockfile records the resulting dependency graph.

A first install attempt encountered a peer-resolution conflict; `npm update --ignore-scripts` resolved the aligned graph. The failed attempt and initial audit remain in the evidence store. A subsequent clean `npm ci` and `npm audit` both exit 0; the audit reports zero findings. Test/build results for this changed frontend candidate are recorded separately from the initial run.

Rust/Tauri source is unchanged by this dependency correction, so their completed source-level checks remain applicable within that scope. Frontend dependencies have changed: initial frontend results are baseline evidence only until repeated. Neither dependency audit nor unit tests replace native UI/security checks.

## Revised frontend verification result

After dependency remediation: clean `npm ci`, `npm audit` (zero findings), **151 frontend tests**, **5 browser-bridge tests** and production build all exit 0. Desktop native Rust tests: **8 passed** on unchanged native source. The frontend tests emit jsdom canvas warnings; these do not establish canvas/native rendering coverage. npm's install-script policy blocked scripts for packages outside its allowlist; build and tests still completed.

The initial bundle warning persists and increases from 1.16 MB to **1.17 MB** (169.21 kB above the 1 MB warning, below the 2 MB error threshold). Bundle performance remains open; the warning threshold was not relaxed.

The final source inventory and working-tree patch identify this revised development candidate separately from the initial snapshot. No commit, hosted CI run, installer or release is created by this campaign. Phase 0 inventory is recorded; release freeze and real executor/minimal-control preflight remain pending. Initial phase 0a local suites are complete within the scopes above.

Next: reconcile HI-01–HI-37 on the revised candidate; complete missing security/native/Windows contracts; build and attribute the product runner binary before protocol diagnosis. E2B/E4B reproduction, screening and confirmatory experiments remain `not run`.


## Second development candidate — macOS and controller repairs

Private evidence ID: `product-verification-20261018-r2`. Binary, sidecar, source-file hashes, dirty-tree patches, transcripts and negative attempts are retained separately from the first candidate. Neither snapshot is a clean release freeze. Earlier `not run` statements above describe the initial snapshot only; the following evidence supersedes them within its stated scope.

Implemented and regression-tested: editor confirmations bound to workspace/document version; PTY output acknowledged after xterm consumption; Unix file-index identity using device/inode/ctime as well as mtime/size; startup request deadlines with workspace recovery; macOS Cmd-Q routed through dirty-buffer confirmation; explicit browser navigation dismissing search selection and rejecting stale workspace results; bounded repeated permission/refusal loops ending without completion or unauthorized execution. Tauri JavaScript minor versions are aligned with the native dependencies after the dependency update exposed build incompatibility.

Native unsigned macOS application: build succeeds; synthetic workspace reaches ready; local browser form works; a page attempt to invoke `core_stop` is denied by ACL; terminal reaches its completion marker after 100,000 output characters; Cmd-Q cancellation preserves an unsaved buffer and explicit discard exits. These observations do not establish arbitrary output saturation, force-crash draft durability, active-turn recovery, signing, installation/upgrade or Windows coverage. Temporary workspace trust and remembered workspace were restored after the walk.

Verification logs distinguish candidates and negative attempts. Frontend: **159 tests pass**; bridge: **5 pass**; native Rust: **9 pass**. Initial second-candidate workspace: **1589 reported passes, 6 ignored**, with Docker-dependent coverage explicitly unexercised. A later workspace run exposed the new refusal regression and failed; after correction the CLI/orchestrator suites and workspace/all-target Clippy pass. A consolidated final workspace run remains necessary after the remaining repairs. Dependency audit reports zero findings; the 1.17 MB initial bundle warning remains open.

## Eight-pack product-path development diagnostic

Frozen synthetic `clamp-v2` task, one fresh trial per base pack, actual ACP product executor, requested 16,384-token window, thinking/assistant disabled, zero human nudges and a 180-second prompt watchdog. Only `clamp.py` may change. Independent external verification uses restored visible tests plus eight hidden cases and an inverted-interval exception. All eight final workspaces satisfy those checks and file scope; budgeted completion differs:

| Gemma 4 pack | Budgeted result | Wall seconds |
|---|---|---:|
| 26B A4B 4-bit | pass | 102.57 |
| E2B 4-bit | pass | 27.42 |
| E4B 4-bit | pass | 72.85 |
| 31B 8-bit | timeout; artifact checks pass | 181.39 |
| E4B QAT 4-bit | pass | 68.96 |
| 12B 4-bit | pass | 39.69 |
| 26B A4B QAT 4-bit | pass | 74.88 |
| 26B A4B 6-bit | pass | 120.41 |

Malformed tool calls and denied operations remain in raw transcripts. In particular, the 6-bit pack attempted to edit a protected test, was denied, and recovered; the file was unchanged. The 31B timeout includes cold loading and review overhead; it is not evidence that it cannot solve the task. Earlier preflight failures and an invalid fixture with undeclared Python are retained as infrastructure/fixture failures, not pack capability results.

These are development diagnostics on one small task, not a capability ranking, repeated-trial success estimate, long-task qualification, or confirmatory uplift. The optional sampling-inspection endpoint is unavailable, so effective sampling provenance remains unresolved. Minimal-control equivalence, numerical threshold approval, prospective power, clean freeze and the heldout campaign remain open. No reserved tasks were used.

## Current contract and remaining gates

[D-2026-10-18-2](../decisions.md#d-2026-10-18-2--gemma-4-product-scope-and-formal-f6-amendment) formally adopts Gemma 4-only product deployments; other families are external benchmarks. Windows is the final execution phase under D-2026-10-18-1. Neither owner decision passes G1/G2/G3/F6.

Next repairs include descriptor-safe site/download access (HI-01/HI-37), followed by consolidated candidate verification and same-candidate minimal-control diagnostics. Full native lifecycle, assistant decoding parity, long-context/task stress, licensing/packaging, hosted CI and native Windows evidence remain incomplete. No release, tag, push or publication was performed.


## Unix site-serving repair

The server now pins its root directory descriptor and opens request components relative to it with `NOFOLLOW`; build-preview selection opens its relative directory beneath a pinned workspace root. Symlinked build directories and index files are not selected. Responses stream from the opened regular file with a fixed-size copy buffer, initial descriptor length and a two-second socket write timeout instead of allocating the entire file.

Two new regressions fail before the repairs and pass afterward. Root replacement previously returned 403 (not a demonstrated disclosure); the repaired server keeps serving the original pinned directory. Linked build discovery previously selected a synthetic external directory and now rejects it. All eight site-module tests pass, including existing symlink/fallback checks and browser-driven local page checks. Unix descriptor protection is not a Windows qualification, hard-link confinement proof, concurrent admission limit or full HI-01 closure. The model-download partial-file race (HI-37) remains open. The earlier eight-pack run used the preserved pre-site-repair candidate and must not be attributed to this source revision.


## Consolidated post-repair source checks

Full workspace test run exits **0**: **1593 reported passed, 0 failed, 6 ignored**. The slow .NET multi-project sandbox test also passes. Workspace/all-target Clippy with warnings denied, formatting, diff whitespace, generated roadmap and public-document checks pass (127 Markdown files, 370 local links). Environment-dependent early-return cases are included in reported successes; this final run did not enable the separate skip log, so it does not establish Docker/browser coverage beyond the explicit earlier evidence.

Logs and source hashes are retained in private evidence. These source checks cover the site repair; the native bundle and model diagnostic binary predate it and have not been requalified for release. No gate is advanced.


## Model partial-download write race

A local HTTP regression reproduces the late `.part` symlink swap: the old writer followed the substituted link and reported success. The revised Unix writer pins the parent directory before waiting for HTTP, opens the partial with `NOFOLLOW` and validates the actual descriptor before truncation. Non-regular and multiply-linked files are refused; resumed length must still match. The regression exercises both late symlink and hard-link substitution and checks the external synthetic file remains byte-for-byte unchanged. No model downloads or remote service changes are needed for these tests.

This closes the tested write/truncate redirection, not every download integrity race. Hashing and final publication still use pathnames, so verification/publication identity, initial destination-root anchoring and concurrent same-user mutation require further work before full HI-37 closure. Windows keeps its existing path and remains unqualified.

The [in-app update proposal](../release/in-app-updates.md) records feasibility and release prerequisites. The feature is not enabled; no signing secrets, updater feed or release were created.


## Same-candidate minimal-arm development check

Private batch `diagnostics-minimal-v1` uses the exact earlier `candidate-controller` binary/sidecar and driver hash, same `clamp-v2` task, external checker, 16,384-token request, disabled thinking/assistant, permissions and watchdog as `diagnostics-v4`. Fresh workspaces and no human nudges. Both arms report `minimal`, with valid scope and passing independent checks: E2B 22.01 seconds; E4B 38.56 seconds. E2B recovers from a refused undeclared pytest command; both recover from an unavailable `python` executable. No extra grants are supplied.

This validates actual minimal-arm execution on these two deployments. The unrandomized single trials do not estimate uplift, establish statistical power, qualify long tasks or close W8.3 equivalence across deployments. Sampling provenance remains unknown for these preserved runs. Site/download source repairs are newer and are not retrospectively included in their binary identity.


## Generation-parameter provenance repair

The sidecar now records a `generation_parameters` event with the normalized sampler values actually passed to `make_sampler`, requested seed (null when absent), penalty settings, granted generation limit, requested context, thinking/reasoning budget and speculative/one-thought processor selection. Defaults and model behavior are unchanged. A regression first fails because the event is absent, then passes while asserting the recorded values equal those received by the sampler/processors. All 92 discovered sidecar tests complete successfully with one missing-historical-trace skip. This improves future traces; it does not fill sampling provenance retroactively for preserved diagnostics.

Model package tests pass after the partial-write correction; targeted all-target Clippy passes after fixing an unchecked read length in the new regression. Negative test and lint logs are retained. The full 1,593-pass workspace result predates these later model-package/sidecar changes and is not relabelled as a new run.


## Rebuilt artifact and live trace validation

The unsigned macOS application builds successfully with the site, initial partial-writer and generation-trace repairs. Its core/sidecar are preserved under `candidate-download-trace`, with source inventory and build log. A fresh E2B product-path development trial passes both external checks with valid scope in 23.01 seconds. Nine completed generations have nine parameter events; temperature is 1.0 except one internal call at 0.3, top-p 0.95, top-k 64, min-p 0.0, requested context 16,384 and no requested seed. This is measured per-call provenance, not a new default or capability claim. Later publication-identity repairs below are newer than this artifact.

Declared licence inventory: 637 distinct Rust package records across workspace/native metadata and 478 npm lock entries; none lacks declared licence metadata. This includes development and target-specific dependencies and is not a distribution/NOTICE clearance. The private inventory records the distinction.

## Assistant integrity and greedy parity development smoke

All registered files of all five speculative assistants pass full size and SHA-256 checks: 26 files total. This supersedes their earlier small-file-only integrity checks, not their quality status.

On two frozen short prompts, each of the five target/assistant pairs produces identical content in isolated greedy, seed-0 processes; all generations finish normally. E2B, E4B and 12B pass the full frozen format-plus-parity criterion. The 26B and 31B return the same `lambda` in both arms and fail the strict `def`-AST format checker; their original primary verdict remains negative. The batch stops at 26B under its rule; a documented amendment completes only the unrun 31B, without repeating or replacing the 26B result. Derived parity dimensions are stored separately. These are engine-level development smokes, not stochastic-distribution parity, long-task evidence, product-executor equivalence or permission to enable assistants by default.

## Download publication and identity repair

A second local HTTP regression demonstrates that an independently created destination was overwritten by the old final rename. Publication now uses atomic no-replacement creation; if the destination appears concurrently it is preserved and the download fails. A filesystem without hard-link publication support fails without replacing the destination and keeps the partial for diagnosis/retry.

Unix verification now opens a no-follow descriptor first and obtains length and hashes from that descriptor, avoiding the metadata/open race. Publication compares the resulting device/inode identity with the verified descriptor identity and refuses to report success after substitution. A regression replaces the partial after hashing and observes a verification error. Conflicting paths are preserved rather than deleting a potentially new owner's file.

Final targeted model package: **17 reported tests pass**; all-target Clippy and formatting pass. Full closure still requires destination-root anchoring across the complete operation, revision-marker handling and the declared concurrent-writer threat model; these repairs do not qualify Windows. The last full workspace and native artifact predate the newest publication repair, so they are not relabelled as its results.


## In-app updater implementation

Owner approval D-2026-10-18-3 adds Settings → Updates, opt-in startup checks, plain-text release notes, download progress, native-owned verified download bytes, explicit install/restart, hidden-workspace draft checks and native activity serialization. Release scripts generate signed archives/manifests only with complete build-time configuration; the existing workflow still creates drafts and never activates a live feed. See the [updater contract](../release/in-app-updates.md).

Local checks: complete frontend suite 166 passes; latest editor/updater targeted suite 19 passes; native suite 10 passes; native Clippy, frontend production build and 3 manifest syntax tests pass. Negative test/launch attempts are preserved. Native walk initially fails due to a missing required plugin configuration field, then passes after the explicit empty unconfigured field and regression are added. The final app shows the correct unconfigured state and quits normally. No production key, feed, signed installed-version upgrade, hosted CI or release publication is established. The initial bundle warning remains about 1.18 MB.


Final updater development candidate: **168 frontend tests**, **10 native tests**, **3 manifest tests**, native all-target Clippy and unsigned macOS app build pass. The additional frontend regression ensures two asynchronously resolved confirmations install/restart once. The native configuration regression confirms unconfigured startup remains valid without weakening TLS. Native Settings → Updates was opened and inspected successfully; production key/feed activation, actual signed upgrade and Windows remain unqualified.


## Third development candidate — complete Unix download operation

Private evidence ID: `product-verification-20261018-r3`. Two additional red regressions reproduce revision-marker symlink writes into an external synthetic target and publication outside a root substituted during HTTP. The revised storage layer pins the root and per-file parents for the entire operation, uses descriptor-relative operations through verification/publication/cleanup, and atomically replaces the marker entry without following it. Named-directory identity checks reject a moved destination rather than reporting a successful installed path.

A third regression demonstrates missing exclusive ownership; an advisory directory lock now serializes cooperating PWR downloads and releases when its owner closes. Additional tests reject nested symlink directories, destinations outside the declared root and parent traversal. The [storage contract](../plan/download-storage.md) documents tested effects, trusted-root assumptions and limits, including arbitrary noncooperating same-user mutation, power loss and Windows fallback. Previous statements that these specific root/marker cases were open describe the older candidates. This change does not certify G1 or Windows.


The consolidated third-candidate workspace run exits 0: **1,601 reported passed, 0 failed, 6 ignored**; workspace/all-target Clippy passes. The skip log explicitly records the Docker engine-socket case as unexercised. The slow .NET multi-project sandbox case passes. Formatting, whitespace and public-document checks pass. Neither these counts nor the scoped Unix regressions qualify Windows. A new optimized native bundle and real-process recovery probe are tracked separately in this campaign's private evidence.


## Third-candidate real-process recovery probes

The new optimized macOS bundle builds and its core/sidecar identities are frozen separately in private `product-verification-20261018-r3/candidate`. Runtime source hashes remain stable throughout the probes. One fresh E2B product-path synthetic repair passes independent checks and file scope before lifecycle testing; no heldout tasks or new model downloads are used.

After a completed turn, SIGKILL of the owned core is followed by exit of its captured MLX child within the 20-second observation bound, without manual intervention. A new core lists and replays the saved conversation, explicitly warns about an external `clamp.py` change, and leaves that change untouched. A deliberately corrupted copied database produces an explicit storage error, not an empty successful session list. The original probe remains a primary fail because its empty other workspace had no model configured: it stopped before the intended saved-session lookup. A documented isolation-only amendment configures the same installed model there; its list is empty and the original ID is rejected with `-32002` / no saved session. No original task is rerun for that amendment.

The first active-crash attempt also remains negative: the first ACP text chunk arrived after generation completed, so the intended active-generation precondition was not met; its completed answer correctly survived. This is an instrumentation limitation, not a demonstrated recovery defect. An amendment changes only the kill trigger to ten incremental non-prefill engine-progress notifications and confirms no finished-generation trace exists before the kill. That active-generation probe passes: the owned sidecar exits within about **0.34 seconds**, prior conversation/recovery notice are restored, the interrupted answer is not replayed as completed, and workspace files are unchanged before and after resume. All captured descendants are gone at the end.

These are actual ACP core-process observations on an identified development candidate, not native-window recovery or proof of power-loss durability. Tool-write crash windows, durable editor drafts, native application lifecycle, longer/resumed coding tasks, effective long context, hosted candidate CI, packaging/signing and confirmatory gates remain open. Windows remains the final phase. No gate, release version or publication is advanced by these results.


## Local signing identity and durable editor drafts

The owner authorized a new local updater signing pair (D-2026-10-18-4). Only its public trust anchor enters the repository. The encrypted private key and random signing password remain outside both repositories with restricted local permissions. A real Tauri CLI signature passes the same `minisign-verify` 0.2.5 verification used by the pinned plugin; altered payload and malformed signature rejection pass. This does not qualify a hosted updater feed, server download, installed-version upgrade, Apple signing/notarization or independent key backup.

[Editor recovery](../plan/editor-recovery.md) now persists private workspace-isolated dirty buffers with bounded validated records and atomic replacement. Restoring does not modify workspace files and retains the original save hash. Latest complete frontend run: **176 passes**; after the final closing-callback guard, all **20 editor regressions** pass. Native tests: **12 passes**; native all-target Clippy passes. The initial bundle-size warning is unchanged.

The first native walk confirms real unsaved text survives SIGKILL and reappears exactly in the restarted macOS editor with a recovery notice. A later external change causes checked save to refuse the write and leaves the external file and draft intact. Its explicit-exit subcheck is retained as negative: polling can replace conflict-status objects while the confirmation is open, invalidating an object-identity approval despite unchanged text. The correction binds approval to workspace/path/text/base/hash and ignores conflict-status-only refreshes; its regression passes. A new app bundle and native rerun are recorded separately, without relabelling this first result.


The corrected macOS bundle passes a second real crash/restart walk with its own executable digest and source inventory. A new marker is persisted, SIGKILL is applied only to the owned test application, and the exact marker reappears in the native editor. After conflict polling, explicit discard exits normally and removes the recovery record. A further launch shows no open recovered file or recovery notice; clean quit succeeds. The synthetic workspace's external file remains unchanged. Previous workspace/trust state is restored byte for byte. The first exit failure remains separate evidence, not a rerun labelled as its original result.

The updater also passes an **actual HTTPS/plugin install probe** in an isolated app copy: the pinned Tauri 2.11.6/updater 2.10.1 checks a local manifest, downloads a malformed-signature case and an altered-archive case and rejects both before installation. It then downloads/verifies a genuine Tauri-signed app archive and installs the isolated copy; the resulting executable hash equals the current candidate and the fixture marker/plist version are present. This uses a mock Tauri application handle and a temporary local certificate authority trusted only by the probe client, with no TLS bypass or operating-system trust changes. It does not exercise the product's UI update/restart sequence, a hosted feed, Apple signing/notarization, admin authorization, power loss or Windows. The fixture plist version is not a product version change. Two harness precondition failures (missing mock plugin config and a symlinked launch path) and their declared corrections are preserved separately.

No Developer ID Application identity is available in the current local keychain (read-only inventory, zero matching identities). Apple distribution qualification therefore remains open; the Tauri key is a separate artifact-signing identity. No gate, hosted CI, push, tag or release is advanced by these scoped results.


## Recovery boundaries and resumed coding development screens

Final editor boundary changes wait for recovery before allowing a model write and block updater/restart while the draft set is still unknown. An unreadable recovery record remains byte-preserved on clean quit/discard rather than trapping the user in the app. **179 frontend tests** pass, including pending recovery, serialized write failures and unreadable-record handling. The optimized unsigned macOS app rebuilds successfully. An additional native corrupt-record walk shows the explicit retained-record error and exits normally; the exact malformed record and all prior app trust/workspace contents are preserved/restored. The earlier crash/CAS/discard walk covers the immediately preceding desktop artifact; the new boundary cases have their own source and executable identities. Native code is unchanged from its 12-test/Clippy pass.

Two new synthetic development coding screens use frozen two-module integer-cent ledger specifications and tests, 16,384 requested context, no thinking/assistant, standard sandbox, no permission escapes, zero human nudges and a predeclared 300-second budget per turn. No heldout or reserved tasks are used. PWR runtime source hashes remain unchanged throughout each screen, and engine processes are gone before independent verification or source work resumes.

- **12B 4-bit:** first turn hits the budget at about 300.63 seconds, with only two reads and no executed write. The partial raw `write_file` argument contains repetitive explanatory text. Independent frozen tests fail; the core reports interrupted/not delivered/not verified, and the second phase is not started under the stopping rule. The controller's overall two-phase status remains `inconclusive`, while the first task is a clear budget/test failure. No retry or sampling/default change is hidden in the result.
- **26B 4-bit:** the first implementation passes the frozen tests and file scope in about 152.92 seconds. The core is closed; a predeclared external zero-return regression is appended, the same saved conversation is loaded by a fresh core, the external change is reported, and repair passes the same tests/scope in about 153.90 seconds. The protected overwrite guard refuses an initial unread-file write before the model reads and repairs it. These are actual lifecycle and declared-check observations, not a long-context or all-pack quality claim.

The 26B code also demonstrates why declared-check success is insufficient for semantic qualification: its `\d` regex accepts Arabic-Indic/fullwidth digits although the original specification explicitly requires ASCII. The original frozen tests omit this boundary. A clearly labelled exploratory audit, added only after inspecting the result, reproduces acceptance of three invalid Unicode inputs in both the first and final artifacts. The original test-pass results remain intact; the semantic failure is recorded alongside them. It is not an amended confirmatory endpoint or evidence of universal model incapacity. The current review prompt did contain the ASCII requirement, so this observation is not attributed to missing context. Product semantic verification/quality remains open; no G1/G3/F6 or release claim is advanced.

Remaining non-Windows work includes the complete current-candidate security/native matrix, broader long/context/model qualification, explicit semantic-verification gaps, eligible same-deployment competitors and prospective owner-approved F6 thresholds, clean-candidate hosted CI, distribution notices/environment reproducibility, Apple signing/notarization and an operational update/restart/rollback release exercise. The owner must supply the applicable Apple identity/account setup; the local keychain contains no Developer ID Application identity. Windows remains the last phase, not the only remaining qualification.


## Fourth development candidate — bundled attribution material

Private evidence ID: `product-verification-20261018-r4`. Artifact inspection reproduces a packaging omission: Angular's generated `3rdpartylicenses.txt` remains outside the embedded frontend directory, and the native Resources directory contains no attribution set or uv license texts.

The native build now assembles and bundles a [hashed attribution inventory](../release/third-party-notices.md): PWR/frontend/font texts, available texts from the locked target's conservative non-dev Cargo closure (including build dependencies), and uv's own license texts. There are **426 Cargo records**, **712 text files**, and **23 explicitly unresolved review items**. Local package paths are absent from the published inventory. This is not a distribution clearance or complete uv/Python/model inventory.

Repeated generation has identical hashes; removing a required synthetic/generated input rejects the build step and preserves the prior complete output. Five verifier regressions reject altered, missing, extra, traversing, linked, duplicate or wrong-target records and mismatched uv bytes. The release script verifies the notices inside the DMG-mounted app. The actual unsigned macOS app rebuilds and passes attribution verification on its Resources directory (about 5.4 MiB of texts); 12 native tests pass after the resource configuration change. Candidate executable/core identities and logs are recorded separately from r3; no earlier UI or model run is relabelled onto this new artifact.

Upstream-source investigation is pinned to each missing crate's packaged VCS revision and retained privately. The initial Python HTTPS client fails certificate-chain validation; system curl succeeds with normal TLS verification. No certificate validation or operating-system trust is weakened. A repository-root licensing explanation is not silently substituted for a missing license text. The remaining attribution review items, Apple identity, operational updater qualification, semantic coding gaps and other documented gates remain open. Windows is not exercised.


## Fourth candidate — managed installation and bounded encoder

The [managed environment contract](../release/managed-engine.md) records the
76-package hash lock, transactional native installer and pinned encoder. No
package versions or model sampling defaults change. Both setup paths and the
sidecar CI configuration consume the lock; hosted CI is still pending. The
initial default macOS-13 wheel resolution fails and remains recorded; explicit
macOS-14 resolution succeeds, without claiming older-host qualification.

A fresh private environment passes dependency/import checks; a deliberately
wrong distribution hash is rejected. Native rollback regressions cover ordinary
failure, cancellation, unwind ordering and retry after partial restoration.
Two real native setup walks have separate candidate identities. The first
exposes an unbounded 43-file encoder snapshot (2,280,201,964 payload bytes).
The final artifact requests seven revision-pinned files (493,292,184 bytes).
Its actual phase-two cancellation restores all 14,198 environment entries and
the old marker exactly, leaves no step children and removes its transaction
backup. Retry completes all four steps; the new marker matches both compiled
hashes, Continue starts the core, and normal application exit succeeds. Prior
workspace/trust settings are restored exactly; the valid new environment stays
installed. Private independent pre-test backups remain retained.

The final bundle's embedding sidecar is exercised with its managed Python and
a cold-prepared isolated seven-file cache under OS-enforced network denial.
Ready arrives in 1.975 seconds. Three vectors have 384 dimensions, finite unit
norms and zero difference between duplicate inputs. EOF exits zero; bundled
resources remain byte-identical and no engine remains. This is a numerical
execution/integrity check, not semantic retrieval or coding qualification.

Latest checks: 18 native passes, native all-target Clippy, 98 sidecar tests
with one explicit skip, one targeted Rust embedding IPC regression, optimized
unsigned macOS app build and actual bundled-notices verification. The previous
179-test frontend result is not relabelled as a new run. The final CI changes
are configuration only and have not run remotely. SIGKILL/power-loss installer
recovery, Apple signing/notarization, operational updater UI/restart/rollback,
23 attribution review items, semantic coding gaps and the remaining quality
gates remain open. Windows is still deferred until the end.


## Release source identity preflight

The local packaging script now refuses dirty tracked/untracked sources before
cleaning build outputs. It checks a hosted tag against HEAD and checks the same
clean commit/tree again before completing checksums. `build-provenance.json`
records source identity, tool versions and artifact hashes/sizes and is attached
by the draft workflow. It is an unsigned local record, not a signed attestation
or proof that quality gates passed. Five synthetic Git-repository regressions
cover clean capture, tracked/untracked dirt, mismatched/changed commit, dirty
build completion and invalid/substituted artifact paths. All 13 release-tool
regressions pass locally; CI is configured to run them. No packaging, clean,
commit, push, tag or release is performed on the dirty development checkout.

Harness² status clarification: its methodology is documented in the adopted
plan, but no PWR Harness² pilot result is recorded. Existing synthetic coding
screens are not treated as PPC or compute-matched evidence. That evaluation
remains open.


## Harness² feasibility follow-up — 2026-10-19

The previously pending [Harness² pilot](2026-10-19-harness2-pilot.md) has now
run using the pinned unmodified upstream recursion scheduler and a private
PWR ACP adapter. The scripted upstream demo passes two tasks. Three completed
source-stable comparisons have no complete independently checked success in
B/C/D; two compositions contain explicit editor failures misclassified by the
adapter as unchanged guidance. The E4B final artifacts pass declared checks
and report verified, yet fail predeclared semantic audits (integer precision
and bool validation). The partial E2B transfer stops on an out-of-surface
Python proposal, which is never harvested. The 26B parser cell is excluded
because UI sources changed during inference; the second task is not started.
The pinned private runtime itself remains unchanged and engines exit.

The finding is no demonstrated benefit and inconclusive method efficacy, not
a universal negative about Harness² or Gemma. A five-regression offline
follow-up validates structured editor failure rejection without rewriting raw
results. The owner chooses to finish this report and defer a fresh 26B run
until UI work is complete. No new inference or promotion is authorized by
this report; the existing heldout, gate and release restrictions remain.
