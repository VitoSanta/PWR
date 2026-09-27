"""Measures the goal review's reviewer: sends its prompt, built as serve.rs
builds it, to the real MLX sidecar for finished runs, and prints the rules
it marks NOT MET. VARIANT=A without reasoning, B with a 4,000-token budget.

    review_probe.py SIDECAR SUITE_DIR RUN/TASK [RUN/TASK ...]

Never while a campaign runs: it loads the model."""
import json, os, pathlib, subprocess, sys, time

SIDECAR = sys.argv[1]
MODEL = pathlib.Path.home() / ".pwr/models/lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit"
PY = pathlib.Path.home() / "Library/Application Support/ai.pwr.desktop/engine/venv/bin/python"
import os
VARIANT = os.environ.get("VARIANT", "A")
INSTRUCTION = ("Go through the specification and the request rule by rule -- every option, error "
    "case, input form and edge case they state -- and write one line per rule:\n"
    "- <the rule> -- MET: <file and function that does it>\n"
    "or\n"
    "- <the rule> -- NOT MET: <what the code does instead>\n"
    "Judge each rule against the code as it is written, reading the lines that would do it, "
    "not against what the code seems meant to do. Write only the list.")
SYSTEM = ("You review code someone else wrote against the specification it was written to. "
          "You did not write it and have no stake in it being finished. You report only rules "
          "the code does not meet, each with the rule quoted and the code that breaks it; you "
          "never report style, and never a rule the code meets.")

def reviewable(path):
    *dirs, name = path.split("/"); name = name.lower()
    if any(d.startswith(".") or d in ("test", "tests", "spec", "__tests__", "node_modules") for d in dirs):
        return False
    return not ("_test." in name or ".test." in name or ".spec." in name or name.startswith("test_")
                or name.endswith(".lock") or name == "package-lock.json")

def prompt_for(run_dir, brief):
    ws = run_dir / "workspace"
    first = subprocess.run(["git", "-C", ws, "rev-list", "--max-parents=0", "HEAD"], capture_output=True, text=True).stdout.split()[0]
    changed = subprocess.run(["git", "-C", ws, "diff", "--name-only", first], capture_output=True, text=True).stdout.split()
    changed += subprocess.run(["git", "-C", ws, "ls-files", "--others", "--exclude-standard"], capture_output=True, text=True).stdout.split()
    code = ""
    for path in sorted(set(changed)):
        if not reviewable(path) or not (ws / path).is_file():
            continue
        try:
            text = (ws / path).read_text()
        except UnicodeDecodeError:
            continue
        code += f"--- {path} ---\n{text[:20000]}\n"
    spec = (ws / "README.md").read_text()[:16000] if (ws / "README.md").exists() else "(there is no README.md)"
    return (f"The request:\n{brief}\n\nThe specification (README.md):\n{spec}\n\n"
            f"The code as it is now:\n{code}\n"
            + INSTRUCTION)

proc = subprocess.Popen([str(PY), SIDECAR], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
def ask(obj):
    proc.stdin.write(json.dumps(obj) + "\n"); proc.stdin.flush()
    text = []
    while True:
        event = json.loads(proc.stdout.readline())
        if event.get("event") == "delta" and event.get("channel") == "content":
            text.append(event["text"])
        if event.get("event") in ("done", "loaded", "error"):
            return event, "".join(text)
print(ask({"id": 1, "op": "load", "path": str(MODEL)})[0].get("event"), flush=True)
suite = pathlib.Path(sys.argv[2])
for spec in sys.argv[3:]:
    run, task = spec.split("/")
    run_dir = pathlib.Path.home() / "Desktop/pwr-evidence/runs" / run / task
    prompt = prompt_for(run_dir, (suite / "tasks" / task / "brief.md").read_text())
    started = time.time()
    done, text = ask({"id": 2, "op": "chat", "messages": [{"role": "system", "content": SYSTEM},
                      {"role": "user", "content": prompt}], "thinking": VARIANT == "B", "max_tokens": 6000 if VARIANT == "B" else 3000, "reasoning_budget": 4000 if VARIANT == "B" else None,
                      "temperature": 0.3, "aside": True})
    print(f"\n===== {spec}: {len(prompt)} chars, {time.time()-started:.0f} s, {done.get('usage', {}).get('prompt_tokens')} tokens", flush=True)
    lines = [l for l in text.splitlines() if "NOT MET" in l]
    print(f"rules listed: {sum(1 for l in text.splitlines() if ' MET' in l)}, NOT MET: {len(lines)}")
    for l in lines: print("   ", l.strip()[:300])
    if not text.strip(): print(done)
proc.stdin.close(); proc.wait(timeout=30)
