"""Prints a suite report (the JSON `pwr --json eval suite` writes) briefly."""
import json
import sys

report = json.load(open(sys.argv[1]))
result = report.get("result") or {}
if not result:
    print(json.dumps(report.get("error"), indent=1))
    sys.exit(1)
print(result["suite"], result["verdicts"])
if result.get("regressions"):
    print("  REGRESSIONS:", ", ".join(result["regressions"]))
edited = (result.get("edits") or {}).get("attempted", 0) > 0
for key in ("edits", "applied_first_time", "verification", "false_completions"):
    if key in ("edits", "applied_first_time") and not edited:
        continue
    if result.get(key) is not None:
        print(f"  {key}: {result[key]}")
for case in result["results"]:
    o = case["observed"]
    if o.get("kind") != "task":
        print(f"  {case['id']:42} {case['verdict']}")
        continue
    line = f"  {case['id']:42} {case['verdict']:9} turns={o['turns']:<3} min={o['minutes']:<5}"
    if o.get("answer_matched") is not None:
        line += f" answered={o['answer_matched']} forbidden={o.get('forbidden_mentioned')} nav={o['navigation']}"
    else:
        line += f" resolved={o['resolved']} declared={o['declared']} false={o['false_completion']}"
        line += f" tests_changed={o['tests_changed']} edits={o['edits']}"
    if o["out_of_scope"]:
        line += f" OUT_OF_SCOPE={o['out_of_scope']}"
    print(line)
