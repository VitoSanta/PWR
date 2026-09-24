> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Hardware Profiling

## Requirement

Capture a versioned `HardwareProfile` before calibration and execution. Include OS/build, architecture, CPU topology, total and currently available memory, accelerator/device capability, storage free space, and power/thermal state where reliably available. Never log serial numbers or user names.

## Collection

Implement a `HardwareProbe` trait. macOS adapter uses documented system interfaces/commands, parsing into typed units; Linux and Windows are later adapters. Record raw probe provenance, timestamp, probe version, unavailable fields, and units. A failed optional probe is `unknown`, not zero.

## Policy

**Heuristic:** reserve a configurable memory floor for OS, editor, and concurrent services; calculate using measured pressure and calibration, not a hard-coded percentage. Refuse calibration/execution when free storage cannot hold artifacts or memory pressure exceeds policy. Hardware changes invalidate calibration by compatibility key.

## A normalized host profile for model recommendations — 2026-09-23

`pwr_runtime::host::detect_host` adds a normalized `HostProfile` for the
app and the Model Manager: OS version, Apple chip, total and available memory
(unified or not), GPUs with exact VRAM only where a source reports it
(`nvidia-smi`), and free disk where models are stored, from the `sysinfo`
library rather than parsed command output. `probe_hardware` is unchanged,
because calibrations are keyed on its hash. Current description:
[`models-and-context.md`](models-and-context.md).
