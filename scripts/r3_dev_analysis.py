"""Analyses R3 H2's development run (experiments/r3-h2-dev-20260917).

Answers the protocol's four questions from the campaign's own artifacts, never
from a model's account of itself:

1. compactions per task under the current policy, and which tasks reach two;
2. for each injected variant, the action step of its injection against the
   steps of its compactions -- a compaction must follow either injection, and
   an external edit should also come after one;
3. evidence-state defects: windows added, stale contents kept, harness errors;
4. minutes per trial, and host hours for a campaign at the provisional size.

Development, not evidence: the report prints no treatment effect.

Usage: python3 scripts/r3_dev_analysis.py [REPORTS_DIR]
"""
from __future__ import annotations

import glob
import json
import os
import statistics
import sys
from collections import defaultdict

REPORTS = sys.argv[1] if len(sys.argv) > 1 else "experiments/r3-h2-dev-20260917/reports"
CORPUS = "corpus/longhorizon-v2.json"


def trace_events(path: str) -> list[dict]:
    if not path or not os.path.exists(path):
        # Traces move with the reports directory; the outcome records where it was.
        path = os.path.join(REPORTS, "traces", os.path.basename(path or ""))
    if not os.path.exists(path):
        return []
    return [json.loads(line) for line in open(path)]


def main() -> None:
    corpus = {t["id"]: t for t in json.load(open(CORPUS))["tasks"]}
    policies = {}
    for manifest in glob.glob(os.path.join(REPORTS, "manifest-*.json")):
        m = json.load(open(manifest))
        policies[m["campaign"]] = m.get("context_policy") or "current"

    trials = defaultdict(list)
    for path in sorted(glob.glob(os.path.join(REPORTS, "trials-*", "*-outcome.json"))):
        record = json.load(open(path))
        o = record["outcome"]
        policy = policies.get(record["campaign"], "?")
        events = trace_events(o.get("event_trace"))
        compacted = [e["payload"]["step"] for e in events if e["event_type"] == "context.compacted"]
        injected = [(e["event_type"], e["payload"].get("step")) for e in events
                    if e["event_type"] in ("task.revision", "injection.external_edit")]
        trials[policy].append(dict(
            task=o["task_id"], resolved=bool(o["declared_complete"] and o["hidden_verifier_passed"]),
            hidden=o["hidden_verifier_passed"], minutes=o["duration_secs"] / 60, error=o["error"],
            terminal=o["terminal"], provider_failure=o["provider_failure"], m=o["mechanism"],
            compacted=compacted, injected=injected, events=events,
        ))

    for policy, rows in sorted(trials.items()):
        print(f"\n## Policy {policy}: {len(rows)} trials\n")
        reach = [r for r in rows if r["m"]["compactions"] >= 2]
        print(f"Resolved {sum(r['resolved'] for r in rows)}/{len(rows)}; hidden check passing "
              f"{sum(r['hidden'] for r in rows)}/{len(rows)}; provider failures "
              f"{sum(r['provider_failure'] for r in rows)}")
        print(f"Tasks with at least two compactions: {len(reach)}/{len(rows)}")
        minutes = [r["minutes"] for r in rows]
        if minutes:
            print(f"Minutes per trial: median {statistics.median(minutes):.1f}, mean "
                  f"{statistics.mean(minutes):.1f}, max {max(minutes):.1f}; total {sum(minutes) / 60:.1f} h")
        print("\n| Task | Resolved | Compactions | Steps compacted | Injection | Lands | Windows | Stale kept | "
              "Re-reads after compaction | Min | Stop |")
        print("|---|---|---:|---|---|---|---:|---:|---:|---:|---|")
        timing = defaultdict(int)
        for r in rows:
            spec = corpus.get(r["task"], {}).get("injections") or []
            lands = ""
            if spec:
                kind = spec[0]["kind"]
                delivered = [step for event, step in r["injected"]]
                if not delivered:
                    lands = "not delivered"
                else:
                    step = delivered[0]
                    before = sum(1 for c in r["compacted"] if c < step)
                    after = sum(1 for c in r["compacted"] if c >= step)
                    lands = f"{before} before, {after} after"
                    # Exposed: a compaction follows the delivery, so a policy can
                    # lose it. An edit is meant to change a file the run has
                    # already read and had compacted away, so one precedes it too.
                    wanted = after >= 1 if kind == "revision" else (before >= 1 and after >= 1)
                    timing[(kind, wanted)] += 1
                    lands += "" if wanted else " (off)"
                if not delivered:
                    timing[(kind, False)] += 1
            m = r["m"]
            print(f"| {r['task']} | {'yes' if r['resolved'] else 'no'} | {m['compactions']} | "
                  f"{','.join(map(str, r['compacted']))} | {spec[0]['kind'] if spec else ''} | {lands} | "
                  f"{m['evidence_windows']} | {m['stale_file_contents_kept']} | {m['rereads_after_compaction']} | "
                  f"{r['minutes']:.1f} | {r['terminal'] or ''} |")
        if timing:
            print("\nInjection timing (as intended / not): " + "; ".join(
                f"{kind}: {timing[(kind, True)]} / {timing[(kind, False)]}" for kind in ("revision", "external_edit")))
        errors = defaultdict(int)
        for r in rows:
            if r["error"] and not r["error"].startswith("action budget"):
                errors[r["error"][:140]] += 1
        if errors:
            print("\nErrors other than the action budget:")
            for text, count in sorted(errors.items(), key=lambda x: -x[1]):
                print(f"  {count} x {text}")

    # Paired view on the variants run under both policies, descriptive only.
    if "current" in trials and len(trials) > 1:
        for policy, rows in trials.items():
            if policy == "current":
                continue
            base = {r["task"]: r for r in trials["current"]}
            pairs = [(base[r["task"]], r) for r in rows if r["task"] in base]
            print(f"\n## Variants under current and {policy}: {len(pairs)} pairs (development, not an effect)\n")
            for key, label in (("rereads_after_compaction", "re-reads after compaction"),
                               ("evidence_windows", "evidence windows"),
                               ("stale_file_contents_kept", "stale contents kept")):
                print(f"{label}: current {sum(a['m'][key] for a, _ in pairs)}, "
                      f"{policy} {sum(b['m'][key] for _, b in pairs)}")
            print(f"minutes: current {sum(a['minutes'] for a, _ in pairs):.0f}, "
                  f"{policy} {sum(b['minutes'] for _, b in pairs):.0f}")

    independent = len({t.get("id", "").removesuffix("-revised").removesuffix("-edited") for t in corpus.values()})
    rows = trials.get("current", [])
    if rows:
        per_trial = statistics.mean(r["minutes"] for r in rows)
        print(f"\nIndependent base tasks: {independent}. At {per_trial:.1f} min per trial, one arm over the "
              f"{len(corpus)} tasks at three seeds is {len(corpus) * 3 * per_trial / 60:.0f} h; three arms "
              f"{3 * len(corpus) * 3 * per_trial / 60:.0f} h.")


if __name__ == "__main__":
    main()
