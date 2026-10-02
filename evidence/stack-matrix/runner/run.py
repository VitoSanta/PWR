#!/usr/bin/env python3
"""The stack matrix: PWR on real tasks across languages, frameworks and tools.

    run.py list [--split dev|heldout]
    run.py reference [TASK ...]          seed fails and reference passes, in containers
    run.py run [TASK ...] --run RUN_ID [--arm pwr|minimal] [--split dev|heldout] [--repeat N]
    run.py verify TASK WORKSPACE         the independent verdict on one workspace
    run.py freeze [--reason WHY]         record every task's split and digest (splits.json)

PWR is driven the way the desktop app drives it -- `pwr serve --stdio`, one
process per task, goal mode, the permission questions answered by a stand-in
person who grants what the task's `allow` names and refuses the rest. The
verdict never comes from PWR: a clean copy of the workspace, with the task's
hidden tests laid over it, is run in the task's container image.
"""

import argparse
import difflib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

HERE = pathlib.Path(__file__).resolve().parent
SUITE = HERE.parent
REPO = SUITE.parent.parent
TASKS = SUITE / "tasks"
# Further task folders, outside this repository: held-out tasks written by
# someone other than whoever tunes PWR, so that person never reads them.
# Separated by the platform's path separator.
EXTRA_TASKS = [pathlib.Path(p).expanduser() for p in
               os.environ.get("PWR_EVIDENCE_TASKS", "").split(os.pathsep) if p]
RESULTS = pathlib.Path(os.environ.get("PWR_EVIDENCE_RESULTS", pathlib.Path.home() / "Desktop/pwr-evidence/runs"))
PWR_BIN = os.environ.get("PWR_BIN", str(REPO / "target/release/pwr"))
MLX_PYTHON = os.environ.get(
    "PWR_MLX_PYTHON",
    str(pathlib.Path.home() / "Library/Application Support/ai.pwr.desktop/engine/venv/bin/python"),
)
MODEL = os.environ.get("PWR_EVIDENCE_MODEL", "lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit")

# The frozen identity of every task: split and digest. A campaign runs only
# tasks that still match it, so a result names the task it was measured on and
# a held-out task cannot be quietly edited between campaigns.
SPLITS = pathlib.Path(os.environ.get("PWR_EVIDENCE_SPLITS", SUITE / "splits.json"))

# One campaign at a time holds the machine's inference engine (D-2026-10-01-1).
LEASE = pathlib.Path(os.environ.get("PWR_EVIDENCE_LEASE", pathlib.Path.home() / "Desktop/pwr-evidence/engine.lock"))

sys.path.insert(0, str(HERE))
from acp import Core, initialize  # noqa: E402
import provenance  # noqa: E402

# Never part of what is verified or diffed: build output, installed
# toolchains, caches and PWR's own state.
NOT_SOURCE = {
    ".git", ".pwr", ".pwr-scratch", ".toolchains", "node_modules", "bin", "obj", "target",
    "build", "dist", "__pycache__", ".venv", "venv", ".gradle", ".mvn-cache", ".pytest_cache",
    ".next", ".angular", ".dart_tool", "vendor", "_build", "deps", ".build", ".swiftpm",
}

# What the stand-in person says after the independent verification failed, by
# protocol version. It never names a hidden test. Version 1 pointed at the
# acceptance checks, which by then usually passed: seen in c2, a model told to
# make passing checks pass re-read its work, said it was done, and spent two
# turns doing nothing. Version 2 says what the person knows -- the work does
# not yet do everything asked -- and where to look. Each result records the
# protocol it ran under; results under different protocols are not pooled.
NUDGES = {
    1: "It is not finished yet: keep going until the acceptance checks pass.",
    2: "It is not finished yet: the work does not do everything I asked. Check it against "
       "my request and the README rule by rule, and try the cases the existing tests do "
       "not cover.",
}
PROTOCOL = int(os.environ.get("PWR_EVIDENCE_PROTOCOL", "2"))
NUDGE = NUDGES[PROTOCOL]


def say(*parts):
    print(time.strftime("%H:%M:%S"), *parts, flush=True)


# ---------------------------------------------------------------- tasks


def load_tasks(names=None, split=None):
    tasks = []
    # Tests point the runner at their own tasks alone.
    folders = EXTRA_TASKS if os.environ.get("PWR_EVIDENCE_ONLY_EXTRA") else [TASKS, *EXTRA_TASKS]
    paths = sorted(p for folder in folders for p in folder.glob("*/task.json"))
    for path in sorted(paths, key=lambda p: p.parent.name):
        task = json.loads(path.read_text())
        task["dir"] = path.parent
        if task["id"] in {t["id"] for t in tasks}:
            sys.exit(f"task {task['id']} is defined twice ({path.parent})")
        if names and task["id"] not in names:
            continue
        if split and task.get("split") != split:
            continue
        tasks.append(task)
    if names:
        missing = set(names) - {task["id"] for task in tasks}
        if missing:
            sys.exit(f"no such task: {', '.join(sorted(missing))}")
    return tasks


def copy_source(source, target, exclude=NOT_SOURCE):
    """Copies a tree without build output, caches and installed toolchains."""
    source = pathlib.Path(source)
    for path in source.rglob("*"):
        relative = path.relative_to(source)
        if any(part in exclude for part in relative.parts):
            continue
        destination = pathlib.Path(target) / relative
        if path.is_symlink():
            continue
        if path.is_dir():
            destination.mkdir(parents=True, exist_ok=True)
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)


def overlay(source, target):
    if pathlib.Path(source).is_dir():
        shutil.copytree(source, target, dirs_exist_ok=True)


# ---------------------------------------------------------------- verdict

# What Docker says when its daemon is not there to run a verdict.
DOCKER_DOWN = re.compile(r"Cannot connect to the Docker daemon|failed to connect to the docker API|"
                         r"Is the docker daemon running", re.I)


class VerifierDown(Exception):
    """The verdict could not run at all. Not a failure of PWR's: a campaign
    that went on would count every task as failed and, turn after turn, tell
    the model its work was incomplete when nothing had judged it."""


def docker_ready():
    try:
        return subprocess.run(["docker", "info"], capture_output=True, timeout=60).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def verify(task, workspace, label="verify"):
    """(passed, output): the task's own acceptance, run outside PWR."""
    spec = task["verify"]
    exclude = NOT_SOURCE - set(spec.get("keep", []))
    with tempfile.TemporaryDirectory(prefix=f"sm-{task['id']}-") as scratch:
        copy = pathlib.Path(scratch) / "w"
        copy.mkdir()
        copy_source(workspace, copy, exclude)
        # The owner's tests as the owner wrote them: a change to them is not a
        # way to pass.
        for relative in spec.get("restore", []):
            original = task["dir"] / "workspace" / relative
            if original.is_dir():
                shutil.copytree(original, copy / relative, dirs_exist_ok=True)
            elif original.is_file():
                (copy / relative).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(original, copy / relative)
        overlay(task["dir"] / "hidden", copy)
        timeout = spec.get("timeout", 1200)
        if "host" in spec:
            command = ["bash", "-c", spec["host"]]
            name = None
        else:
            name = f"sm-{task['id']}-{label}-{os.getpid()}-{int(time.time())}"
            command = ["docker", "run", "--rm", "--name", name, "-v", f"{copy}:/w", "-w", "/w"]
            for cache in spec.get("cache", []):
                volume, path = cache.split(":", 1)
                command += ["-v", f"pwr-sm-{volume}:{path}"]
            for key, value in spec.get("env", {}).items():
                command += ["-e", f"{key}={value}"]
            # An image whose entrypoint is its tool (terraform) takes the
            # shell as its entrypoint instead.
            if spec.get("entrypoint"):
                command += ["--entrypoint", spec["entrypoint"]]
                command += [spec["image"], "-c", spec["command"]]
            else:
                command += [spec["image"], "sh", "-c", spec["command"]]
        try:
            # One stream, in order: a verdict line printed after a build log
            # must end the output, not sit before the log.
            done = subprocess.run(command, cwd=copy, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                  text=True, timeout=timeout)
            output = done.stdout
            passed = done.returncode == 0
            if name and not passed and DOCKER_DOWN.search(output):
                raise VerifierDown(output.strip()[-500:])
        except subprocess.TimeoutExpired as error:
            if name:
                subprocess.run(["docker", "kill", name], capture_output=True)
            output = f"{error.stdout or ''}{error.stderr or ''}\nTIMEOUT after {timeout}s"
            passed = False
        if passed and spec.get("expect"):
            # A run that exits 0 having run nothing is not a pass.
            passed = re.search(spec["expect"], output, re.M) is not None
        return passed, output


def cmd_reference(args):
    failures = 0
    for task in load_tasks(args.tasks or None):
        seed_passed, seed_output = verify(task, task["dir"] / "workspace", "seed")
        with tempfile.TemporaryDirectory() as scratch:
            solved = pathlib.Path(scratch) / "w"
            shutil.copytree(task["dir"] / "workspace", solved)
            overlay(task["dir"] / "reference", solved)
            ref_passed, ref_output = verify(task, solved, "reference")
        sound = not seed_passed and ref_passed
        failures += not sound
        say(f"{'OK  ' if sound else 'BAD '} {task['id']:<28} seed={'pass' if seed_passed else 'fail'} reference={'pass' if ref_passed else 'fail'}")
        if not sound:
            print((ref_output if not ref_passed else seed_output)[-3000:], flush=True)
    sys.exit(1 if failures else 0)


# ---------------------------------------------------------------- the person


class Person:
    """Answers PWR's questions the way the task says a person would, and
    watches what happens so the result can say it."""

    def __init__(self, core, allow):
        self.core = core
        self.allow = set(allow)
        self.questions = []
        self.actions = 0
        self.failed_actions = 0
        self.prompt_tokens = 0
        self.generated_tokens = 0
        self.prefill_ms = 0
        self.generation_ms = 0
        # Prompts the engine computed mostly from scratch: long, and less than
        # half resumed from its cache. Each is a history that stopped being a
        # prefix of the one before, and on this model that costs minutes.
        self.cold_prefills = []
        self.peak_context = 0
        self.answer = ""

    def __call__(self, message):
        method = message.get("method")
        params = message.get("params") or {}
        if method == "session/request_permission":
            meta = (params.get("_meta") or {}).get("pwr", {})
            kind = meta.get("approval", "")
            title = params.get("toolCall", {}).get("title", "")
            granted = kind in self.allow
            choice = "allow_always" if granted else "reject_once"
            self.questions.append({"approval": kind, "question": title, "answer": choice})
            say(f"  ? {kind}: {title[:140]} -> {choice}")
            self.core.answer(message["id"], {"outcome": {"outcome": "selected", "optionId": choice}})
        elif method == "_pwr/usage":
            self.peak_context = max(self.peak_context, int(params.get("used") or 0))
        elif method == "_pwr/turn_event" and params.get("event") == "generation":
            self.prompt_tokens += int(params.get("promptTokens") or 0)
            self.generated_tokens += int(params.get("generatedTokens") or 0)
            self.prefill_ms += int(params.get("promptEvalMs") or 0)
            self.generation_ms += int(params.get("generationMs") or 0)
            prompt, cached = params.get("promptTokens") or 0, params.get("cachedTokens")
            if cached is not None and prompt > 4096 and cached * 2 < prompt:
                self.cold_prefills.append({"at": time.strftime("%H:%M:%S"), "prompt": prompt,
                                           "cached": cached, "ms": params.get("promptEvalMs")})
        elif method == "session/update":
            update = params.get("update", {})
            kind = update.get("sessionUpdate")
            if kind == "tool_call":
                self.actions += 1
                say(f"  [{self.actions}] {update.get('title', '')[:150]}")
            elif kind == "tool_call_update" and update.get("status") == "failed":
                self.failed_actions += 1
                detail = ""
                for content in update.get("content") or []:
                    if content.get("type") == "content":
                        detail = content.get("content", {}).get("text", "")
                say(f"      failed: {detail[:200]}".replace("\n", " "))
            elif kind == "agent_message_chunk":
                live = (update.get("_meta") or {}).get("pwr", {}).get("live")
                text = update.get("content", {}).get("text", "")
                if not live and text.strip():
                    self.answer = text


# ---------------------------------------------------------------- one task


def diff_against_seed(seed, workspace, exclude=NOT_SOURCE):
    seed, workspace = pathlib.Path(seed), pathlib.Path(workspace)

    def files(root):
        found = {}
        for path in root.rglob("*"):
            relative = path.relative_to(root)
            if any(part in exclude for part in relative.parts) or not path.is_file():
                continue
            found[str(relative)] = path
        return found

    def text(path):
        try:
            data = path.read_bytes()
        except OSError:
            # A file the model left unreadable (mode 000 from a test of its
            # own): the diff says so and the run goes on.
            return None, -1
        if b"\0" in data[:8192] or len(data) > 512 * 1024:
            return None, len(data)
        try:
            return data.decode("utf-8").splitlines(True), len(data)
        except UnicodeDecodeError:
            return None, len(data)

    before, after = files(seed), files(workspace)
    chunks = []
    for name in sorted(set(before) | set(after)):
        old, old_size = text(before[name]) if name in before else ([], 0)
        new, new_size = text(after[name]) if name in after else ([], 0)
        if old is None or new is None:
            def same():
                try:
                    return before[name].read_bytes() == after[name].read_bytes()
                except OSError:
                    return False
            if before.get(name) is None or after.get(name) is None or not same():
                chunks.append(f"Binary or large file {name}: {old_size} -> {new_size} bytes\n")
        elif old != new:
            chunks.extend(difflib.unified_diff(old, new, f"a/{name}", f"b/{name}"))
    return "".join(chunks)


def sidecar_path():
    """The sidecar pinned beside the binary (`sidecar-<revision>/`), else the
    source tree's, which is what an unpinned binary reads."""
    if os.environ.get("PWR_MLX_SIDECAR"):
        return pathlib.Path(os.environ["PWR_MLX_SIDECAR"])
    name = os.path.basename(PWR_BIN)
    if name.startswith("pwr-"):
        pinned = pathlib.Path(PWR_BIN).parent / f"sidecar-{name[4:]}" / "pwr_mlx.py"
        if pinned.exists():
            return pinned
    return REPO / "crates/pwr-mlx/sidecar/pwr_mlx.py"


def sidecar_digest():
    import hashlib
    path = sidecar_path()
    return hashlib.sha256(path.read_bytes()).hexdigest()[:12] if path.exists() else None


def task_digest(task_dir):
    """Every file of a task -- brief, workspace, hidden tests, verifier --
    hashed together, so a result names the exact task it ran against and a
    task revised after a run is visible as one."""
    import hashlib
    digest = hashlib.sha256()
    for path in sorted(p for p in task_dir.rglob("*") if p.is_file()):
        digest.update(str(path.relative_to(task_dir)).encode() + b"\0")
        digest.update(path.read_bytes() + b"\0")
    return digest.hexdigest()[:12]


def binary_revision():
    """The revision the binary under test was built from: a pinned copy is
    named for it (`pwr-<revision>`); otherwise the checkout's, marked so."""
    name = os.path.basename(PWR_BIN)
    if name.startswith("pwr-"):
        return name[4:]
    head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    return f"{head} (unpinned build)"


def frozen_manifest():
    if not SPLITS.is_file():
        return None
    return json.loads(SPLITS.read_text())


def unfrozen(tasks, manifest):
    """Why each task does not match the frozen manifest; empty when all do."""
    if manifest is None:
        return [f"no frozen manifest at {SPLITS}; run `run.py freeze`"]
    problems = []
    for task in tasks:
        entry = manifest["tasks"].get(task["id"])
        if entry is None:
            problems.append(f"{task['id']}: not in the frozen manifest")
        elif entry["split"] != task.get("split"):
            problems.append(f"{task['id']}: split {task.get('split')} but frozen as {entry['split']}")
        elif entry["digest"] != task_digest(task["dir"]):
            problems.append(f"{task['id']}: changed since the manifest was frozen")
    return problems


def cmd_freeze(args):
    tasks = load_tasks()
    previous = frozen_manifest()
    if previous and not args.reason:
        sys.exit(f"{SPLITS} exists; a new freeze needs --reason, kept in its history")
    history = (previous or {}).get("history", [])
    if previous:
        history.append({"frozen": previous["frozen"], "reason_replaced": args.reason,
                        "changed": sorted(t["id"] for t in tasks
                                          if previous["tasks"].get(t["id"], {}).get("digest") != task_digest(t["dir"]))})
    manifest = {
        "frozen": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "note": "Split and digest of every task, as run.py's task_digest computes it. "
                "A campaign runs only tasks that match; a changed task is listed in README.md "
                "under Revisions and the manifest frozen again with a reason.",
        "tasks": {t["id"]: {"split": t.get("split"), "digest": task_digest(t["dir"])} for t in tasks},
        "history": history,
    }
    SPLITS.write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n")
    say(f"froze {len(tasks)} tasks in {SPLITS}")


def wait_for_idle_engines(seconds=60):
    """Other inference engines still running after `seconds`: the previous
    task's engine is given time to exit; anything left competes for the GPU."""
    deadline = time.time() + seconds
    while True:
        engines = provenance.running_engines()
        if not engines or time.time() >= deadline:
            return engines
        time.sleep(2)


# What each arm sends with every prompt. `pwr` is the product as the app runs
# it; `minimal` is the W8.3 control behind the same `pwr serve` (same engine,
# sampling, tools, sandbox and budgets, none of PWR's harness).
ARMS = {
    "pwr": {"goalMode": True},
    "minimal": {"goalMode": False, "harness": "minimal"},
}


def run_task(task, run_id, attempt, turns_override=None, campaign=None, arm="pwr"):
    label = task["id"] if attempt == 1 else f"{task['id']}~{attempt}"
    out = RESULTS / run_id / label
    if (out / "result.json").exists():
        say(f"{label}: already done")
        return json.loads((out / "result.json").read_text())
    if out.exists():
        shutil.rmtree(out)
    workspace = out / "workspace"
    shutil.copytree(task["dir"] / "workspace", workspace)
    git = dict(os.environ, GIT_AUTHOR_NAME="owner", GIT_AUTHOR_EMAIL="owner@example.com",
               GIT_COMMITTER_NAME="owner", GIT_COMMITTER_EMAIL="owner@example.com")
    subprocess.run(["git", "init", "-q", "-b", "main"], cwd=workspace, check=True)
    subprocess.run(["git", "add", "-A"], cwd=workspace, check=True)
    subprocess.run(["git", "commit", "-qm", "Initial"], cwd=workspace, env=git, check=True)

    brief = (task["dir"] / "brief.md").read_text()
    turns = turns_override or task.get("turns", 3)
    budget = task.get("minutes", 90) * 60
    result = {
        "task": task["id"], "title": task["title"], "stacks": task["stacks"],
        "category": task["category"], "split": task.get("split"), "attempt": attempt,
        "arm": arm,
        "run": run_id, "model": MODEL, "binary": PWR_BIN,
        "revision": binary_revision(),
        "protocol": PROTOCOL,
        # The MLX sidecar is read from the source tree, not from the binary:
        # what it was is recorded beside the binary's revision.
        "sidecar": sidecar_digest(),
        "task_digest": task_digest(task["dir"]),
        "started": time.strftime("%Y-%m-%dT%H:%M:%S"), "turns": [],
        # What a reader needs to compare this run with another: which code,
        # which artifact, which engine, under what conditions.
        "provenance": {
            **(campaign or {}),
            "binary_digest": provenance.digest(PWR_BIN),
            "runner_digest": {name: provenance.digest(HERE / name)
                              for name in ("run.py", "acp.py", "provenance.py")},
            "model": provenance.model_facts(MODEL),
            # The core sends no seed: generations are unseeded, and repeated
            # trials differ by sampling.
            "seed": None,
            "load_start": provenance.load(),
        },
    }
    say(f"=== {label}: {task['title']}")
    started = time.time()
    # The person's own profile and memories (`~/.pwr/profile.json`) would reach
    # the prompt -- their name, their language -- and make a run depend on who
    # ran it. Each task gets an empty personal folder instead.
    personal = out / "pwr-home"
    personal.mkdir(exist_ok=True)
    core = Core(PWR_BIN, out / "transcript.jsonl",
                {"PWR_MLX_PYTHON": MLX_PYTHON, "PWR_MLX_SIDECAR": str(sidecar_path()),
                 "PWR_HOME": str(personal),
                 # The engine's own record of each request -- what the prompt
                 # cache saved, and where a missed prompt parted from the last.
                 # It holds model text, so it stays with the run and is never
                 # published.
                 "PWR_MLX_TRACE": str(out / "mlx-trace.jsonl")})
    person = Person(core, task.get("allow", []))
    passed, output = False, ""
    watchdog = None
    try:
        initialize(core)
        models = core.request("_pwr/models", {"cwd": str(workspace), "model": MODEL,
                                              "acknowledgeProvisional": True},
                              on_message=person, timeout=900)
        result["provenance"]["window"] = provenance.window_from(models)
        try:
            sampling = core.request("_pwr/model_sampling", {"cwd": str(workspace), "modelRef": MODEL},
                                    on_message=person, timeout=120)
            result["provenance"]["sampling"] = provenance.sampling_from(sampling)
        except Exception as error:  # recorded as unknown, not guessed
            result["provenance"]["sampling"] = {"error": repr(error)}
        core.request("_pwr/approvals", {"cwd": str(workspace), "mode": "ask"}, timeout=60)
        session = core.request("session/new", {"cwd": str(workspace), "mcpServers": []},
                               on_message=person, timeout=900)["sessionId"]
        stopped_for_time = threading.Event()

        def stop_when_over():
            while not stopped_for_time.wait(15):
                if time.time() - started > budget:
                    say(f"  budget of {budget // 60} min reached: cancelling")
                    core.notify("session/cancel", {"sessionId": session})
                    stopped_for_time.set()

        watchdog = threading.Thread(target=stop_when_over, daemon=True)
        watchdog.start()
        for turn in range(1, turns + 1):
            text = brief if turn == 1 else NUDGE
            turn_started = time.time()
            reply = core.request(
                "session/prompt",
                {"sessionId": session, "prompt": [{"type": "text", "text": text}], **ARMS[arm]},
                on_message=person,
            )
            meta = (reply.get("_meta") or {}).get("pwr", {})
            # The core says which harness ran; a comparison never rests on
            # what was asked for alone.
            ran = meta.get("harness", "pwr")
            if ran != arm:
                raise RuntimeError(f"asked for the {arm} arm, the core ran {ran}")
            passed, output = verify(task, workspace, f"t{turn}")
            record = {
                "turn": turn, "prompt": text if turn > 1 else "(brief)",
                "stop": reply.get("stopReason"), "terminal": meta.get("terminal"),
                "goal": meta.get("goal"), "minutes": round((time.time() - turn_started) / 60, 1),
                "passed": passed, "verdict_tail": output[-2500:],
            }
            result["turns"].append(record)
            say(f"  turn {turn}: {'PASS' if passed else 'fail'} stop={record['stop']} terminal={record['terminal']} {record['minutes']} min")
            if passed or stopped_for_time.is_set():
                break
        stopped_for_time.set()
    except VerifierDown:
        # Not evidence of anything: the campaign stops (cmd_run).
        raise
    except Exception as error:  # the run is evidence either way
        result["error"] = repr(error)
        say(f"  error: {error!r}")
    finally:
        core.close()
    result["provenance"]["load_end"] = provenance.load()
    result.update({
        "passed": passed,
        "minutes": round((time.time() - started) / 60, 1),
        "actions": person.actions, "failed_actions": person.failed_actions,
        "questions": person.questions, "final_answer": person.answer,
        "tokens": {"prompt": person.prompt_tokens, "generated": person.generated_tokens,
                   "prefill_ms": person.prefill_ms, "generation_ms": person.generation_ms,
                   "peak_context": person.peak_context,
                   "cold_prefills": person.cold_prefills},
    })
    kept = NOT_SOURCE - set(task["verify"].get("keep", []))
    (out / "diff.patch").write_text(diff_against_seed(task["dir"] / "workspace", workspace, kept))
    (out / "verdict.txt").write_text(output)
    (out / "result.json").write_text(json.dumps(result, indent=1))
    say(f"=== {label}: {'PASS' if passed else 'FAIL'} in {result['minutes']} min, {person.actions} actions")
    return result


def cmd_run(args):
    tasks = load_tasks(args.tasks or None, args.split)
    # A campaign whose verdicts cannot run would only record failures: c4's
    # first attempt ran six tasks with Docker stopped.
    problems = unfrozen(tasks, frozen_manifest())
    if problems and not args.allow_unfrozen:
        sys.exit("Tasks do not match the frozen manifest:\n  " + "\n  ".join(problems)
                 + "\n(--allow-unfrozen runs them anyway, recorded; such results are not a measurement "
                 "of the frozen task set)")
    if any("host" not in task["verify"] for task in tasks) and not docker_ready():
        sys.exit("Docker is not running: the verdicts run in containers. Start Docker, then run again.")
    try:
        lease = provenance.Lease(LEASE, f"run.py run --run {args.run}, pid {os.getpid()}")
        lease.__enter__()
    except provenance.LeaseHeld as held:
        sys.exit(f"Another campaign holds the inference engine ({LEASE}): {held}")
    campaign = {"machine": provenance.machine(),
                "engine_libraries": provenance.engine_libraries(MLX_PYTHON),
                "busy_machine_allowed": args.allow_busy_machine,
                "splits": {"manifest_digest": provenance.digest(SPLITS),
                           "unfrozen": problems}}
    summary = []
    try:
        for attempt in range(1, args.repeat + 1):
            for task in tasks:
                # Two engines on one Mac overrun its GPU working set and both
                # write text without meaning (2026-10-01): a run beside another
                # engine measures neither, so the campaign stops instead.
                engines = wait_for_idle_engines()
                if engines and not args.allow_busy_machine:
                    say("--- stopped: another inference engine is running; no result recorded for "
                        f"{task['id']}.")
                    for engine in engines:
                        say(f"    {engine['engine']} pid={engine['pid']} {engine['command'][:120]}")
                    sys.exit(3)
                try:
                    result = run_task(task, args.run, attempt, args.turns,
                                      dict(campaign, engines_at_start=engines), args.arm)
                except VerifierDown as down:
                    say(f"--- stopped: Docker stopped answering during {task['id']}; "
                        f"its result is not recorded.\n{down}")
                    sys.exit(2)
                summary.append((result["task"], attempt, result["passed"], result["minutes"]))
    finally:
        lease.__exit__(None, None, None)
    say("--- summary")
    for name, attempt, passed, minutes in summary:
        say(f"{'PASS' if passed else 'FAIL'}  {name}  #{attempt}  {minutes} min")


def cmd_verify(args):
    task = load_tasks([args.task])[0]
    passed, output = verify(task, args.workspace)
    print(output[-4000:])
    say("PASS" if passed else "FAIL")
    sys.exit(0 if passed else 1)


def cmd_list(args):
    for task in load_tasks(split=args.split):
        print(f"{task['id']:<28} {task.get('split', '-'):<8} {','.join(task['stacks']):<32} {task['title']}")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    listing = sub.add_parser("list")
    listing.add_argument("--split")
    reference = sub.add_parser("reference")
    reference.add_argument("tasks", nargs="*")
    run = sub.add_parser("run")
    run.add_argument("tasks", nargs="*")
    run.add_argument("--run", required=True)
    run.add_argument("--split")
    run.add_argument("--repeat", type=int, default=1)
    run.add_argument("--turns", type=int)
    run.add_argument("--arm", choices=sorted(ARMS), default="pwr",
                     help="pwr (the product) or minimal (the W8.3 control); one arm per run")
    run.add_argument("--allow-unfrozen", action="store_true",
                     help="run tasks that do not match the frozen manifest (recorded)")
    run.add_argument("--allow-busy-machine", action="store_true",
                     help="run even with another inference engine present (recorded; timings are then not comparable)")
    freeze = sub.add_parser("freeze")
    freeze.add_argument("--reason", help="why an existing manifest is replaced (kept in its history)")
    check = sub.add_parser("verify")
    check.add_argument("task")
    check.add_argument("workspace")
    args = parser.parse_args()
    {"list": cmd_list, "reference": cmd_reference, "run": cmd_run, "verify": cmd_verify,
     "freeze": cmd_freeze}[args.cmd](args)


if __name__ == "__main__":
    main()
