# In-app updates — implementation and release contract

Status: implemented locally after owner approval on 2026-10-18; release activation and installed-version upgrade qualification are pending. Settings → Updates checks, displays plain-text release notes, downloads with progress, and installs only after explicit confirmation. Automatic startup checks are opt-in and installation is always manual. A local signing pair was subsequently created with owner approval; remote secrets and a live feed remain unconfigured.

Use the official [Tauri updater plugin](https://v2.tauri.app/plugin/updater/) rather than a custom executable downloader. Its signature verification is mandatory. Updater signatures and Apple Developer ID signing/notarization are separate release controls; record each explicitly.

## User flow

Offer “Check for updates” in the application controls, with current/available version, release notes, download progress and retryable errors. Check automatically only under a documented user preference; installation remains an explicit action. Never interrupt an active turn, terminal/service work or unsaved editor changes. Before installation, drain and stop owned processes through the normal lifecycle and preserve recoverable state. Restart is a separate confirmed action after successful installation.

Keep stable and prerelease channels separate. An alpha installation must not accidentally follow a stable-only feed or install an older version. Match platform and architecture. Missing, malformed, untrusted, wrong-platform and bad-signature responses must leave the installed app usable. Confine updater commands to the trusted main window; external preview pages receive no updater permissions.

## Release prerequisites

- Establish durable ownership, backup and recovery for the updater signing key. Keep private material out of the repository and logs; embed only its public key.
- Choose the release channel and HTTPS manifest endpoint. GitHub Releases can host a static manifest and platform archives; publication remains part of the separately authorized release process.
- Configure signed updater artifact generation in CI. Publish the manifest only after the referenced immutable artifacts and signatures exist. Do not enable a feed pointing to placeholders.
- Qualify Apple signing/notarization and first-install behavior separately from updater package signing.
- Pin compatible plugin dependencies, implement the state machine and restrict capabilities. Test manifest/signature failures, cancelled downloads, interrupted installation, dirty buffers, active inference and process cleanup.
- Exercise an actual installed previous build upgrading to a newer signed build, including version/resources/core/sidecar correspondence and retained user state. Source/unit checks do not establish this behavior.

macOS update qualification is still required before claiming this feature works. Windows updater and installer qualification belong to the final Windows phase. The owner approved implementation on 2026-10-18. Local implementation does not establish a published update channel or a qualified installed-version upgrade.


## Implemented boundary

The native wrapper pins `tauri-plugin-updater` 2.10.1 alongside Tauri 2.11.6, avoiding an unrelated Tauri minor upgrade. The plugin downloads and verifies the signature; only the resulting native-owned bytes can reach installation. The frontend cannot supply a package URL, signature, public key or install bytes. Plugin guest commands receive no capabilities; wrapper commands are guarded by the existing main-window dispatcher. No updater permissions are granted to previews.

The frontend refuses installation during turns, workspace switches, engine setup, open terminal sessions or unsaved buffers across workspaces, and rechecks after confirmation and core shutdown. Confirmation explicitly authorizes stopping the workspace and its services. Native installation additionally requires the core to be stopped and all terminals closed. A shared native activity lock prevents a new core/terminal launch racing with installation. Normal close is refused while installation owns the lock. Successful installation blocks new activity until explicit restart; restart uses the existing exit-cleanup path. Forced OS termination and power-loss recovery still require qualification.

## Build and release configuration

Set these **at build time**, not through a user workspace or runtime environment:

- `PWR_UPDATER_PUBLIC_KEY`: public key content from the durable Tauri signing pair.
- `PWR_UPDATER_ENDPOINT`: HTTPS URL serving the static manifest for this channel.
- `PWR_UPDATER_CHANNEL`: `stable` or `prerelease`; stable refuses prerelease offers.

The normal development build remains unconfigured and does not check the network for updates. The release script accepts either no updater configuration or a complete configuration plus `TAURI_SIGNING_PRIVATE_KEY` (and optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). Private values belong in release secrets. Partial configuration fails closed. No key is embedded from runtime input.

Configured builds generate `PWR.app.tar.gz`, `.sig`, `latest.json` and checksums. The manifest points to the archive under its immutable version tag. The release workflow attaches these only to its existing draft release; it does not publish the draft or update a live channel. Hosting/copying the signed manifest to the declared endpoint is a separately authorized release step. Do not use GitHub's stable `latest` endpoint as a prerelease feed accidentally. A channel endpoint must remain reachable across releases and must not advertise absent artifacts.

Local verification: 166 frontend tests (including 7 updater cases), 9 native tests, native all-target Clippy, production frontend build and 3 manifest syntax tests pass. Mocked failure tests do not replace signature rejection against a real server or an installed-build upgrade. The existing initial-bundle warning remains (about 1.18 MB); its threshold is unchanged. A production key/feed and a real signed upgrade are still required before activating or claiming end-to-end update support.


## Native startup reconciliation

The first packaged walk found that the plugin requires a `pubkey` configuration field even when update commands are disabled. The unconfigured app now provides an empty field (not a signing key); wrappers still refuse checks without the complete build-time configuration. A deserialization regression checks this configuration and confirms all TLS/transport weakening flags remain false. The rebuilt native app opens, shows Settings → Updates, version/channel and the unconfigured notice, then exits normally. No update is downloaded or installed during this walk.

Latest scoped checks: 19 editor/updater frontend regressions pass after the hidden-buffer correction; 10 native tests pass. The earlier complete 166-test frontend suite and production build pass. A real signed package, bad-signature server test, installed-version upgrade and rollback/recovery exercise remain open; UI presence and mocked tests do not certify them.


Final integration verification: the complete frontend suite passes **168 tests** after the hidden-buffer and simultaneous-confirmation regressions (8 updater tests); 10 native tests, 3 manifest tests, Clippy and the final unsigned app build pass. The native walk above covers the same UI and configuration; the final bundle additionally includes the simultaneous-confirmation guard. Production activation and actual upgrade qualification remain pending.


## Local signing identity — 2026-10-18

The owner authorized creation of a new local pair. The public trust anchor is [updater.pub](../../apps/desktop/src-tauri/updater.pub), SHA-256 `7022c71f00ce5b879b71d9630b2edfd9d929d3ccde49e5c4860f503a80d95881`. Native builds embed it by default; an explicit build-time key override remains available for isolated qualification or a planned rotation. The absence of an endpoint still disables checks. The release script uses this public key when a complete updater release configuration is supplied.

The encrypted private key and a randomly generated signing password reside only in the owner's local release-signing directory under `$HOME/.config/pwr`, with directory mode 0700 and secret file modes 0600. They are neither repository files nor evidence attachments. The password is stored alongside the encrypted key to permit local unattended signing; filesystem access protects both. An independent protected backup and CI secret provisioning remain operational work; a second copy on the same computer is not disaster recovery. Losing this pair after distribution requires an explicit trust-anchor migration, not silently generating another production pair.

A real payload signed by the Tauri CLI is accepted by `minisign-verify` 0.2.5, the same verifier version used by the pinned updater. A one-byte payload mutation and a malformed signature are rejected. This is a cryptographic signing smoke test, not a server/download/installed-application upgrade test. No feed, remote secrets, release or publication was created.


## Isolated HTTPS and install probe

A local integration probe now exercises the actual pinned plugin against HTTPS: malformed signature and altered-archive downloads fail before installation; a genuine signed candidate archive downloads, verifies and installs into an owned isolated app copy. The installed executable matches the candidate hash. Only the fixture's plist version and marker distinguish the copies; repository versions remain unchanged. A mock application handle drives the plugin, with a temporary certificate authority added only to that client's trust store. TLS validation remains enabled and operating-system trust is unchanged. Harness setup failures and corrections remain in the private campaign evidence.

This replaces the earlier “real bad-signature server test still pending” statement only for this isolated plugin scope. Product Settings → download/install/restart against a hosted release, Apple signing/notarization, authorization/rollback/power interruption and Windows remain unqualified. The local keychain currently has no Developer ID Application identity. See the [campaign record](../reviews/2026-10-18-product-verification.md#local-signing-identity-and-durable-editor-drafts).
