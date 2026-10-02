"""What a run needs to be read later: which engine had the machine, and what ran.

A timing or a pass rate means nothing without the conditions it was measured
under. Two engines on one Mac overran the GPU's working set on 2026-10-01 and
both emitted text without meaning; those runs were discarded by hand. The lease
below makes the runner refuse that situation instead of recording it.
"""

import fcntl
import hashlib
import json
import os
import pathlib
import re
import subprocess
import time
import urllib.request

# Processes that hold a model in memory and compete for the GPU, matched on the
# command line. Ollama's server alone holds nothing; its runner does, and its
# `/api/ps` lists what is loaded. LM Studio is not installed on the
# maintainer's Mac today and is not recognised.
ENGINE_SIGNATURES = [
    ("pwr-mlx", re.compile(r"pwr_mlx\.py")),
    ("mlx-lm-server", re.compile(r"mlx_lm[. ]server")),
    ("llama-server", re.compile(r"(^|/)llama-server(\s|$)")),
    ("ollama-runner", re.compile(r"(^|/)ollama\s+runner(\s|$)")),
]


def engines_in(ps_output, own_pids=()):
    """The engine processes in `ps -axo pid=,command=` output, without ours."""
    found = []
    for line in ps_output.splitlines():
        line = line.strip()
        if not line:
            continue
        pid, _, command = line.partition(" ")
        if not pid.isdigit() or int(pid) in own_pids:
            continue
        for name, pattern in ENGINE_SIGNATURES:
            if pattern.search(command):
                found.append({"pid": int(pid), "engine": name, "command": command[:200]})
                break
    return found


def running_engines():
    """Engine processes on this machine now, and Ollama's loaded models."""
    try:
        ps = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True,
                            timeout=10).stdout
    except (OSError, subprocess.TimeoutExpired):
        ps = ""
    found = engines_in(ps, own_pids={os.getpid()})
    try:
        with urllib.request.urlopen("http://127.0.0.1:11434/api/ps", timeout=2) as reply:
            for model in json.load(reply).get("models", []):
                found.append({"pid": None, "engine": "ollama-loaded", "command": model.get("name", "")})
    except Exception:  # no Ollama server: nothing loaded there
        pass
    return found


class LeaseHeld(Exception):
    """Another campaign holds the engine lease."""


class Lease:
    """One campaign at a time on this machine's inference engine.

    An exclusive `flock` on a file every runner opens: the kernel releases it
    when the holder exits, however it exits, so a crashed campaign does not
    leave the machine locked. The file names the holder for the one refused.
    """

    def __init__(self, path, holder):
        self.path = pathlib.Path(path)
        self.holder = holder
        self.file = None

    def __enter__(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.file = open(self.path, "a+")
        try:
            fcntl.flock(self.file, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            self.file.seek(0)
            held_by = self.file.read().strip() or "an unnamed holder"
            self.file.close()
            self.file = None
            raise LeaseHeld(held_by)
        self.file.seek(0)
        self.file.truncate()
        self.file.write(json.dumps({"pid": os.getpid(), "holder": self.holder,
                                    "since": time.strftime("%Y-%m-%dT%H:%M:%S")}))
        self.file.flush()
        return self

    def __exit__(self, *_):
        if self.file:
            self.file.seek(0)
            self.file.truncate()
            fcntl.flock(self.file, fcntl.LOCK_UN)
            self.file.close()
            self.file = None


def digest(path, length=12):
    path = pathlib.Path(path)
    if not path.is_file():
        return None
    return hashlib.sha256(path.read_bytes()).hexdigest()[:length]


def _sysctl(name):
    try:
        return subprocess.run(["sysctl", "-n", name], capture_output=True, text=True,
                              timeout=5).stdout.strip() or None
    except (OSError, subprocess.TimeoutExpired):
        return None


def machine():
    """The host, once per campaign."""
    try:
        system = subprocess.run(["sw_vers", "-productVersion"], capture_output=True, text=True,
                                timeout=5).stdout.strip() or None
    except (OSError, subprocess.TimeoutExpired):
        system = None
    memory = _sysctl("hw.memsize")
    return {
        "model": _sysctl("hw.model"),
        "chip": _sysctl("machdep.cpu.brand_string"),
        "memory_bytes": int(memory) if memory and memory.isdigit() else None,
        "macos": system,
    }


def load():
    """How busy the machine is at this moment: load averages and free memory."""
    one, five, fifteen = os.getloadavg()
    free = None
    try:
        out = subprocess.run(["memory_pressure", "-Q"], capture_output=True, text=True,
                             timeout=10).stdout
        match = re.search(r"free percentage:\s*(\d+)%", out)
        free = int(match.group(1)) if match else None
    except (OSError, subprocess.TimeoutExpired):
        pass
    return {"at": time.strftime("%Y-%m-%dT%H:%M:%S"), "load_avg": [round(one, 2), round(five, 2), round(fifteen, 2)],
            "memory_free_percent": free}


def engine_libraries(python):
    """The libraries that generate, as the engine's interpreter imports them."""
    probe = ("import json, platform, mlx.core as mx, mlx_lm; "
             "print(json.dumps({'python': platform.python_version(), 'mlx': mx.__version__, "
             "'mlx_lm': mlx_lm.__version__}))")
    try:
        out = subprocess.run([python, "-c", probe], capture_output=True, text=True, timeout=60)
        return json.loads(out.stdout) if out.returncode == 0 else {"error": out.stderr.strip()[-300:]}
    except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError) as error:
        return {"error": repr(error)}


def models_root():
    return pathlib.Path(os.environ.get("PWR_MLX_MODELS", pathlib.Path.home() / ".pwr/models"))


def model_facts(model_ref, root=None):
    """The artifact a run used: revision, quantization, weights and template."""
    folder = (root or models_root()) / model_ref
    facts = {"ref": model_ref, "path": str(folder)}
    revision = folder / ".pwr-revision"
    facts["revision"] = revision.read_text().strip() if revision.is_file() else None
    try:
        config = json.loads((folder / "config.json").read_text())
    except (OSError, json.JSONDecodeError):
        config = {}
    quantization = config.get("quantization") or config.get("quantization_config") or {}
    facts["quantization"] = {key: value for key, value in quantization.items()
                             if not isinstance(value, dict)} or None
    # Mixed-precision artifacts quantize some layers differently; how many is
    # enough to tell two artifacts apart without copying the table.
    facts["quantization_overrides"] = sum(isinstance(value, dict) for value in quantization.values())
    facts["model_type"] = (config.get("text_config") or {}).get("model_type") or config.get("model_type")
    facts["weights_bytes"] = sum(p.stat().st_size for p in folder.glob("*.safetensors")) or None
    facts["config_digest"] = digest(folder / "config.json")
    facts["chat_template_digest"] = digest(folder / "chat_template.jinja") or digest(folder / "tokenizer_config.json")
    return facts


def sampling_from(reply):
    """Effective sampling and where each value came from, from `_pwr/model_sampling`."""
    if not isinstance(reply, dict):
        return None
    return {field["name"]: {"value": field.get("value"), "source": field.get("source")}
            for field in reply.get("fields", []) if isinstance(field, dict) and "name" in field}


def window_from(reply):
    """The window the core granted, from `_pwr/models`."""
    if not isinstance(reply, dict):
        return None
    decision = reply.get("contextDecision") or {}
    return {"context_tokens": reply.get("contextTokens"),
            "granted_tokens": decision.get("grantedTokens"),
            "requested_tokens": decision.get("requestedTokens"),
            "computed": decision.get("computed"),
            "rationale": decision.get("rationale"),
            "reasoning_effort": reply.get("reasoningEffort")}
