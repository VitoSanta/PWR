#!/usr/bin/env python3
"""R2's comparison, from the rerun's retained outcomes.

The protocol asks a paired question: on the same task, with the same deployment,
does the PWR harness (B1) resolve what a conventional agent (B0) does not,
and how does the staged control (B2) sit between them. Rates alone cannot answer
it -- thirty tasks with a dozen resolutions each have a wide interval, and two
arms differing by three tasks may differ on three tasks or on nine that cancel.
So the comparison here is per task: the discordant pairs, an exact sign test on
them, and the classes the unresolved trials fall into.

Every trial is counted. A trial the classifier calls `harness` is a failure of
this harness rather than of the arm, so each rate is reported twice: over every
trial, and over the trials no harness defect touched. D6 -- prompts that filled
the window with no room to reply -- lands hardest on the arm with the largest
prompts, which is exactly the kind of bias that must be shown rather than
averaged away.

Usage: r2_rerun_analysis.py <experiment-dir> [--write]
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
from typing import Any

import r2_classify_failures as classifier

SUITES = ["external-v1", "external-v2", "m6-hard-v1", "longhaul-v1", "realistic-v1"]
# The arm as the classifier labels it, from the suite report's conditions.
ARMS = ["conventional", "poor_ai", "staged"]
ARM_NAMES = {
    "conventional": "B0 conventional",
    "poor_ai": "B1 PWR",
    "staged": "B2 staged",
}


def sign_test(wins: int, losses: int) -> float:
    """Two-sided exact binomial on the discordant pairs, p = 0.5 under the null.

    The pairs that agree carry no information about a difference, which is why
    they are not in it: this is the test the protocol names.
    """
    n = wins + losses
    if n == 0:
        return 1.0
    fewer = min(wins, losses)
    tail = sum(math.comb(n, i) for i in range(fewer + 1)) / 2**n
    return min(1.0, 2 * tail)


def trials(experiment: Path) -> list[dict[str, Any]]:
    """Every trial, with its condition, class and cost."""
    report = classifier.build(experiment)
    rows = []
    for row in report["rows"]:
        # The classifier keeps the class and the facts behind it; the costs
        # live in the trial's own outcome, which the campaign and index name.
        outcome_path = (
            experiment
            / "reports"
            / f"trials-{row.get('campaign')}"
            / f"{int(row.get('index') or 0):04d}-outcome.json"
        )
        outcome = {}
        if outcome_path.exists():
            outcome = json.loads(outcome_path.read_text()).get("outcome") or {}
        rows.append(
            {
                "task_id": row.get("task_id"),
                "suite": row.get("suite"),
                "arm": row.get("arm"),
                "deployment": row.get("deployment"),
                "class": row.get("class"),
                "resolved": row.get("class") == "resolved",
                # Resolved means declared complete and accepted. A task whose
                # hidden check passes while the run never declared completion
                # is work done and not claimed -- the distinction D4 was about,
                # and the one the budget exhaustions turn on.
                "hidden_passed": bool(outcome.get("hidden_verifier_passed")),
                "terminal": outcome.get("terminal"),
                "generated_tokens": outcome.get("generated_tokens") or 0,
                "duration_secs": outcome.get("duration_secs") or 0.0,
                "turns": outcome.get("turns") or 0,
                "rationale": row.get("rationale"),
            }
        )
    return rows


def paired(rows: list[dict[str, Any]], deployment: str, left: str, right: str, drop_harness: bool):
    """Tasks both arms ran, and how they disagree."""
    by_arm = {
        arm: {
            row["task_id"]: row
            for row in rows
            if row["deployment"] == deployment and row["arm"] == arm
        }
        for arm in (left, right)
    }
    shared = sorted(set(by_arm[left]) & set(by_arm[right]))
    if drop_harness:
        shared = [
            task
            for task in shared
            if by_arm[left][task]["class"] != "harness"
            and by_arm[right][task]["class"] != "harness"
        ]
    wins = [t for t in shared if by_arm[left][t]["resolved"] and not by_arm[right][t]["resolved"]]
    losses = [t for t in shared if by_arm[right][t]["resolved"] and not by_arm[left][t]["resolved"]]
    return {
        "tasks": len(shared),
        "left_resolved": sum(by_arm[left][t]["resolved"] for t in shared),
        "right_resolved": sum(by_arm[right][t]["resolved"] for t in shared),
        "only_left": wins,
        "only_right": losses,
        "p": sign_test(len(wins), len(losses)),
    }


def cost(rows: list[dict[str, Any]], deployment: str, arm: str) -> dict[str, Any]:
    these = [r for r in rows if r["deployment"] == deployment and r["arm"] == arm]
    return {
        "trials": len(these),
        "resolved": sum(r["resolved"] for r in these),
        "hidden_passed": sum(r["hidden_passed"] for r in these),
        "tokens": sum(r["generated_tokens"] for r in these),
        "minutes": round(sum(r["duration_secs"] for r in these) / 60),
        "turns": sum(r["turns"] for r in these),
    }


def markdown(experiment: Path, rows: list[dict[str, Any]]) -> str:
    deployments = sorted({r["deployment"] for r in rows if r["deployment"]})
    lines = [
        f"# R2 rerun analysis — `{experiment.name}`",
        "",
        "Generated by `scripts/r2_rerun_analysis.py`. Classes come from",
        "`scripts/r2_classify_failures.py` in the protocol's order of precedence.",
        "A `harness` trial is this harness failing, not the arm, and every rate is",
        "given with and without those trials.",
        "",
        "## Accounting",
        "",
        "| Deployment | Arm | Trials | Resolved | Hidden check passed | Generated tokens | Minutes | Turns |",
        "|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for deployment in deployments:
        for arm in ARMS:
            c = cost(rows, deployment, arm)
            if not c["trials"]:
                continue
            lines.append(
                f"| `{deployment}` | {ARM_NAMES[arm]} | {c['trials']} | {c['resolved']} | "
                f"{c['hidden_passed']} | {c['tokens']:,} | {c['minutes']} | {c['turns']} |"
            )
    lines += ["", "## The paired comparison", ""]
    for deployment in deployments:
        lines.append(f"### `{deployment}`")
        lines.append("")
        lines.append("| Comparison | Tasks | Resolved | Only left | Only right | Sign test |")
        lines.append("|---|---:|---|---:|---:|---:|")
        for left, right in (("poor_ai", "conventional"), ("poor_ai", "staged"), ("staged", "conventional")):
            for drop in (False, True):
                result = paired(rows, deployment, left, right, drop)
                if not result["tasks"]:
                    continue
                label = f"{ARM_NAMES[left]} vs {ARM_NAMES[right]}"
                if drop:
                    label += " (harness trials dropped)"
                lines.append(
                    f"| {label} | {result['tasks']} | {result['left_resolved']} v "
                    f"{result['right_resolved']} | {len(result['only_left'])} | "
                    f"{len(result['only_right'])} | p = {result['p']:.3f} |"
                )
        lines.append("")
        for left, right in (("poor_ai", "conventional"), ("poor_ai", "staged")):
            result = paired(rows, deployment, left, right, False)
            if result["only_left"] or result["only_right"]:
                lines.append(
                    f"- {ARM_NAMES[left]} alone: {', '.join(f'`{t}`' for t in result['only_left']) or 'none'}."
                )
                lines.append(
                    f"- {ARM_NAMES[right]} alone: {', '.join(f'`{t}`' for t in result['only_right']) or 'none'}."
                )
        lines.append("")
    lines += ["## Classes by condition", "", "| Deployment | Arm | " + " | ".join(classifier.PRECEDENCE) + " |",
              "|---|---|" + "---:|" * len(classifier.PRECEDENCE)]
    for deployment in deployments:
        for arm in ARMS:
            these = [r for r in rows if r["deployment"] == deployment and r["arm"] == arm and not r["resolved"]]
            if not these:
                continue
            counts = Counter(r["class"] for r in these)
            lines.append(
                f"| `{deployment}` | {ARM_NAMES[arm]} | "
                + " | ".join(str(counts.get(k, 0)) for k in classifier.PRECEDENCE)
                + " |"
            )
    lines += ["", "## By suite", "", "| Suite | Deployment | " + " | ".join(ARM_NAMES[a] for a in ARMS) + " |",
              "|---|---|" + "---:|" * len(ARMS)]
    for suite in SUITES:
        for deployment in deployments:
            cells = []
            for arm in ARMS:
                these = [r for r in rows if r["suite"] == suite and r["deployment"] == deployment and r["arm"] == arm]
                cells.append(f"{sum(r['resolved'] for r in these)}/{len(these)}" if these else "-")
            if any(cell != "-" for cell in cells):
                lines.append(f"| `{suite}` | `{deployment}` | " + " | ".join(cells) + " |")
    # The choice rule: what R3 should test is the largest class B1 loses trials
    # to that a treatment could address -- never `harness`, which is repaired.
    b1_unresolved = [r for r in rows if r["arm"] == "poor_ai" and not r["resolved"]]
    testable = Counter(r["class"] for r in b1_unresolved if r["class"] in classifier.AVOIDABLE_FOR_R3 and r["class"] != "harness")
    lines += [
        "",
        "## Choice rule",
        "",
        f"B1 unresolved trials by class: `{json.dumps(dict(Counter(r['class'] for r in b1_unresolved)), sort_keys=True)}`.",
        f"Largest class a treatment could address: `{testable.most_common(1)[0][0] if testable else '-'}`"
        f" ({testable.most_common(1)[0][1] if testable else 0} trials).",
        "",
        "## Harness trials",
        "",
    ]
    harness = [r for r in rows if r["class"] == "harness"]
    if harness:
        lines.append("| Deployment | Arm | Task | Why |")
        lines.append("|---|---|---|---|")
        for row in harness:
            lines.append(
                f"| `{row['deployment']}` | {ARM_NAMES.get(row['arm'], row['arm'])} | `{row['task_id']}` | {row['rationale']} |"
            )
    else:
        lines.append("None.")
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("experiment", type=Path)
    parser.add_argument("--write", action="store_true", help="write analysis.md into the experiment")
    arguments = parser.parse_args()
    rows = trials(arguments.experiment)
    text = markdown(arguments.experiment, rows)
    if arguments.write:
        (arguments.experiment / "analysis.md").write_text(text)
        print(f"wrote {arguments.experiment / 'analysis.md'}")
    else:
        print(text)


if __name__ == "__main__":
    main()
