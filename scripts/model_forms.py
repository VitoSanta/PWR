#!/usr/bin/env python3
"""Which forms each model gets wrong, read from the audits (backlog C.24).

    python3 scripts/model_forms.py [ROOT ...]        # default: the current directory

Walks every `.poorai/state.sqlite` under the given roots and groups the refused
calls (`action.malformed`) and the failed generations (`turn.failed`) by model
and by kind, next to how many generations each model produced. Each kind is
marked with what the harness does about it today:

- `repaired` -- rewritten into the form the tool takes, in `repair_form` or
  `merged_replacements` (crates/pwr-orchestrator/src/lib.rs), so a model
  making it no longer loses a turn;
- `refused` -- refused with a message saying how to write it, because it has
  more than one reading (repairing it would be a guess);
- `engine` -- produced below the harness (an unreadable reply, a reply cut off).

The model comes from `run.started`'s `model_ref` (recorded since 2026-09-23);
older runs carry only a digest, which is resolved through the inspection
artifacts in `.poorai/models` found under the same roots, and shown as a
digest when none is.
"""
import collections
import glob
import json
import os
import sqlite3
import sys

REPAIRED_PROBLEMS = (
    # repair_form
    ("schema_mismatch", "apply_replace", "missing field `replacement`"),
    ("schema_mismatch", "replace_text", "missing field `replace`"),
    ("schema_mismatch", "move_path", "missing field `to`"),
    ("schema_mismatch", "run_command", "missing field `args`"),
    # the tool itself was renamed: `apply_patch` is its name since 2026-09-07
    ("schema_mismatch", "apply_patch", "unknown variant `apply_patch`"),
    # list_tree takes `path` and defaults `max_entries` since 2026-09-23
    ("schema_mismatch", "list_tree", "missing field `max_entries`"),
)
ENGINE_KINDS = {"unparsed_output", "runaway_reply", "backend_fault", "context_limit", "cancelled"}


def treatment(kind, problem, detail):
    if kind in ENGINE_KINDS:
        return "engine"
    for repaired_kind, tool, marker in REPAIRED_PROBLEMS:
        if kind == repaired_kind and f"tool call {tool} " in problem and marker in problem:
            return "repaired"
    if kind == "multiple_calls":
        calls = [part.strip() for part in str(detail).split(",")]
        edits = [c for c in calls if c.startswith("replace_text(")]
        if edits and len(edits) == len(calls) and len({c for c in calls}) == 1:
            return "repaired (one patch when the hashes match)"
        if all(c.split("(")[0] in {"read_file", "list_tree", "search", "find_definition"} for c in calls):
            return "accepted (reads batch)"
    return "refused"


def digest_names(roots):
    names = {}
    for root in roots:
        for path in glob.glob(os.path.join(root, "**/.poorai/models/*.json"), recursive=True):
            try:
                artifact = json.load(open(path))
                names[artifact["definition"]["digest"]] = artifact["deployment"]["model_ref"]
            except (OSError, ValueError, KeyError, TypeError):
                continue
    return names


def main(roots):
    names = digest_names(roots)
    generations = collections.Counter()
    faults = collections.defaultdict(collections.Counter)
    example = {}
    for root in roots:
        for db in glob.glob(os.path.join(root, "**/.poorai/state.sqlite"), recursive=True):
            if "node_modules" in db:
                continue
            workspace = os.path.dirname(os.path.dirname(db))
            chat_model = None
            try:
                chat_model = json.load(open(os.path.join(workspace, ".poorai/chat-config.json"))).get("model")
            except (OSError, ValueError):
                pass
            try:
                rows = sqlite3.connect(db).execute(
                    "select run_id, event_type, payload from events order by at"
                ).fetchall()
            except sqlite3.Error:
                continue
            model_of = {}
            for run, kind, payload in rows:
                if kind == "run.started":
                    started = json.loads(payload)
                    digest = started.get("model_digest")
                    model_of[run] = started.get("model_ref") or names.get(digest) or (
                        f"digest {digest[:16]}" if digest else None
                    )
            for run, kind, payload in rows:
                model = model_of.get(run) or chat_model or "unknown"
                if kind == "turn.generated":
                    generations[model] += 1
                elif kind in ("action.malformed", "turn.failed"):
                    event = json.loads(payload)
                    fault = event.get("kind") or event.get("outcome") or "unclassified"
                    how = treatment(fault, str(event.get("problem", "")), event.get("detail", ""))
                    key = (fault, how)
                    faults[model][key] += 1
                    example.setdefault((model, key), str(event.get("problem") or event.get("detail"))[:140])
    for model, counts in sorted(faults.items(), key=lambda item: -sum(item[1].values())):
        total = sum(counts.values())
        print(f"{model}: {total} refused or failed, {generations[model]} generations")
        for (fault, how), count in counts.most_common():
            print(f"  {count:4d}  {fault:<16} {how}")
            print(f"        e.g. {example[(model, (fault, how))]}")
    return 0


if __name__ == "__main__":
    sys.exit(main([os.path.abspath(root) for root in (sys.argv[1:] or ["."])]))
