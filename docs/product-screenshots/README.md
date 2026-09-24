# PWR visual documentation

Real application screenshots captured from the installed PWR desktop app on 2026-09-24. This documents observed behavior; it is not a product specification. No PWR frontend, styles, or functionality were changed.

Images are 2720 × 1718 JPEG captures. They show the application window. The capture tool sometimes adds a purple badge or pointer; those are capture artifacts, not PWR interface elements. Paths into the user's home directory were redacted in settings and model-manager images. The demo workspace path `/private/tmp/pwr-demo-doc-capture` is visible in some captures and contains only the disposable greeting example.

“Public-safe” here means no private conversation, personal file contents, credentials, or unredacted home-directory paths are visible. It does not imply the image is polished marketing material. No final website selections are made here.

## Captured screens

| Screenshot | State and feature | How reached | Public-safe | Website candidate | Privacy / reproducibility notes |
|---|---|---|---|---|---|
| [02-first-launch-welcome-engine-setup.jpg](02-first-launch/02-first-launch-welcome-engine-setup.jpg) | Fresh-profile welcome and engine setup requirements | Backed up PWR's local state, then opened the installed app | Yes | Maybe | Real first-run screen; engine install estimates about 1.2 GB. |
| [02-first-launch-engine-installing.jpg](02-first-launch/02-first-launch-engine-installing.jpg) | Python/MLX engine install progress | Chose Install engine and waited during setup | Yes | Maybe | Real progress state. |
| [02-first-launch-engine-ready.jpg](02-first-launch/02-first-launch-engine-ready.jpg) | Engine installed and verified; Continue action | Waited for installation and checks to finish | Yes | Maybe | Real success state; path shown as `~/Library/...`. |
| [02-first-launch-fresh-empty-shell.jpg](02-first-launch/02-first-launch-fresh-empty-shell.jpg) | Empty Chat shell after first-run engine setup | Continued into PWR with reset chat storage | Yes | Maybe | Sidebar was collapsed in this capture. |
| [02-first-launch-model-manager-discover.jpg](02-first-launch/02-first-launch-model-manager-discover.jpg) | Model discovery and hardware summary during setup | Opened Model Manager from the no-model-selected state | Yes | Maybe | Home-directory path redacted; machine specifications are visible. |
| [02-first-launch-model-manager-local-model.jpg](02-first-launch/02-first-launch-model-manager-local-model.jpg) | Existing local model available for first-run selection | Opened On this Mac | Yes | Maybe | LM Studio path redacted; reused the existing model without downloading it. |
| [02-first-launch-clean-chat-with-model.jpg](02-first-launch/02-first-launch-clean-chat-with-model.jpg) | Clean Chat ready with existing model selected | Chose Use for the local model | Yes | Maybe | Empty conversation list; no new model downloaded. |
| [02-workspace-trust-dialog.jpg](03-configuration/02-workspace-trust-dialog.jpg) | Workspace trust prompt | Selected the disposable demo folder in Agent mode | Yes | No | Demo path visible. Trust prompt is real. |
| [04-empty-state-chat-light.jpg](04-main-shell/04-empty-state-chat-light.jpg) | Empty Chat state in the OS light appearance | Switched to Chat with no conversation selected | Yes | Maybe | Private history was hidden with the sidebar collapsed. Capture badge/pointer present. |
| [05-empty-state-chat-dark.jpg](04-main-shell/05-empty-state-chat-dark.jpg) | Empty Chat state in Dark appearance | Changed Appearance to Dark and opened a new conversation | Yes | Maybe | Capture badge/pointer present. |
| [06-agent-empty-selected-model-dark.jpg](04-main-shell/06-agent-empty-selected-model-dark.jpg) | Empty Agent workspace and composer | Trusted the demo workspace and selected Agent | Yes | No | Demo workspace path visible. |
| [06-coding-run-task-submitted.jpg](06-agent-run/06-coding-run-task-submitted.jpg) | Submitted coding task and active run | Asked PWR to add `--shout` support to the demo greeting CLI | Yes | No | Run state captured; tool activity later encountered verifier restrictions. |
| [06-coding-run-file-changes.jpg](06-agent-run/06-coding-run-file-changes.jpg) | Agent conversation showing code changes | Same demo run after editing `greeting.py` and its tests | Yes | No | Actual changed files in disposable `/tmp` workspace. |
| [07-model-selector-untested-dark.jpg](07-models/07-model-selector-untested-dark.jpg) | Model selector, selected local model, untested status, capability/context details | Opened the model selector in Agent mode | Yes | Maybe | The displayed model profile has thinking disabled, so Low/Medium/High controls are unavailable here. |
| [10-model-manager-discover-dark.jpg](07-models/10-model-manager-discover-dark.jpg) | Discover list, hardware summary, filters, recommendations and downloadable models | Opened Model Manager → Discover | Yes | Maybe | Home-directory download location redacted. Hardware and free-disk figures are machine-specific. No model downloaded. |
| [10-model-manager-on-this-mac-dark.jpg](07-models/10-model-manager-on-this-mac-dark.jpg) | Installed local model and Use/Delete actions | Opened Model Manager → On this Mac | Yes | Maybe | Model path and download directory redacted. Existing model was selected with Use; it was not deleted. |
| [08-context-empty-dark.jpg](08-context/08-context-empty-dark.jpg) | Empty context summary | Opened context popover before a conversation had content | Yes | Maybe | Real empty-state summary. |
| [08-context-populated-dark.jpg](08-context/08-context-populated-dark.jpg) | Used, Window, Remaining and context composition | Opened context popover after the demo coding run | Yes | Maybe | Values come from the actual run. |
| [08-context-post-compaction-dark.jpg](08-context/08-context-post-compaction-dark.jpg) | Manual compact result and updated context values | Used Compact now on the disposable demo conversation | Yes | Maybe | Real post-compaction state. |
| [09-inspector-evidence-no-checks.jpg](09-inspector/09-inspector-evidence-no-checks.jpg) | Evidence / Verify result | Selected Evidence after the agent run and invoked Verify | Yes | No | PWR reports that the workspace declares no checks; useful limitation reference. |
| [09-inspector-core-log-empty.jpg](09-inspector/09-inspector-core-log-empty.jpg) | Core log empty state | Selected Core log after the run | Yes | No | PWR reported that the core had written nothing to its log. |
| [10-attachments-menu-light.jpg](10-attachments/10-attachments-menu-light.jpg) | Attachment menu with Images, Files, and Folder read-only | Opened Attach in a clean Chat composer | Yes | Maybe | Real menu; capture badge and pointer present. |
| [10-attachment-demo-readme-composer.jpg](10-attachments/10-attachment-demo-readme-composer.jpg) | Harmless README attached to the Chat composer | Selected the disposable demo README in the file picker | Yes | Maybe | Attachment is staged but unsent. Demo path is visible; removed from the live composer after capture. |
| [12-settings-system-light.jpg](11-settings/12-settings-system-light.jpg) | Settings, System appearance, installed engine and shortcuts | Opened Settings while macOS was in Light appearance | Yes | Maybe | Local engine path is redacted. Shows actual installed-engine state. |
| [13-settings-light.jpg](11-settings/13-settings-light.jpg) | Settings, explicit Light appearance | Selected Light in Appearance | Yes | Maybe | Local engine path is redacted. |
| [13-settings-dark.jpg](11-settings/13-settings-dark.jpg) | Settings, explicit Dark appearance | Selected Dark in Appearance | Yes | Maybe | Local engine path is redacted. |

## States inspected but not captured

The first-run engine welcome, install progress, and ready state were captured after backing up and resetting PWR's local state. The engine was reinstalled (about 1.2 GB); the existing LM Studio model was reused. The generated DMG, drag-to-Applications screen, and a Gatekeeper warning remain uncaptured. The same installed app had already been approved by macOS, so reopening it did not produce a Gatekeeper warning. No macOS security settings were changed.

The model manager showed Discover and On this Mac; no search query, additional filters, download progress/completion, delete-confirmation dialog, or model calibration run was captured. The selected model was marked untested and offered Quick Calibration, but calibration was not run. The app reported that thinking is disabled for this model profile, so Low/Medium/High could not be demonstrated honestly.

The attachment menu and an unsent demo README attachment in the Chat composer were captured. Image selection, folder selection, and a posted attachment inside a sent conversation were not captured. The file picker exposed unrelated local filenames in its accessibility tree; no picker screenshot was saved. The conversation delete control displayed a tooltip, but no confirmation dialog appeared, so no delete action was taken. Conversation rename/context menu options were not captured. No additional app-window sizes, light-theme model/context views, or medium/small responsive states were captured.

Before reset, the Chat list contained private history. PWR's app data and chat database were moved to a local backup before testing; the clean profile now has no prior conversations (confirmed in the app, though the expanded-sidebar capture had a layout glitch and was discarded). The retained first-run captures use a collapsed sidebar or the disposable Agent workspace; no private conversation was saved into this set. The backup is at `~/Desktop/PWR-reset-backup-20260924-161618`; `RESTORE.txt` inside it lists the original locations and restore steps.

## Agent run and verification

The real PWR run edited only the disposable workspace at `/private/tmp/pwr-demo-doc-capture`, changing `greeting.py` and `tests/test_greeting.py`. PWR could not run the declared command because its workspace verifier policy rejected Python execution and it attempted to add project configuration; that configuration was refused and not retained. Running the README command manually in the disposable workspace produced four passing tests, plus `Hello, world!` and `HELLO, PWR!` from the default and shout CLI invocations. The inspector Evidence view still says no checks are declared. This distinction is visible in the captures and should be retained if the run is reused publicly.

## Preliminary strongest references

For product review, the most informative captures are the empty Chat state, the model selector, the populated context panel, the Discover list, the installed model row, and the settings appearance cards. The two agent-run images show a real workflow but also expose the tool-verification limitation. Before website use, recapture without the purple capture badge/pointer, confirm the demo path policy, and choose current model catalog values deliberately.
