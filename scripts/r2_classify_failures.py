#!/usr/bin/env python3
"""Draft R2 failure classes from retained per-trial outcomes and traces.

This is a review aid, not the final R2 adjudicator. The pilot protocol requires
unresolved trials to be classified from their trace into exactly one class, in a
fixed order of precedence, with every `unknown` read a second time. This script
applies that order with rules that are deterministic on the retained artifacts:
the per-trial outcome, the raw event trace it names, and the corpus task. Rows
whose class rests on a heuristic rather than a recorded fact are marked for
trace review.

Conditions (arm, oracle context, deployment) come from the completed suite
report the campaign response names for each manifest. Older manifests carry
none of them, and an experiment that runs one suite under several arms has
several reports per suite, so matching by suite alone cannot tell them apart.
The manifest stays authoritative for assignment and reconciliation.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re
from typing import Any


ROOT = Path(__file__).resolve().parent.parent

CORPORA = {
    "external-v1": ROOT / "corpus/external-v1.json",
    "external-v2": ROOT / "corpus/external-v2.json",
    "m6-hard-v1": ROOT / "corpus/m6-hard-v1.json",
    "longhaul-v1": ROOT / "corpus/longhaul-v1.json",
    "realistic-v1": ROOT / "corpus/realistic-v1.json",
}

# The protocol's order of precedence. A trial takes the first class that holds.
PRECEDENCE = [
    "harness",
    "provider",
    "protocol",
    "localization",
    "wrong_change",
    "unfinished",
    "scope",
    "declined",
    "unknown",
]

AVOIDABLE_FOR_R3 = {"harness", "protocol", "localization", "unfinished"}

EDIT_CAPABILITIES = {
    "replace_text",
    "apply_replace",
    "apply_patch",
    "write_file",
    "delete_path",
    "move_path",
}

ARM_LABELS = {"poor_ai": "b1", "conventional": "b0", "staged": "b2"}


def read_json(path: Path) -> Any:
    return json.loads(path.read_text())


def rel(path: Path | str | None) -> str | None:
    if path is None:
        return None
    p = Path(path)
    try:
        return str(p.resolve().relative_to(ROOT))
    except (OSError, ValueError):
        return str(path)


def suite_tasks(suite: str | None) -> dict[str, dict[str, Any]]:
    path = CORPORA.get(suite or "")
    if path is None or not path.exists():
        return {}
    data = read_json(path)
    return {task["id"]: task for task in data.get("tasks", [])}


def report_conditions(data: dict[str, Any]) -> dict[str, Any]:
    return {
        "arm": data.get("arm"),
        "mode": data.get("mode"),
        "oracle_context": data.get("oracle_context"),
        "model_digest": data.get("model_digest"),
    }


def campaign_responses(reports: Path) -> list[dict[str, Any]]:
    """Every CLI response the runner appended, decoded from a concatenated stream."""
    source = reports / "campaigns.jsonl"
    if not source.exists():
        return []
    text = source.read_text()
    decoder = json.JSONDecoder()
    found = []
    index = 0
    while True:
        start = text.find("{", index)
        if start < 0:
            break
        try:
            value, end = decoder.raw_decode(text, start)
        except ValueError:
            index = start + 1
            continue
        index = end
        if isinstance(value, dict):
            found.append(value)
    return found


def conditions_by_campaign(reports: Path) -> dict[str, dict[str, Any]]:
    """Conditions of each campaign, from the report its own response names."""
    linked: dict[str, dict[str, Any]] = {}
    for response in campaign_responses(reports):
        result = response.get("result") or {}
        manifest = (result.get("trials") or {}).get("manifest")
        report = result.get("report_json")
        if not manifest or not report:
            continue
        report_path = reports / Path(report).name
        try:
            data = read_json(report_path)
        except (OSError, ValueError):
            continue
        campaign = Path(manifest).stem.removeprefix("manifest-")
        linked[campaign] = report_conditions(data)
    return linked


def suite_report_conditions(reports: Path) -> dict[str, dict[str, Any]]:
    """Condition fields from a unique completed report for each suite.

    The fallback for campaigns no response links: exactly one completed report
    for the suite, or nothing.
    """
    by_suite: dict[str, list[dict[str, Any]]] = {}
    for path in sorted(reports.glob("*.json")):
        if path.name.startswith(("manifest-", "evaluation-run-")):
            continue
        if path.name in {"last-campaign.json", "status.json"}:
            continue
        try:
            data = read_json(path)
        except (OSError, ValueError):
            continue
        if not isinstance(data, dict) or "outcomes" not in data:
            continue
        suite = data.get("suite")
        if isinstance(suite, str):
            by_suite.setdefault(suite, []).append(report_conditions(data))
    return {
        suite: candidates[0]
        for suite, candidates in by_suite.items()
        if len(candidates) == 1
    }


def condition_value(
    manifest: dict[str, Any], completed_report: dict[str, Any], field: str
) -> Any:
    value = manifest.get(field)
    if value is not None:
        return value
    return completed_report.get(field)


def condition_label(arm: Any, oracle_context: Any) -> str:
    label = ARM_LABELS.get(str(arm), str(arm) if arm else "unknown")
    return f"{label}+oracle" if oracle_context else label


def deployment_label(digest: Any) -> str:
    if not isinstance(digest, str) or not digest:
        return "unknown"
    return digest.removeprefix("lmstudio-variant:")[:12]


def outcome_resolved(outcome: dict[str, Any]) -> bool:
    """`TaskOutcome::resolved` in `pwr-eval`, for the kinds the pilot runs."""
    if outcome.get("kind") == "repository_question":
        return bool(
            outcome.get("declared_complete")
            and outcome.get("answer_matched") is True
            and not outcome.get("changed_files")
        )
    return bool(
        outcome.get("declared_complete")
        and outcome.get("hidden_verifier_passed")
        and not outcome.get("out_of_scope_changes")
    )


def trace_facts(outcome: dict[str, Any], allowed: set[str], expected_file: str | None) -> dict[str, Any] | None:
    """What the raw event trace records, or None when it was not retained."""
    path = outcome.get("event_trace")
    if not path or not Path(path).exists():
        return None
    facts: dict[str, Any] = {
        "withheld_as_unrunnable": False,
        "state_directory_denied": False,
        "read_allowed": False,
        "edited_allowed": False,
        "read_expected": False,
        "rereads": 0,
        "compactions": 0,
        "actions": 0,
        "served_window": None,
        # D6: turns whose prompt reached the window with nothing left to
        # generate with, and the turns that then produced only reasoning.
        "turns": 0,
        "turns_without_room": 0,
        "thinking_only": 0,
    }
    for line in Path(path).read_text().splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        kind = event.get("event_type")
        payload = event.get("payload") or {}
        if kind == "task.transition":
            if "failed to run in this environment" in str(payload.get("detail") or ""):
                facts["withheld_as_unrunnable"] = True
        elif kind == "context.compacted":
            facts["compactions"] += 1
        elif kind == "turn.generated":
            facts["turns"] += 1
            delivery = payload.get("prompt_delivery") or {}
            reported = delivery.get("reported_prompt_tokens")
            window = delivery.get("authorised_context_tokens")
            # The reserve the prompt compiler holds back for the reply. A
            # prompt inside it leaves the deployment nowhere to answer.
            if reported and window and reported + window * 0.25 > window:
                facts["turns_without_room"] += 1
        elif kind == "action.malformed":
            if payload.get("kind") == "thinking_only":
                facts["thinking_only"] += 1
        elif kind in {"verification.baseline", "verification.result", "tool.action", "task.failed"}:
            text = json.dumps(payload)
            if "scandir" in text and ".poorai" in text or "testFiles.length" in text:
                facts["state_directory_denied"] = True
            served = re.search(r"available context size \((\d+) tokens\)", text)
            if served:
                facts["served_window"] = int(served.group(1))
        if kind != "tool.action":
            continue
        facts["actions"] += 1
        action = payload.get("action") or {}
        capability = action.get("capability")
        target = action.get("path") or action.get("to") or action.get("from")
        result = payload.get("outcome")
        if isinstance(result, dict) and result.get("already_read"):
            facts["rereads"] += 1
        if capability == "read_file" and isinstance(target, str):
            if target in allowed:
                facts["read_allowed"] = True
            if expected_file and target.endswith(expected_file):
                facts["read_expected"] = True
        if capability == "search" and isinstance(result, dict):
            for hit in result.get("files") or []:
                hit_path = str(hit.get("path") or "")
                if hit_path in allowed:
                    facts["read_allowed"] = True
                if expected_file and hit_path.endswith(expected_file):
                    facts["read_expected"] = True
        if capability in EDIT_CAPABILITIES and payload.get("status") == "allowed":
            if isinstance(target, str) and target in allowed:
                facts["edited_allowed"] = True
            patch = action.get("patch")
            if isinstance(patch, str) and any(
                f"b/{name}" in patch or f" {name}" in patch for name in allowed
            ):
                facts["edited_allowed"] = True
    return facts


def expected_file_of(task: dict[str, Any] | None) -> str | None:
    expected = (task or {}).get("expected_in_rationale")
    terms = expected if isinstance(expected, list) else [expected] if isinstance(expected, str) else []
    for term in terms:
        if isinstance(term, str) and "." in term:
            return term
    return None


def classify(
    outcome: dict[str, Any], task: dict[str, Any] | None
) -> tuple[str, str, bool, dict[str, Any] | None]:
    """Return (class, rationale, needs_trace_review, trace facts)."""
    allowed = set((task or {}).get("allowed_files") or [])
    expected_file = expected_file_of(task)
    facts = trace_facts(outcome, allowed, expected_file)
    if outcome_resolved(outcome):
        return "resolved", "declared complete and accepted by the hidden check or answer rubric", False, facts

    kind = outcome.get("kind") or (task or {}).get("kind")
    question = kind == "repository_question"
    terminal = str(outcome.get("terminal") or "").lower()
    error = str(outcome.get("error") or "")
    changed = {
        path for path in outcome.get("changed_files") or [] if "__pycache__" not in path
    }
    out_of_scope = outcome.get("out_of_scope_changes") or []
    declared = bool(outcome.get("declared_complete"))
    hidden = bool(outcome.get("hidden_verifier_passed"))
    answered = outcome.get("answer_matched") is True
    pinned_upstream = bool((task or {}).get("repository"))

    # 1. harness
    if facts and facts["withheld_as_unrunnable"] and not out_of_scope and (
        (question and answered and not changed) or (not question and hidden)
    ):
        return (
            "harness",
            "correct work completed as unverifiable: a check that ran and failed at baseline was classified unrunnable",
            False,
            facts,
        )
    if pinned_upstream and outcome.get("visible_verifier_passed_before") is False:
        evidence = (
            "; the trace shows the check stopped on the harness state directory"
            if facts and facts["state_directory_denied"]
            else ""
        )
        return (
            "harness",
            "the visible check `check-corpus` certifies at this commit failed before the run acted" + evidence,
            not (facts and facts["state_directory_denied"]),
            facts,
        )
    # D6, 2026-09-16: the prompt budget was enforced on a character estimate
    # the backend's own count exceeded by a median 1.37x, so prompts arrived
    # filling the window and the turn came back as reasoning with no call in
    # it. The trial ends `protocol`, which reads as the deployment failing to
    # emit usable calls; the cause is the harness, and it falls hardest on the
    # arm whose prompts are largest. Fixed in source after this rerun was
    # frozen, so these trials belong to the harness, not to the arm.
    if (
        facts
        and facts["thinking_only"]
        and facts["turns_without_room"]
        and terminal in {"protocol", "unclassified"}
    ):
        return (
            "harness",
            f"no room to reply: {facts['turns_without_room']} of {facts['turns']} turns filled the "
            f"window and {facts['thinking_only']} returned reasoning with no call (D6)",
            False,
            facts,
        )
    recorded_window = outcome.get("context_tokens")
    if facts and facts["served_window"] and recorded_window and facts["served_window"] < recorded_window:
        return (
            "harness",
            f"served a {facts['served_window']}-token window while {recorded_window} was recorded",
            False,
            facts,
        )

    # 2. provider
    if outcome.get("provider_failure") or outcome.get("timed_out") or terminal == "provider":
        return "provider", "provider failure, timeout or refused context recorded", False, facts

    # 3. protocol
    if terminal in {"protocol", "unclassified"} or "unusable replies" in error:
        return "protocol", "the run ended on malformed calls or unusable replies", False, facts

    # 4. localization
    if question:
        if declared and not answered:
            return "localization", "the declared answer did not name the file and function", True, facts
        if facts and expected_file and not facts["read_expected"]:
            return "localization", f"never read or searched into {expected_file}", True, facts
    elif allowed:
        # Only when nothing shows the right place was found. A run that read
        # the right file and wrote a scratch script beside it changed files
        # outside the allowed set without having looked in the wrong place.
        if facts and not facts["read_allowed"] and not facts["edited_allowed"] and not (changed & allowed):
            return "localization", "no file the change belongs in was ever read or edited", True, facts
        if not facts and changed and changed.isdisjoint(allowed) and not hidden:
            return "localization", "changed files, none of them the files the change belongs in", True, facts

    # 5. wrong_change
    edited_allowed = bool(changed & allowed) or bool(facts and facts["edited_allowed"])
    if not question and edited_allowed and not hidden:
        return "wrong_change", "edited the right place and the hidden check fails", True, facts

    # 6. unfinished
    if not declared:
        if terminal == "recovery":
            why = "stopped after repeated failed repairs"
        elif hidden or (question and answered):
            why = "the work was right but no completion was accepted before the budget ran out"
        else:
            why = "the budget ran out before a completion was declared"
        if facts and facts["actions"]:
            why += f" ({facts['rereads']} of {facts['actions']} actions re-read an unchanged file; {facts['compactions']} compactions)"
        return "unfinished", why, False, facts

    # 7. scope
    if out_of_scope or (question and changed):
        return "scope", "changed outside allowed files: " + ", ".join(out_of_scope or sorted(changed)), False, facts

    # 8. declined
    if outcome.get("declined"):
        return "declined", "the deployment refused a legitimate task", True, facts

    return "unknown", "no rule establishes a class from the retained artifacts", True, facts


def build(experiment: Path) -> dict[str, Any]:
    reports = experiment / "reports"
    linked = conditions_by_campaign(reports)
    by_suite = suite_report_conditions(reports)
    rows: list[dict[str, Any]] = []
    for manifest_path in sorted(reports.glob("manifest-*.json")):
        manifest = read_json(manifest_path)
        suite = manifest.get("suite")
        campaign = manifest["campaign"]
        completed_report = linked.get(campaign) or by_suite.get(suite or "", {})
        conditions_from = (
            "campaign response" if campaign in linked else "unique suite report" if completed_report else None
        )
        tasks = suite_tasks(suite)
        trial_dir = manifest_path.parent / f"trials-{campaign}"
        arm = condition_value(manifest, completed_report, "arm")
        oracle = condition_value(manifest, completed_report, "oracle_context")
        digest = manifest.get("model_digest") or completed_report.get("model_digest")
        for outcome_path in sorted(trial_dir.glob("*-outcome.json")):
            data = read_json(outcome_path)
            outcome = data.get("outcome") or {}
            trial = data.get("trial") or {}
            task_id = trial.get("task_id") or outcome.get("task_id")
            klass, rationale, needs_review, facts = classify(outcome, tasks.get(task_id or ""))
            rows.append(
                {
                    "campaign": campaign,
                    "suite": suite,
                    "arm": arm,
                    "mode": condition_value(manifest, completed_report, "mode"),
                    "oracle_context": oracle,
                    "condition": condition_label(arm, oracle),
                    "deployment": deployment_label(digest),
                    "conditions_from": conditions_from,
                    "index": data.get("index"),
                    "task_id": task_id,
                    "seed": trial.get("seed") or outcome.get("seed"),
                    "class": klass,
                    "rationale": rationale,
                    "needs_trace_review": needs_review,
                    "trace": rel(outcome.get("event_trace")),
                    "trace_facts": facts,
                    "out_of_scope_changes": outcome.get("out_of_scope_changes") or [],
                    "declared_complete": outcome.get("declared_complete", False),
                    "hidden_verifier_passed": outcome.get("hidden_verifier_passed", False),
                    "terminal": outcome.get("terminal"),
                    "error": outcome.get("error"),
                }
            )

    counts = Counter(row["class"] for row in rows if row["class"] != "resolved")
    by_condition: dict[str, dict[str, Counter]] = {}
    for row in rows:
        cell = by_condition.setdefault(row["condition"], {}).setdefault(row["deployment"], Counter())
        cell[row["class"]] += 1

    # The choice rule reads B1, summed over deployments. Primary trials are the
    # seed-1 ones; the variance seeds are reported beside them, not added in.
    b1_primary = [row for row in rows if row["condition"] == "b1" and row["seed"] == 1]
    avoidable = Counter(
        row["class"] for row in b1_primary if row["class"] in AVOIDABLE_FOR_R3
    )
    ranked = sorted(((count, klass) for klass, count in avoidable.items()), reverse=True)
    tested = [(count, klass) for count, klass in ranked if klass != "harness"]
    return {
        "experiment": rel(experiment),
        "rows": rows,
        "counts_unresolved": dict(sorted(counts.items())),
        "counts_by_condition": {
            condition: {deployment: dict(sorted(cell.items())) for deployment, cell in sorted(cells.items())}
            for condition, cells in sorted(by_condition.items())
        },
        "b1_primary_avoidable": dict(sorted(avoidable.items())),
        "largest_avoidable_b1_class": ranked[0][1] if ranked else None,
        # `harness` is fixed rather than tested, so what R3 would test is the
        # largest of the others.
        "largest_testable_b1_class": tested[0][1] if tested else None,
        "needs_trace_review": sum(1 for row in rows if row["needs_trace_review"]),
    }


def write_markdown(path: Path, report: dict[str, Any]) -> None:
    lines = [
        "# R2 draft failure classification",
        "",
        "Generated by `scripts/r2_classify_failures.py` from retained outcomes and event traces, in the protocol's order of precedence. Rows marked for trace review rest on a heuristic and are not final adjudications.",
        "",
        "## Unresolved trials by class",
        "",
        "| Class | Count |",
        "|---|---:|",
    ]
    for klass in PRECEDENCE:
        if klass in report["counts_unresolved"]:
            lines.append(f"| `{klass}` | {report['counts_unresolved'][klass]} |")
    lines += ["", "## By condition and deployment", ""]
    header = ["Condition", "Deployment", "resolved"] + PRECEDENCE
    lines.append("| " + " | ".join(header) + " |")
    lines.append("|---|---|" + "---:|" * (len(header) - 2))
    for condition, cells in report["counts_by_condition"].items():
        for deployment, cell in cells.items():
            values = [str(cell.get(klass, 0)) for klass in ["resolved"] + PRECEDENCE]
            lines.append(f"| `{condition}` | `{deployment}` | " + " | ".join(values) + " |")
    lines += [
        "",
        "## Choice rule inputs",
        "",
        f"B1 primary trials, avoidable classes summed over deployments: `{json.dumps(report['b1_primary_avoidable'])}`.",
        f"Largest avoidable class: `{report['largest_avoidable_b1_class'] or '-'}`. Largest class R3 could test (excluding `harness`, which is fixed): `{report['largest_testable_b1_class'] or '-'}`.",
        "",
        "## Rows",
        "",
        "| Condition | Deployment | Task | Seed | Class | Review | Rationale |",
        "|---|---|---|---:|---|---|---|",
    ]
    for row in report["rows"]:
        lines.append(
            f"| `{row['condition']}` | `{row['deployment']}` | `{row['task_id']}` | {row['seed']} | `{row['class']}` | {row['needs_trace_review']} | {row['rationale']} |"
        )
    path.write_text("\n".join(lines) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("experiment", type=Path)
    parser.add_argument("--no-write", action="store_true")
    args = parser.parse_args()
    experiment = args.experiment.resolve()
    report = build(experiment)
    if args.no_write:
        print(json.dumps(report, indent=2))
        return
    (experiment / "failure-classes.json").write_text(json.dumps(report, indent=2) + "\n")
    write_markdown(experiment / "failure-classes.md", report)
    print(
        json.dumps(
            {key: value for key, value in report.items() if key != "rows"},
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
