# 9B desktop preparation and repeat evaluation

2026-10-12. Vito authorizes useful app/frontend changes while the local 9B
training runs, then repeating previously observed tasks. Only lightweight
source edits now; builds, tests and model inference wait until training exits.

The model picker abbreviates references by dropping quantization markers.
Show the exact reference alongside its readable name so variants remain
distinguishable. Quick Calibration is a protocol compatibility smoke, not
evidence of coding ability. State that distinction beside its results.

Do not allow model selection while calibration or a conversation turn runs.
Enforce this both at the picker and at the shared store; do not silently attach
an assessment from one selected model to a different selection. Do not start
calibration during an active turn. Test these cases through the store.

Acceptance after training: focused AgentStore tests pass; desktop production
build passes; inspect the selector's reference wrapping and existing selected,
disabled and narrow-panel layout. Until those checks, changes are prepared,
not validated or deployed. No backend/tool schema/prompt changes in this patch.

First model evaluation: preserve the frozen helper and fixtures from
`pwr-evidence/swebench-pilot/audits/qlora-microrepo-hard-qwen35-9b-20261012`.
Repeat the three previously observed interval/dependency/version tasks with
the new coding adapter, in a fresh evidence folder. Preserve the five phases,
temperature 0, top_p 1, top_k 0, seed 42, thinking false and 512-token cap.
Only adapter weights and their required loader mapping may differ; verify
every mapped tensor before inference. Do not recompile the helper against
this UI patch. If runtime/render/helper identity differs, label the comparison
as descriptive and report the differences. No automatic adoption of weights.

Report solved tasks, accepted phases, final independent tests, extra tool
attempts, closing behavior and elapsed time. Historical base results may be
reused only with exact recorded provenance. These known tasks measure changes
on an observed diagnostic suite, not generalization. A coding adapter has not
yet been trained on native PWR behavior. No reserved forty tasks, release,
upload, push or new large campaign.

## Validation after training, 2026-10-12

AgentStore focused suite: 23 tests passed (including the two new guards).
Desktop production build passed with Angular 22; git diff --check passed.
The built demo was inspected in the in-app browser: the 380px model panel
shows the full references legibly, the selected row remains distinct, and
both model choices are disabled during the active turn. Calibration's
compatibility paragraph is conditional and was not present in this demo
state; the production template compiled, but that state was not visually
exercised. No app deployment or model default change.

The repeat evaluation is recorded separately in
`audits/qlora-coding-hard-r1-20261012` in the benchmark repository; this patch
does not change the frozen helper or sidecar used there.
