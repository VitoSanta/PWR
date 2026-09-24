#!/usr/bin/env python3
"""What the post-freeze repairs are worth, paired against the rerun.

Preregistered in `experiments/r2-harness-revision-20260916/protocol.md`. Two
campaigns differ in one declared condition, the harness revision, and are
compared task by task. The primary outcome is the paired resolution count with a
sign test; the secondary outcomes are counted per action or per turn, where the
noise the variance measurement found -- a quarter of tasks flipping on the seed
alone -- is far smaller.

Every secondary count here is a quantity some repair was supposed to move, and
each is reported whether it moved or not:

- refusals by the cause each repair targets;
- turns whose prompt filled the window, and turns that returned reasoning with
  no call;
- compactions per trial, and re-reads of an unchanged file before and after the
  first compaction -- D6 makes the budget bind sooner, so this is where a
  correction could cost more than it saves.

Usage: r2_harness_revision_analysis.py <treatment-dir> <control-dir> [--write]
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
import math
from pathlib import Path
from typing import Any

def sign_test(wins: int, losses: int) -> float:
    total = wins + losses
    if total == 0:
        return 1.0
    fewer = min(wins, losses)
    return min(1.0, 2 * sum(math.comb(total, i) for i in range(fewer + 1)) / 2**total)


def trials(experiment: Path) -> list[dict]:
    """Every trial, with the two conditions that select a block: arm and model.

    Both are recorded beside the outcome -- the arm in the trial's `active`
    record, the deployment digest in its `started` one -- so a block is chosen
    from what the campaign wrote, not guessed from a trace.
    """
    found = []
    for path in sorted(experiment.glob("reports/trials-*/*-outcome.json")):
        outcome = json.loads(path.read_text())["outcome"]
        index = path.name.split("-")[0]
        started = path.parent / f"{index}-started.json"
        active = path.parent / f"{index}-active.json"
        outcome["_digest"] = (
            json.loads(started.read_text())["trial"].get("model_digest", "")
            if started.exists()
            else ""
        )
        outcome["_arm"] = (
            json.loads(active.read_text()).get("arm") if active.exists() else None
        )
        found.append(outcome)
    return found


def facts(outcome: dict) -> dict:
    """What one trial's trace records about the quantities the repairs move."""
    counts = {
        "actions": 0,
        "denials": 0,
        "denial_args_repeat": 0,
        "denial_missing_max_matches": 0,
        "denial_find_absent": 0,
        "denial_command_not_allowlisted": 0,
        "turns": 0,
        "turns_filling_window": 0,
        "thinking_only": 0,
        "malformed": 0,
        "compactions": 0,
        "rereads_before_compaction": 0,
        "rereads_after_compaction": 0,
        "actions_before_compaction": 0,
        "actions_after_compaction": 0,
        "normalized_edits": 0,
    }
    trace = outcome.get("event_trace")
    if not trace or not Path(trace).exists():
        return counts
    compacted = False
    for line in Path(trace).read_text().splitlines():
        event = json.loads(line)
        kind = event.get("event_type")
        payload = event.get("payload") or {}
        if kind == "context.compacted":
            counts["compactions"] += 1
            compacted = True
        elif kind == "turn.generated":
            counts["turns"] += 1
            delivery = payload.get("prompt_delivery") or {}
            reported = delivery.get("reported_prompt_tokens")
            window = delivery.get("authorised_context_tokens")
            if reported and window and reported >= 0.98 * window:
                counts["turns_filling_window"] += 1
        elif kind == "action.malformed":
            counts["malformed"] += 1
            if payload.get("kind") == "thinking_only":
                counts["thinking_only"] += 1
        elif kind == "tool.action":
            counts["actions"] += 1
            result = payload.get("outcome")
            if isinstance(result, dict):
                if result.get("already_read"):
                    key = "rereads_after_compaction" if compacted else "rereads_before_compaction"
                    counts[key] += 1
                if result.get("normalized"):
                    counts["normalized_edits"] += 1
            counts["actions_after_compaction" if compacted else "actions_before_compaction"] += 1
            denial = payload.get("denial")
            if isinstance(denial, str):
                counts["denials"] += 1
                if "args must not repeat" in denial:
                    counts["denial_args_repeat"] += 1
                elif "max_matches" in denial:
                    counts["denial_missing_max_matches"] += 1
                elif "find text does not appear" in denial:
                    counts["denial_find_absent"] += 1
                elif "is not allowlisted" in denial:
                    counts["denial_command_not_allowlisted"] += 1
    return counts


def totals(these: list[dict]) -> dict:
    summed = Counter()
    for outcome in these:
        summed.update(facts(outcome))
        summed["trials"] += 1
        summed["resolved"] += bool(outcome.get("declared_complete") and outcome.get("hidden_verifier_passed"))
        summed["hidden_passed"] += bool(outcome.get("hidden_verifier_passed"))
        summed["generated_tokens"] += outcome.get("generated_tokens") or 0
        summed["seconds"] += outcome.get("duration_secs") or 0
    return dict(summed)


def markdown(treatment: list[dict], control: list[dict], names: tuple[str, str]) -> str:
    left, right = totals(treatment), totals(control)
    shared = sorted({o["task_id"] for o in treatment} & {o["task_id"] for o in control})
    by_task = {
        "treatment": {o["task_id"]: o for o in treatment},
        "control": {o["task_id"]: o for o in control},
    }

    def passed(side: str, task: str) -> bool:
        outcome = by_task[side][task]
        return bool(outcome.get("hidden_verifier_passed"))

    only_new = [t for t in shared if passed("treatment", t) and not passed("control", t)]
    only_old = [t for t in shared if passed("control", t) and not passed("treatment", t)]
    lines = [
        "# What the post-freeze repairs are worth",
        "",
        f"Treatment `{names[0]}` against control `{names[1]}`, paired on {len(shared)} tasks.",
        "Preregistered in `protocol.md`; generated by `scripts/r2_harness_revision_analysis.py`.",
        "",
        "## Primary outcome, paired by task",
        "",
        f"- hidden check passed: **{sum(passed('treatment', t) for t in shared)}** with the repairs, "
        f"**{sum(passed('control', t) for t in shared)}** without.",
        f"- resolved (declared and accepted): {left.get('resolved', 0)} against {right.get('resolved', 0)}.",
        f"- only the repaired binary: {', '.join(f'`{t}`' for t in only_new) or 'none'}.",
        f"- only the recorded one: {', '.join(f'`{t}`' for t in only_old) or 'none'}.",
        f"- sign test on the {len(only_new) + len(only_old)} discordant pairs: p = "
        f"{sign_test(len(only_new), len(only_old)):.3f}.",
        "",
        "The variance measurement puts a quarter of tasks as seed-dependent, so this is",
        "reported and not read as an effect unless the discordance is large and one-sided.",
        "",
        "## Secondary outcomes, per action and per turn",
        "",
        "| | repaired | recorded |",
        "|---|---:|---:|",
    ]
    rows = [
        ("trials", "trials"),
        ("actions", "actions"),
        ("refusals, all causes", "denials"),
        ("… `args` repeating the program (D5)", "denial_args_repeat"),
        ("… `search` without `max_matches` (D5)", "denial_missing_max_matches"),
        ("… `find` text not present (D7 targets the escaped case)", "denial_find_absent"),
        ("… command not allowlisted (D8 names the tool)", "denial_command_not_allowlisted"),
        ("edits applied after decoding escapes (D7)", "normalized_edits"),
        ("turns", "turns"),
        ("… filling 98% of the window (D6)", "turns_filling_window"),
        ("… returning reasoning with no call", "thinking_only"),
        ("malformed calls, all kinds", "malformed"),
        ("compactions", "compactions"),
        ("re-reads before the first compaction", "rereads_before_compaction"),
        ("actions before it", "actions_before_compaction"),
        ("re-reads after it", "rereads_after_compaction"),
        ("actions after it", "actions_after_compaction"),
        ("generated tokens", "generated_tokens"),
    ]
    for label, key in rows:
        lines.append(f"| {label} | {left.get(key, 0):,} | {right.get(key, 0):,} |")
    lines.append(f"| minutes | {left.get('seconds', 0) / 60:,.0f} | {right.get('seconds', 0) / 60:,.0f} |")

    def share(side: dict, part: str, whole: str) -> str:
        bottom = side.get(whole, 0)
        return f"{side.get(part, 0) / bottom:.0%}" if bottom else "-"

    lines += [
        "",
        "Re-read share, which is what H2 exists to move:",
        "",
        "| | repaired | recorded |",
        "|---|---:|---:|",
        f"| before the first compaction | {share(left, 'rereads_before_compaction', 'actions_before_compaction')} "
        f"| {share(right, 'rereads_before_compaction', 'actions_before_compaction')} |",
        f"| after it | {share(left, 'rereads_after_compaction', 'actions_after_compaction')} "
        f"| {share(right, 'rereads_after_compaction', 'actions_after_compaction')} |",
        "",
    ]
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("treatment", type=Path)
    parser.add_argument("control", type=Path)
    parser.add_argument("--write", action="store_true")
    arguments = parser.parse_args()
    treatment = [o for o in trials(arguments.treatment) if o["_arm"] == "poor_ai"]
    if not treatment:
        raise SystemExit("the treatment campaign has no B1 trials yet")
    # The control campaign holds every arm and both deployments. The block that
    # pairs with this one is the same arm, the same deployment and seed 1.
    digests = {o["_digest"] for o in treatment}
    control = [
        o
        for o in trials(arguments.control)
        if o["_arm"] == "poor_ai" and o["_digest"] in digests and o.get("seed") == 1
    ]
    tasks = {o["task_id"] for o in treatment}
    control = [o for o in control if o["task_id"] in tasks]
    text = markdown(treatment, control, (arguments.treatment.name, arguments.control.name))
    if arguments.write:
        (arguments.treatment / "analysis.md").write_text(text)
        print(f"wrote {arguments.treatment / 'analysis.md'}")
    else:
        print(text)


if __name__ == "__main__":
    main()
