#!/usr/bin/env python3
"""Execute the preregistered local-v5 diagnostic pilot, serially and durably.

No dependencies beyond Python's standard library; no model installation.
Each CLI invocation has bounded runtime and its own immutable output files.
An existing experiment directory is never resumed or overwritten implicitly.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "experiments/local-v5-20260906"
MODELS = ["qwen3.8:27b-mlx", "ornith-1.5:35b", "gpt-oss:20b"]
TASKS = [
    ("m6-hard-v1", "crossmodule-median"),
    ("m6-hard-v1", "known-failing-suite"),
    ("m6-hard-v1", "diagnose-do-not-fix"),
    ("m6-hard-v1", "monotonic-table"),
    ("realistic-v1", "two-files-in-a-large-repo"),
]


def write_json(path, value):
    with path.open("x") as f:
        json.dump(value, f, indent=2)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    runs = OUT / "runs"
    runs.mkdir(exist_ok=False)
    inputs = OUT / "inputs"
    inputs.mkdir(exist_ok=False)
    binary = ROOT / ".pwr/experiment-binaries/local-v5-20260906"
    binary.parent.mkdir(parents=True, exist_ok=True)
    if binary.exists():
        raise RuntimeError("refusing to replace frozen experiment binary")
    shutil.copy2(ROOT / "target/debug/PWR", binary)
    source_files = sorted((ROOT / "crates").rglob("*.rs")) + sorted((ROOT / "crates").rglob("*.toml"))
    source_files += [ROOT / p for p in ["Cargo.toml", "Cargo.lock", "docs/thresholds.json", "strategies/default.json", "strategies/models.json"]]
    started = time.monotonic()
    deadline = started + 4 * 3600
    manifest = {
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "binary_sha256": digest(binary), "binary": str(binary),
        "protocol_sha256": digest(OUT / "protocol.md"),
        "runner_sha256": digest(Path(__file__)),
        "source_sha256": {str(p.relative_to(ROOT)): digest(p) for p in source_files},
        "models": MODELS, "seeds": [1, 2], "tasks": TASKS,
    }
    write_json(OUT / "manifest.json", manifest)

    def invoke(label, args, timeout):
        if time.monotonic() >= deadline:
            raise TimeoutError("overall preregistered deadline reached")
        command = [str(binary), "--json", *args]
        record = {"label": label, "command": command, "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        write_json(runs / f"{label}.started.json", record)
        print(f"START {label}", flush=True)
        before = time.monotonic()
        with (runs / f"{label}.stdout.json").open("x") as stdout, (runs / f"{label}.stderr.log").open("x") as stderr:
            process = subprocess.Popen(command, cwd=ROOT, stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                code = process.wait(timeout=min(timeout, deadline - before))
                record["timed_out"] = False
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGINT)
                try:
                    code = process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    code = process.wait()
                record["timed_out"] = True
        record.update(exit_code=code, elapsed_secs=time.monotonic()-before)
        try:
            response = json.loads((runs / f"{label}.stdout.json").read_text())
        except (ValueError, OSError):
            response = {}
        record["ok"] = response.get("ok", False)
        record["error"] = response.get("error")
        write_json(runs / f"{label}.finished.json", record)
        print(f"END {label} exit={code} elapsed={record['elapsed_secs']:.1f}s", flush=True)
        return response

    profiles = {}
    for i, model in enumerate(MODELS):
        reply = invoke(f"calibration-{i}", ["calibrate", model, "--ladder", "8192,16384,32768", "--seed", "1"], 3 * 4 * 600 + 180)
        if reply.get("ok"):
            profiles[model] = reply["result"]["artifact"]
    write_json(OUT / "profiles.json", profiles)
    selected = []
    for corpus, name in TASKS:
        suite = json.loads((ROOT / f"corpus/{corpus}.json").read_text())
        task = next(t for t in suite["tasks"] if t["id"] == name)
        path = inputs / f"{name}.json"
        write_json(path, {"name": f"local-v5-{name}", "tasks": [task]})
        selected.append((name, path, task["time_budget_secs"]))
    for seed in [1, 2]:
        ordered_models = MODELS if seed == 1 else list(reversed(MODELS))
        ordered_tasks = selected if seed == 1 else list(reversed(selected))
        for model in ordered_models:
            if model not in profiles:
                continue
            for name, path, budget in ordered_tasks:
                label = f"eval-{MODELS.index(model)}-{seed}-{name}"
                invoke(label, ["eval", "run", str(path), "--model", model, "--profile", profiles[model], "--seed", str(seed), "--out-dir", str(OUT / "reports")], budget + 180)
    write_json(OUT / "complete.json", {"elapsed_secs": time.monotonic()-started, "admitted_models": list(profiles)})


if __name__ == "__main__":
    main()
