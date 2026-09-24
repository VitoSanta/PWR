#!/usr/bin/env python3
"""Run the separately preregistered diagnostic-contract follow-up."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import urllib.request

from local_v5_experiment import ROOT, MODELS, digest, write_json

OUT = ROOT / "experiments/local-v5-readonly-fix-20260906"
BASE = ROOT / "experiments/local-v5-20260906"


def resident():
    with urllib.request.urlopen("http://127.0.0.1:11434/api/ps", timeout=10) as response:
        return json.load(response)["models"]


def main():
    if not (BASE / "complete.json").exists():
        raise RuntimeError("phase one must finish before the follow-up")
    runs = OUT / "runs"
    runs.mkdir(exist_ok=False)
    binary = ROOT / ".pwr/experiment-binaries/local-v5-readonly-fix-20260906"
    if binary.exists():
        raise RuntimeError("frozen follow-up binary already exists")
    shutil.copy2(ROOT / "target/debug/PWR", binary)
    profiles = json.loads((BASE / "profiles.json").read_text())
    source = json.loads((BASE / "manifest.json").read_text())["source_sha256"]
    manifest = {"binary_sha256": digest(binary), "binary": str(binary),
                "protocol_sha256": digest(OUT / "protocol.md"),
                "runner_sha256": digest(Path(__file__)),
                "source_sha256": {name: digest(ROOT / name) for name in source},
                "models": MODELS, "profiles": profiles,
                "planned_per_model": 3,
                "schedule": [["diagnose-do-not-fix", 1], ["diagnose-do-not-fix", 2], ["crossmodule-median", 1]],
                "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    write_json(OUT / "manifest.json", manifest)
    deadline = time.monotonic() + 7200
    for index, model in enumerate(MODELS):
        loaded = resident()
        unknown = [m["name"] for m in loaded if m["name"] not in MODELS]
        if unknown:
            raise RuntimeError(f"non-experiment models are resident: {unknown}")
        unloads = []
        for other in loaded:
            if other["name"] != model:
                result = subprocess.run(["ollama", "stop", other["name"]], capture_output=True, text=True, timeout=30)
                unloads.append({"model": other["name"], "exit_code": result.returncode,
                                "stdout": result.stdout, "stderr": result.stderr})
                result.check_returncode()
        after = resident()
        write_json(OUT / f"residency-{index}.json", {"before": loaded, "unloads": unloads, "after": after})
        if any(m["name"] != model for m in after):
            raise RuntimeError("other model still resident after unloading")
        for task, seed in [("diagnose-do-not-fix", 1), ("diagnose-do-not-fix", 2), ("crossmodule-median", 1)]:
            if time.monotonic() >= deadline:
                raise TimeoutError("follow-up deadline reached")
            label = f"eval-{index}-{seed}-{task}"
            command = [str(binary), "--json", "eval", "run", str(BASE / f"inputs/{task}.json"),
                       "--model", model, "--profile", profiles[model], "--seed", str(seed),
                       "--out-dir", str(OUT / "reports")]
            record = {"label": label, "model": model, "task": task, "seed": seed,
                      "command": command, "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
            write_json(runs / f"{label}.started.json", record)
            print(f"START {label}", flush=True)
            started = time.monotonic()
            with (runs / f"{label}.stdout.json").open("x") as out, (runs / f"{label}.stderr.log").open("x") as err:
                process = subprocess.Popen(command, cwd=ROOT, stdout=out, stderr=err, start_new_session=True)
                try:
                    code = process.wait(timeout=min(1080, deadline-started))
                    record["timed_out"] = False
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGINT)
                    try:
                        code = process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        code = process.wait()
                    record["timed_out"] = True
            record.update(exit_code=code, elapsed_secs=time.monotonic()-started)
            write_json(runs / f"{label}.finished.json", record)
            print(f"END {label} exit={code} elapsed={record['elapsed_secs']:.1f}s", flush=True)
    write_json(OUT / "complete.json", {"trials": 9, "finished_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})


if __name__ == "__main__":
    main()
