#!/usr/bin/env python3
"""Summarize retained CLI metrics; never reimplement TaskOutcome.resolved()."""
import json
from pathlib import Path
import statistics
import sys

ROOT = Path(__file__).resolve().parent.parent
OUT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "experiments/local-v5-20260906"


def read(path):
    return json.loads(path.read_text())


def main():
    manifest = read(OUT / "manifest.json")
    models = manifest["models"]
    rows = []
    for path in sorted((OUT / "runs").glob("eval-*.finished.json")):
        finished = read(path)
        label = finished["label"]
        _, model_index, seed, task = label.split("-", 3)
        row = {"model": models[int(model_index)], "seed": int(seed), "task": task,
               "exit_code": finished["exit_code"], "invocation_secs": finished["elapsed_secs"],
               "invocation_timed_out": finished["timed_out"], "error": finished.get("error")}
        response_path = path.with_name(label + ".stdout.json")
        try:
            response = read(response_path)
            if response.get("ok"):
                result = response["result"]
                report = read(Path(result["report_json"]))
                assert len(report["outcomes"]) == 1, "only single-task reports are comparable here"
                metrics = {m["name"]: m for m in result["metrics"]}
                row.update(resolved=metrics["resolved_task_rate"]["successes"],
                           measured=metrics["resolved_task_rate"]["total"],
                           report=str(Path(result["report_json"]).relative_to(ROOT)),
                           harness_rev=report["harness_rev"], corpus_rev=report["corpus_rev"],
                           outcome=report["outcomes"][0])
            else:
                row["error"] = response.get("error") or "invocation failed without a structured error"
        except (ValueError, OSError) as error:
            row["error"] = f"response or report unavailable: {error}"
        rows.append(row)
    summary = []
    for model in models:
        trials = [r for r in rows if r["model"] == model]
        outcomes = [r["outcome"] for r in trials if "outcome" in r]
        times = [r["invocation_secs"] for r in trials]
        resolved = sum(r.get("resolved", 0) for r in trials)
        summary.append({
            "model": model, "planned": manifest.get("planned_per_model") or len(manifest["seeds"]) * len(manifest["tasks"]),
            "attempted": len(trials), "reported": len(outcomes), "resolved": resolved,
            "conditionally_measured": sum(r.get("measured", 0) for r in trials),
            "provider_failures": sum(o["provider_failure"] for o in outcomes),
            "invocation_failures": sum("outcome" not in r for r in trials),
            "wall_minutes": sum(times)/60,
            "median_invocation_secs": statistics.median(times) if times else None,
            "resolved_per_minute": resolved/(sum(times)/60) if sum(times) else None,
            "turns": sum(o["turns"] for o in outcomes),
            "generated_tokens": sum(o["generated_tokens"] for o in outcomes),
            "peak_prompt_tokens": max((o["peak_prompt_tokens"] for o in outcomes), default=None),
            "compactions": sum(o["events"].get("context.compacted", 0) for o in outcomes),
            "turns_under_pressure": sum(o["turns_under_pressure"] for o in outcomes),
        })
    data = {"complete": (OUT / "complete.json").exists(), "models": summary, "trials": rows}
    (OUT / "summary.json").write_text(json.dumps(data, indent=2) + "\n")
    lines = ["# Local v5 diagnostic results", "",
             "Generated from each invocation's own resolved-task metric. Previously used tasks; trial counts are specified in protocol.md and manifest.json. No causal comparison with historical runs and no generalization claim.", "",
             f"Runner complete: {data['complete']}", "",
             "| Model | Resolved / attempted (planned) | Wall min | Turns | Generated tokens | Peak prompt | Compactions |",
             "|---|---:|---:|---:|---:|---:|---:|"]
    for s in summary:
        lines.append(f"| {s['model']} | {s['resolved']}/{s['attempted']} ({s['planned']}) | {s['wall_minutes']:.2f} | {s['turns']} | {s['generated_tokens']} | {s['peak_prompt_tokens']} | {s['compactions']} |")
    lines += ["", "Wall time includes CLI preparation and verification; calibration time is separate. Missing reports stay in the attempted denominator. Conditional model-served counts and individual artifacts are in summary.json.", "",
              "| Model | Seed | Task | Resolved | Terminal | Report |",
              "|---|---:|---|---:|---|---|"]
    for r in rows:
        o = r.get("outcome", {})
        report = "[JSON](../../"+r["report"]+")" if "report" in r else "missing"
        terminal = str(o.get("terminal") or o.get("error") or r.get("error") or "normal exit")
        terminal = terminal.replace("\n", " ").replace("|", "\\|")[:240]
        lines.append(f"| {r['model']} | {r['seed']} | {r['task']} | {r.get('resolved', 'unmeasured')} | {terminal} | {report} |")
    (OUT / "results.md").write_text("\n".join(lines)+"\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
