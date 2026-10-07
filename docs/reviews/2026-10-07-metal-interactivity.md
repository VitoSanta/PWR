# Metal interactivity failure — 2026-10-07

Status: code correction and local validation completed; this record is not a release gate or a guarantee for every long-context workload.

## Failure and evidence

A local Qwen3.6 35B A3B 4-bit conversation failed around 108k tokens with `kIOGPUCommandBufferCallbackErrorImpactingInteractivity`. The macOS kernel recorded `AGX: Channel error 14` at the same instant. The retry succeeded on the same request and later reached 122k tokens. This error does not itself establish out-of-memory or a memory leak.

Subsequent measurements of the replacement process were about 28 GB footprint, with a 34 GB lifetime peak. They are not measurements of the failed process. Nearby kernel records reported approximately 28–29 GB wired against a 55.7 GB limit; no peak at the precise failure is available. Personal conversation, images and raw machine logs remain outside the repository.

At 108,393 keys on the same model's attention geometry (16 query heads, 2 KV heads, head dimension 256), an isolated attention probe measured about 58 MB additional peak allocation for 16 query tokens and 920 MB for 256. This demonstrates the temporary-memory scaling, not the complete model's peak or a reproduction of the crash.

## Changes

- Before importing MLX, the macOS sidecar defaults `AGX_RELAX_CDM_CTXSTORE_TIMEOUT` to `1`, preserving any explicit caller override, including `0`. This is the [GPU context-switch timeout workaround adopted by llama.cpp](https://github.com/ggml-org/llama.cpp/pull/22216). It does not increase the wired-memory or context budget.
- Adaptive prefill can shrink to 16 tokens rather than stopping at 256. Non-fused attention targets one second per chunk; fused attention retains the three-second target. Growth is at most twice the previous chunk. These remain measured time targets, not guarantees about individual GPU kernel duration.
- Errors include the current operation and a memory sample taken before the operation. Diagnostics can be returned without asking a damaged GPU for another measurement. Optional tracing records chunk durations and before/after memory; raw generation tracing can contain private conversation text and must remain local.
- The existing provider recovery still discards a sidecar whose Metal command failed. It never reuses a damaged GPU engine.

The exact duration of a failed GPU kernel was not captured historically. The evidence identifies the driver failure class; it does not prove which kernel, image batch or context-switch delay triggered this particular occurrence. The workaround is a mitigation of that class, not an upstream driver repair.

## Checks

- Sidecar suite: 87 tests run, 86 passed, one skipped because its historical experiment trace fixtures are outside this checkout.
- Rust MLX provider suite: 38 passed, including fresh-process recovery after a Metal fault.
- Startup test intercepts the MLX import and checks the driver setting is already applied, with unset, enabled and disabled caller environments.
- Regression tests exercise shrinking below the former floor, diagnostics on a damaged device, and error metadata on the actual JSON protocol.
- Full-model replay: three baseline requests and five corrected requests completed, with no Metal errors. Initial inputs matched at 118,784, 118,809 and 118,834 tokens. Two additional corrected requests reprocessed growing image groups (29 and 30 images), reaching 119,989 and 121,144 tokens. No generated tool actions were executed.
- Corrected active memory was about 26 GB after the matched requests, and 26.32 GB after both added images. The largest MLX peak was 33.575 GB. This is a short repeat test, not an hours-long leak test.
- The cold request took 383.439 s before and 413.157 s with correction and tracing; this single observation is about 7.8% slower. Warm matched requests took 8.6–9.3 s. No general performance claim is inferred from this pair.
- The initial corrected vision batch took 21.625 s. Error phase metadata is important because vision work precedes language prefill and can itself be long.
- The first capped answer differed between chunk schedules; the two matched warm answers were identical. The replay is not a quality evaluation or a claim of bitwise equivalence.

The original fault was not reproduced in the quiet comparison, even on baseline. Passing replays validate the corrected code's execution and memory behaviour; they do not establish that every interactivity timeout is eliminated. The exact historical request is unavailable, and native desktop/browser contention still needs verification on a release candidate.

See [inference](../inference.md) for startup configuration and diagnostic interpretation.
