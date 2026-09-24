#!/usr/bin/env python3
"""Summarize an R2 pilot directory without reading model traces.

The R2 runner deliberately writes several artifact layers:

- a campaign manifest before trials start;
- per-trial `started` and `outcome` files as the campaign crosses those
  boundaries;
- a CLI wrapper response for a completed campaign;
- the final SuiteReport named by that wrapper.

This script keeps those layers separate. It is a progress/accounting view, not a
task-quality classifier and not a replacement for the preregistered R2 failure
taxonomy review.

For older manifests that predate allocation-level conditions, a unique completed
suite report may provide the displayed condition fields. The manifest remains
authoritative for assignment and reconciliation; the fallback only repairs the
human-readable status view.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import UTC, datetime
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_EXPERIMENT = ROOT / "experiments/r2-pilot-20260913-b5bd562"


def read_json(path: Path) -> Any:
    return json.loads(path.read_text())


def maybe_read_json(path: Path) -> Any | None:
    try:
        if not path.exists() or path.stat().st_size == 0:
            return None
        return read_json(path)
    except (OSError, ValueError):
        return None


def rel(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def parse_instant(value: Any) -> datetime | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def elapsed_secs_since(value: Any) -> float | None:
    instant = parse_instant(value)
    if instant is None:
        return None
    if instant.tzinfo is None:
        instant = instant.replace(tzinfo=UTC)
    return max(0.0, (datetime.now(UTC) - instant.astimezone(UTC)).total_seconds())


def campaign_responses(reports: Path) -> list[dict[str, Any]]:
    found: list[dict[str, Any]] = []
    for source in [reports / "campaigns.jsonl", reports / "last-campaign.json"]:
        if not source.exists() or source.stat().st_size == 0:
            continue
        if source.name.endswith(".jsonl"):
            decoder = json.JSONDecoder()
            text = source.read_text()
            offset = 0
            while offset < len(text):
                start = text.find("{", offset)
                if start < 0:
                    break
                try:
                    value, end = decoder.raw_decode(text[start:])
                except ValueError:
                    found.append({"source": rel(source), "parse_error": text[start : start + 240]})
                    break
                if isinstance(value, dict):
                    value = dict(value)
                    value["source"] = rel(source)
                    found.append(value)
                offset = start + end
        else:
            value = maybe_read_json(source)
            if isinstance(value, dict):
                value = dict(value)
                value["source"] = rel(source)
                found.append(value)
    return found


def suite_reports(reports: Path) -> list[dict[str, Any]]:
    found: list[dict[str, Any]] = []
    for path in sorted(reports.glob("*.json")):
        if path.name.startswith(("manifest-", "evaluation-run-")):
            continue
        if path.name in {"last-campaign.json", "status.json"}:
            continue
        data = maybe_read_json(path)
        if not isinstance(data, dict) or "outcomes" not in data:
            continue
        found.append(
            {
                "path": rel(path),
                "suite": data.get("suite"),
                "arm": data.get("arm"),
                "mode": data.get("mode"),
                "oracle_context": data.get("oracle_context", False),
                "model_digest": data.get("model_digest"),
                "outcomes": len(data.get("outcomes") or []),
            }
        )
    return found


def campaign_status(manifest_path: Path) -> dict[str, Any]:
    manifest = read_json(manifest_path)
    campaign = manifest["campaign"]
    trial_dir = manifest_path.parent / f"trials-{campaign}"
    started = sorted(trial_dir.glob("*-started.json")) if trial_dir.exists() else []
    active = sorted(trial_dir.glob("*-active.json")) if trial_dir.exists() else []
    outcomes = sorted(trial_dir.glob("*-outcome.json")) if trial_dir.exists() else []
    outcome_by_index: dict[int, dict[str, Any]] = {}
    started_by_index: dict[int, dict[str, Any]] = {}
    active_by_index: dict[int, dict[str, Any]] = {}
    outcome_rows: list[dict[str, Any]] = []
    for path in outcomes:
        data = maybe_read_json(path)
        if isinstance(data, dict) and isinstance(data.get("index"), int):
            outcome_by_index[data["index"]] = data
            outcome = data.get("outcome") or {}
            trial = data.get("trial") or {}
            outcome_rows.append(
                {
                    "index": data["index"],
                    "task_id": trial.get("task_id") or outcome.get("task_id"),
                    "completed_at": data.get("completed_at"),
                    "event_trace": outcome.get("event_trace"),
                    "declared_complete": outcome.get("declared_complete", False),
                    "hidden_verifier_passed": outcome.get("hidden_verifier_passed", False),
                    "timed_out": outcome.get("timed_out", False),
                    "provider_failure": outcome.get("provider_failure", False),
                    "declined": outcome.get("declined", False),
                    "out_of_scope_changes": outcome.get("out_of_scope_changes") or [],
                    "terminal": outcome.get("terminal"),
                    "error": outcome.get("error"),
                    "turns": outcome.get("turns"),
                    "tool_attempts": outcome.get("tool_attempts"),
                    "duration_secs": outcome.get("duration_secs"),
                }
            )
    assigned = manifest.get("assigned") or []
    started_indexes = []
    for path in started:
        data = maybe_read_json(path)
        if isinstance(data, dict) and isinstance(data.get("index"), int):
            started_indexes.append(data["index"])
            started_by_index[data["index"]] = data
    for path in active:
        data = maybe_read_json(path)
        if not isinstance(data, dict):
            continue
        index = None
        try:
            index = int(path.name.split("-", 1)[0])
        except (TypeError, ValueError):
            pass
        if index is not None:
            active_by_index[index] = data
    started_indexes.sort()
    completed_indexes = sorted(outcome_by_index)
    active_indexes = [index for index in started_indexes if index not in outcome_by_index]
    active_rows = []
    for index in active_indexes:
        started_data = started_by_index.get(index) or {}
        active_data = active_by_index.get(index) or {}
        trial = started_data.get("trial") or {}
        started_at = started_data.get("started_at")
        entered = active_data.get("entered_action_loop_at")
        active_rows.append(
            {
                "index": index,
                "task_id": trial.get("task_id") or active_data.get("task_id"),
                "started_at": started_at,
                "entered_action_loop_at": entered,
                "age_secs": elapsed_secs_since(entered or started_at),
                "run_id": active_data.get("run_id"),
                "workspace_root": active_data.get("workspace_root"),
                "time_budget_secs": active_data.get("time_budget_secs"),
                "max_actions": active_data.get("max_actions"),
                "state": "active" if active_data else "started_without_active_artifact",
                "active_artifact": rel(trial_dir / f"{index:04d}-active.json") if active_data else None,
            }
        )
    return {
        "campaign": campaign,
        "suite": manifest.get("suite"),
        "mode": manifest.get("mode"),
        "arm": manifest.get("arm"),
        "oracle_context": manifest.get("oracle_context"),
        "turn_timeout_secs": manifest.get("turn_timeout_secs"),
        "harness_rev": manifest.get("harness_rev"),
        "deployment_fingerprint": manifest.get("deployment_fingerprint"),
        "execution_profile_id": manifest.get("execution_profile_id"),
        "created_at": manifest.get("created_at"),
        "manifest": rel(manifest_path),
        "trial_artifacts": rel(trial_dir),
        "assigned": len(assigned),
        "started": len(started_indexes),
        "outcomes": len(completed_indexes),
        "active_indexes": active_indexes,
        "active_entered_loop": len(active_by_index),
        "active_rows": active_rows,
        "unstarted": max(0, len(assigned) - len(started_indexes)),
        "complete_by_artifacts": len(assigned) > 0 and len(completed_indexes) == len(assigned),
        "outcome_rows": sorted(outcome_rows, key=lambda row: row["index"]),
    }


def build_status(experiment: Path) -> dict[str, Any]:
    reports = experiment / "reports"
    manifests = sorted(reports.glob("manifest-*.json")) if reports.exists() else []
    progress = (experiment / "progress.log").read_text().splitlines() if (experiment / "progress.log").exists() else []
    campaigns = [campaign_status(path) for path in manifests]
    responses = campaign_responses(reports)
    reports_found = suite_reports(reports)
    reports_by_suite: dict[str, list[dict[str, Any]]] = {}
    for report in reports_found:
        suite = report.get("suite")
        if isinstance(suite, str):
            reports_by_suite.setdefault(suite, []).append(report)
    for campaign in campaigns:
        candidates = reports_by_suite.get(campaign.get("suite"), [])
        if len(candidates) != 1:
            continue
        report = candidates[0]
        used_fallback = False
        for field in ("arm", "mode", "oracle_context"):
            if campaign.get(field) is None and field in report:
                campaign[field] = report[field]
                used_fallback = True
        if used_fallback:
            campaign["conditions_from_completed_report"] = True
    unfinished_trials = [
        {
            "campaign": campaign["campaign"],
            "suite": campaign["suite"],
            "manifest": campaign["manifest"],
            **row,
        }
        for campaign in campaigns
        for row in campaign["active_rows"]
    ]
    assigned = sum(c["assigned"] for c in campaigns)
    started = sum(c["started"] for c in campaigns)
    outcomes = sum(c["outcomes"] for c in campaigns)
    response_failures = [response for response in responses if response.get("ok") is False]
    failure_categories = Counter(
        (response.get("error") or {}).get("category") or "unknown"
        for response in response_failures
    )
    return {
        "experiment": rel(experiment),
        "progress_lines": progress,
        "campaigns": campaigns,
        "campaign_responses": responses,
        "suite_reports": reports_found,
        "unfinished_trials": unfinished_trials,
        "campaign_failure_categories": dict(sorted(failure_categories.items())),
        "totals": {
            "campaign_manifests": len(campaigns),
            "campaign_responses": len(responses),
            "campaign_failures": len(response_failures),
            "assigned": assigned,
            "started": started,
            "outcomes": outcomes,
            "unstarted": max(0, assigned - started),
            "active": sum(len(c["active_indexes"]) for c in campaigns),
            "completed_campaign_reports": len(reports_found),
        },
    }


def write_markdown(path: Path, status: dict[str, Any]) -> None:
    totals = status["totals"]
    lines = [
        "# R2 pilot status",
        "",
        "Generated from manifest, per-trial artifacts, campaign responses and completed campaign reports. It does not classify task failure causes.",
        "",
        "| Campaigns | Attempts | Failed | Assigned | Started | Outcomes | Active | Suite reports |",
        "|---:|---:|---:|---:|---:|---:|---:|---:|",
        f"| {totals['campaign_manifests']} | {totals['campaign_responses']} | {totals['campaign_failures']} | {totals['assigned']} | {totals['started']} | {totals['outcomes']} | {totals['active']} | {totals['completed_campaign_reports']} |",
        "",
        "## Campaign responses",
        "",
        "Failures here occur before a trial manifest can be created and remain separate from per-trial outcomes.",
        "",
        "| Failure category | Count |",
        "|---|---:|",
    ]
    for category, count in status["campaign_failure_categories"].items():
        lines.append(f"| `{category}` | {count} |")
    lines += ["", "## Campaigns", "", "| Campaign | Suite | Arm | Mode | Assigned | Started | Outcomes | Active indexes | Complete by artifacts |", "|---|---|---|---|---:|---:|---:|---|---|"]
    for campaign in status["campaigns"]:
        active = ", ".join(str(i) for i in campaign["active_indexes"]) or "-"
        lines.append(
            f"| `{campaign['campaign']}` | `{campaign['suite']}` | `{campaign.get('arm') or '-'}` | `{campaign.get('mode') or '-'}` | {campaign['assigned']} | {campaign['started']} | {campaign['outcomes']} | {active} | {campaign['complete_by_artifacts']} |"
        )
    lines += ["", "## Progress log", ""]
    lines.extend(f"- {line}" for line in status["progress_lines"][-20:])
    lines += ["", "## Active trial artifacts", ""]
    lines += [
        "| Campaign | Index | Task | State | Started | Entered loop | Age | Run | Budget |",
        "|---|---:|---|---|---|---|---:|---|---|",
    ]
    any_active = False
    for campaign in status["campaigns"]:
        for active in campaign["active_rows"]:
            any_active = True
            age = "-" if active["age_secs"] is None else f"{active['age_secs']:.0f}s"
            entered = active["entered_action_loop_at"] or "-"
            run = f"`{active['run_id']}`" if active["run_id"] else "-"
            budget = "-"
            if active["time_budget_secs"] is not None or active["max_actions"] is not None:
                budget = f"{active['time_budget_secs'] or '?'}s / {active['max_actions'] or '?'} actions"
            lines.append(
                f"| `{campaign['campaign']}` | {active['index']} | `{active['task_id']}` | `{active['state']}` | {active['started_at'] or '-'} | {entered} | {age} | {run} | {budget} |"
            )
    if not any_active:
        lines.append("| - | - | - | - | - | - | - | - | - |")
    lines += ["", "## Completed trial artifacts", ""]
    lines += [
        "| Campaign | Index | Task | Declared | Hidden | Scope | Timeout | Provider | Trace |",
        "|---|---:|---|---|---|---|---|---|---|",
    ]
    for campaign in status["campaigns"]:
        for outcome in campaign["outcome_rows"]:
            trace = outcome.get("event_trace")
            trace_cell = f"`{rel(Path(trace))}`" if trace else "-"
            scope = ", ".join(f"`{path}`" for path in outcome["out_of_scope_changes"]) or "ok"
            lines.append(
                f"| `{campaign['campaign']}` | {outcome['index']} | `{outcome['task_id']}` | {outcome['declared_complete']} | {outcome['hidden_verifier_passed']} | {scope} | {outcome['timed_out']} | {outcome['provider_failure']} | {trace_cell} |"
            )
    path.write_text("\n".join(lines) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("experiment", nargs="?", type=Path, default=DEFAULT_EXPERIMENT)
    parser.add_argument("--no-write", action="store_true", help="print JSON only")
    args = parser.parse_args()
    experiment = args.experiment.resolve()
    status = build_status(experiment)
    print(json.dumps(status, indent=2))
    if not args.no_write:
        (experiment / "status.json").write_text(json.dumps(status, indent=2) + "\n")
        write_markdown(experiment / "status.md", status)
        (experiment / "unfinished-trials.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "generated_at": datetime.now(UTC).isoformat().replace("+00:00", "Z"),
                    "experiment": rel(experiment),
                    "trials": status["unfinished_trials"],
                },
                indent=2,
            )
            + "\n"
        )


if __name__ == "__main__":
    main()
