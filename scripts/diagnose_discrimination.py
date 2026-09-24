#!/usr/bin/env python3
"""Run a preregistered check that one corpus task discriminates.

Usage: diagnose_discrimination.py <task-id> <experiment-directory>
                                  [deadline-hours] [corpus] [seeds]

Same shape as the earlier campaigns: a frozen binary, one report per invocation,
exclusive residency inside the cohort, a deadline, and no retry that depends on
what a trial returned. The invocation loop is short enough to read here; the
residency check and the JSON helpers come from the runners that established
them, so the campaigns cannot drift apart in what they mean by a trial.

Each campaign's manifest records the hash of the runner that ran it. This file
took its task and directory from constants for the first one and from the
command line afterwards, so a manifest whose runner_sha256 does not match this
file identifies an earlier version of it in git history, not a different runner.
"""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

from local_v5_experiment import ROOT, MODELS, digest, write_json
from local_v5_readonly_followup import resident

BASE = ROOT / "experiments/local-v5-20260906"
SEEDS = [int(s) for s in sys.argv[5].split(",")] if len(sys.argv) > 5 else [1, 2]
TASKS = sys.argv[1].split(",")
OUT = ROOT / sys.argv[2]


def main():
    runs = OUT / "runs"
    runs.mkdir(exist_ok=False)
    inputs = OUT / "inputs"
    inputs.mkdir(exist_ok=False)
    binary = ROOT / ".pwr/experiment-binaries" / OUT.name
    if binary.exists():
        raise RuntimeError("frozen binary already exists")
    shutil.copy2(ROOT / "target/debug/PWR", binary)
    corpus = ROOT / (sys.argv[4] if len(sys.argv) > 4 else "corpus/m6-hard-v1.json")
    suite = json.loads(corpus.read_text())
    selected = []
    for name in TASKS:
        task = next(t for t in suite["tasks"] if t["id"] == name)
        path = inputs / f"{name}.json"
        write_json(path, {"name": f"discrimination-{name}", "tasks": [task]})
        selected.append((name, path, task["time_budget_secs"]))
    profiles = json.loads((BASE / "profiles.json").read_text())
    write_json(OUT / "manifest.json", {
        "binary_sha256": digest(binary), "binary": str(binary),
        "protocol_sha256": digest(OUT / "protocol.md"),
        "runner_sha256": digest(Path(__file__)),
        "corpus_sha256": digest(corpus), "corpus": str(corpus.relative_to(ROOT)),
        "tasks": TASKS, "models": MODELS, "seeds": SEEDS, "profiles": profiles,
        "planned_per_model": len(SEEDS) * len(TASKS),
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    deadline = time.monotonic() + 3600 * (float(sys.argv[3]) if len(sys.argv) > 3 else 1)
    for index, model in enumerate(MODELS):
        loaded = resident()
        unknown = [m["name"] for m in loaded if m["name"] not in MODELS]
        if unknown:
            raise RuntimeError(f"non-experiment models are resident: {unknown}")
        unloads = []
        for other in loaded:
            if other["name"] != model:
                stopped = subprocess.run(["ollama", "stop", other["name"]],
                                         capture_output=True, text=True, timeout=30)
                unloads.append({"model": other["name"], "exit_code": stopped.returncode,
                                "stdout": stopped.stdout, "stderr": stopped.stderr})
                stopped.check_returncode()
        after = resident()
        write_json(OUT / f"residency-{index}.json",
                   {"before": loaded, "unloads": unloads, "after": after})
        if any(m["name"] != model for m in after):
            raise RuntimeError("other model still resident after unloading")
        for seed, (name, task_path, budget) in [(s, t) for s in SEEDS for t in selected]:
            if time.monotonic() >= deadline:
                raise TimeoutError("preregistered deadline reached")
            label = f"eval-{index}-{seed}-{name}"
            command = [str(binary), "--json", "eval", "run", str(task_path),
                       "--model", model, "--profile", profiles[model], "--seed", str(seed),
                       "--out-dir", str(OUT / "reports")]
            record = {"label": label, "model": model, "task": name, "seed": seed,
                      "command": command,
                      "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
            write_json(runs / f"{label}.started.json", record)
            print(f"START {label}", flush=True)
            started = time.monotonic()
            with (runs / f"{label}.stdout.json").open("x") as out, \
                 (runs / f"{label}.stderr.log").open("x") as err:
                process = subprocess.Popen(command, cwd=ROOT, stdout=out, stderr=err,
                                           start_new_session=True)
                try:
                    code = process.wait(timeout=min(budget + 180, deadline - started))
                    record["timed_out"] = False
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGINT)
                    try:
                        code = process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        code = process.wait()
                    record["timed_out"] = True
            record.update(exit_code=code, elapsed_secs=time.monotonic() - started)
            try:
                response = json.loads((runs / f"{label}.stdout.json").read_text())
            except (ValueError, OSError):
                response = {}
            record["ok"] = response.get("ok", False)
            record["error"] = response.get("error")
            write_json(runs / f"{label}.finished.json", record)
            print(f"END {label} exit={code} elapsed={record['elapsed_secs']:.1f}s", flush=True)
    write_json(OUT / "complete.json",
               {"trials": len(MODELS) * len(SEEDS) * len(TASKS),
                "finished_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})


if __name__ == "__main__":
    main()
