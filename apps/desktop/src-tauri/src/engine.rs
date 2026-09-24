//! The MLX engine's Python environment: found, or installed on first run.
//!
//! The core runs models through a Python sidecar with `mlx-lm` installed
//! (`PWR_MLX_PYTHON`). A person who downloads the app has none, so the app
//! installs one -- a standalone Python and the pinned packages, with the `uv`
//! binary bundled in the app -- under its own data folder, where it can be
//! found again and removed in one piece. Nothing here depends on the system
//! Python or on Xcode's command-line tools.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

/// The versions the engine was measured on; the same as `scripts/setup-mlx.sh`.
const PYTHON_VERSION: &str = "3.11";
const PACKAGES: &[&str] = &["mlx==0.32.0", "mlx-lm==0.31.3", "mlx-embeddings==0.1.0", "mlx-vlm==0.6.17"];
/// The encoder the semantic section ranking uses; the sidecar reads it offline.
const ENCODER: &str = "intfloat/multilingual-e5-small";
/// Written last, so a half-finished install is never taken for a ready one.
const MARKER: &str = "pwr-engine.json";

#[derive(Default)]
pub struct Setup {
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// This platform runs models on the MLX engine, so it needs the environment.
    needed: bool,
    /// An Apple-silicon Mac, where MLX runs.
    supported: bool,
    ready: bool,
    /// Where the interpreter came from: "environment", "installed" or "checkout".
    source: Option<&'static str>,
    python: Option<String>,
    /// Where an install goes.
    location: String,
    packages: Vec<&'static str>,
    python_version: &'static str,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    step: usize,
    total: usize,
    label: &'static str,
    /// The latest line a step printed, for the curious and for bug reports.
    line: Option<String>,
}

const STEPS: [&str; 4] = [
    "Preparing Python",
    "Installing the MLX engine",
    "Downloading the search encoder",
    "Checking the installation",
];

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("engine"))
        .map_err(|error| error.to_string())
}

fn venv_python(root: &Path) -> PathBuf {
    root.join("venv/bin/python")
}

fn needed() -> bool {
    let backend = std::env::var("PWR_BACKEND").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") { "mlx" } else { "llama" }.into()
    });
    backend == "mlx"
}

fn supported() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

/// `PWR_MLX_PYTHON`, else the environment this app installed, else -- in a
/// development build only -- the checkout's (`scripts/setup-mlx.sh`). A
/// release build behaves as it will for anyone who downloads it.
fn find(app: &AppHandle) -> Option<(&'static str, PathBuf)> {
    if let Some(python) = std::env::var_os("PWR_MLX_PYTHON") {
        return Some(("environment", PathBuf::from(python)));
    }
    if let Ok(root) = root(app) {
        let python = venv_python(&root);
        if python.is_file() && root.join(MARKER).is_file() {
            return Some(("installed", python));
        }
    }
    if cfg!(debug_assertions) {
        let checkout = crate::repository()?;
        return [".venv-mlx/bin/python", "experiments/engine-spike-mlx-20260917/.venv/bin/python"]
            .into_iter()
            .map(|candidate| checkout.join(candidate))
            .find(|path| path.is_file())
            .map(|path| ("checkout", path));
    }
    None
}

/// The interpreter the core should be started with, if there is one.
pub fn python(app: &AppHandle) -> Option<PathBuf> {
    find(app).map(|(_, python)| python)
}

#[tauri::command]
pub fn engine_status(app: AppHandle) -> Result<EngineStatus, String> {
    let found = find(&app);
    Ok(EngineStatus {
        needed: needed(),
        supported: supported(),
        ready: found.is_some(),
        source: found.as_ref().map(|(source, _)| *source),
        python: found.map(|(_, python)| python.display().to_string()),
        location: root(&app)?.display().to_string(),
        packages: PACKAGES.to_vec(),
        python_version: PYTHON_VERSION,
    })
}

/// The bundled `uv`; in development, the one on PATH.
fn uv(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("uv"))
        .filter(|path| path.is_file())
    {
        return Ok(path);
    }
    for dir in crate::login_path().split(':') {
        let path = Path::new(dir).join("uv");
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("The installer (uv) is missing from this copy of PWR. Download the app again.".into())
}

/// Installs the environment, reporting each step as an `engine-setup` event.
#[tauri::command]
pub async fn engine_install(app: AppHandle, setup: State<'_, Setup>) -> Result<(), String> {
    if !supported() {
        return Err("The MLX engine needs a Mac with Apple silicon (M1 or later).".into());
    }
    if setup.running.swap(true, Ordering::SeqCst) {
        return Err("The engine is already being installed.".into());
    }
    setup.cancelled.store(false, Ordering::SeqCst);
    let child = setup.child.clone();
    let cancelled = setup.cancelled.clone();
    let running = setup.running.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = install(&app, &child, &cancelled);
        running.store(false, Ordering::SeqCst);
        result
    })
    .await
    .map_err(|error| error.to_string())?;
    result
}

/// Stops an install; what it left is replaced by the next one.
#[tauri::command]
pub fn engine_cancel(setup: State<'_, Setup>) {
    setup.cancelled.store(true, Ordering::SeqCst);
    if let Ok(mut guard) = setup.child.lock() {
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
        }
    }
}

fn install(app: &AppHandle, child: &Mutex<Option<Child>>, cancelled: &AtomicBool) -> Result<(), String> {
    let root = root(app)?;
    let uv = uv(app)?;
    std::fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    // A previous attempt that did not finish is started over.
    let _ = std::fs::remove_file(root.join(MARKER));
    let venv = root.join("venv");
    if venv.exists() {
        std::fs::remove_dir_all(&venv).map_err(|error| format!("{}: {error}", venv.display()))?;
    }
    let python = venv_python(&root);

    let base = |program: &Path| {
        let mut command = Command::new(program);
        command
            .env("UV_PYTHON_INSTALL_DIR", root.join("python"))
            .env("UV_CACHE_DIR", root.join("cache"))
            .env("UV_PYTHON_PREFERENCE", "only-managed")
            .env("UV_NO_PROGRESS", "1")
            .env("NO_COLOR", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .env("PATH", crate::login_path());
        command
    };

    let mut venv_command = base(&uv);
    venv_command.args(["venv", "--python", PYTHON_VERSION]).arg(&venv);
    run(app, 1, venv_command, child, cancelled)?;

    let mut pip = base(&uv);
    pip.args(["pip", "install", "--python"]).arg(&python).args(PACKAGES);
    run(app, 2, pip, child, cancelled)?;

    let mut encoder = base(&python);
    encoder.args([
        "-c",
        &format!("from huggingface_hub import snapshot_download; print(snapshot_download('{ENCODER}'))"),
    ]);
    run(app, 3, encoder, child, cancelled)?;

    let mut check = base(&python);
    check.args([
        "-c",
        "import mlx.core as mx, mlx_lm, mlx_vlm, mlx_embeddings; print('mlx', mx.__version__, 'on', mx.default_device())",
    ]);
    run(app, 4, check, child, cancelled)?;

    let marker = serde_json::json!({
        "python": PYTHON_VERSION,
        "packages": PACKAGES,
        "encoder": ENCODER,
        "installedBy": env!("CARGO_PKG_VERSION"),
    });
    std::fs::write(root.join(MARKER), serde_json::to_vec_pretty(&marker).unwrap_or_default())
        .map_err(|error| format!("{}: {error}", root.display()))?;
    // The download cache is not needed once the environment exists.
    let _ = std::fs::remove_dir_all(root.join("cache"));
    Ok(())
}

/// Runs one step, forwarding its output, and fails with its last lines.
fn run(
    app: &AppHandle,
    step: usize,
    mut command: Command,
    slot: &Mutex<Option<Child>>,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let label = STEPS[step - 1];
    let emit = |line: Option<String>| {
        let _ = app.emit("engine-setup", Progress { step, total: STEPS.len(), label, line });
    };
    emit(None);
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled.".into());
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut spawned = command.spawn().map_err(|error| format!("{label}: {error}"))?;
    let stdout = spawned.stdout.take();
    let stderr = spawned.stderr.take();
    *slot.lock().map_err(|_| "installer state poisoned")? = Some(spawned);

    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let readers: Vec<_> = [stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)]
        .into_iter()
        .flatten()
        .map(|stream| {
            let app = app.clone();
            let tail = tail.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let line = line.trim().to_owned();
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(mut tail) = tail.lock() {
                        tail.push(line.clone());
                        let excess = tail.len().saturating_sub(12);
                        tail.drain(..excess);
                    }
                    let _ = app.emit(
                        "engine-setup",
                        Progress { step, total: STEPS.len(), label, line: Some(line) },
                    );
                }
            })
        })
        .collect();
    for reader in readers {
        let _ = reader.join();
    }
    let status = slot
        .lock()
        .map_err(|_| "installer state poisoned")?
        .take()
        .map(|mut child| child.wait())
        .transpose()
        .map_err(|error| format!("{label}: {error}"))?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled.".into());
    }
    match status {
        Some(status) if status.success() => Ok(()),
        _ => {
            let tail = tail.lock().map(|tail| tail.join("\n")).unwrap_or_default();
            Err(format!("{label} failed.\n{tail}"))
        }
    }
}
