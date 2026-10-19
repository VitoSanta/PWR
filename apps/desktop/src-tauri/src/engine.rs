//! The MLX engine's Python environment: found, or installed on first run.
//!
//! The core runs models through a Python sidecar with `mlx-lm` installed
//! (`PWR_MLX_PYTHON`). A person who downloads the app has none, so the app
//! installs one -- a standalone Python and the pinned packages, with the `uv`
//! binary bundled in the app -- under its own data folder, where it can be
//! found again and removed in one piece. Nothing here depends on the system
//! Python or on Xcode's command-line tools.

use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

/// Runtime pins verified for the Gemma line; keep setup-mlx.sh aligned.
const PYTHON_VERSION: &str = "3.14.8";
const PACKAGES: &[&str] = &[
    "mlx==0.32.3",
    "mlx-lm==0.32.0",
    "mlx-embeddings==0.1.0",
    "mlx-vlm==0.7.8",
];
/// Complete wheel/version allowlist for the managed macOS environment.
const REQUIREMENTS: &str = include_str!("../engine/requirements-macos-arm64-py314.lock");
fn requirements_hash() -> &'static str {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| format!("{:x}", Sha256::digest(REQUIREMENTS.as_bytes())))
}

/// The encoder the semantic section ranking uses; the sidecar reads it offline.
const ENCODER: &str = "intfloat/multilingual-e5-small";
const ENCODER_MANIFEST: &str = include_str!("../../../../crates/pwr-mlx/sidecar/encoder.json");
fn encoder_manifest_hash() -> String {
    format!("{:x}", Sha256::digest(ENCODER_MANIFEST.as_bytes()))
}
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
        if cfg!(target_os = "macos") {
            "mlx"
        } else {
            "llama"
        }
        .into()
    });
    backend == "mlx"
}

fn supported() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

/// `PWR_MLX_PYTHON`, else the environment this app installed, else -- in a
/// development build only -- the checkout's (`scripts/setup-mlx.sh`). A
/// release build behaves as it will for anyone who downloads it.
///
/// A Python named by the environment or the checkout is taken only if it has
/// mlx-lm: taken on its name alone, one that had never been installed was
/// reported ready, the installer was never offered and every model failed to
/// load with "cannot import mlx_lm".
fn find(app: &AppHandle) -> Option<(&'static str, PathBuf)> {
    if let Some(python) = std::env::var_os("PWR_MLX_PYTHON").map(PathBuf::from) {
        if has_engine(&python) {
            return Some(("environment", python));
        }
    }
    if let Ok(root) = root(app) {
        let python = venv_python(&root);
        if python.is_file()
            && std::fs::read(root.join(MARKER)).is_ok_and(|marker| marker_current(&marker))
        {
            return Some(("installed", python));
        }
    }
    if cfg!(debug_assertions) {
        let checkout = crate::repository()?;
        let python = checkout.join(".venv-mlx/bin/python");
        return has_engine(&python).then_some(("checkout", python));
    }
    None
}

/// Whether an install's marker names the packages this build pins. An install
/// made by an earlier build is not ready: it would keep libraries the pins
/// have left behind (mlx-vlm 0.6.17 cannot load 1-bit weights), so the
/// installer is offered again and replaces it.
fn marker_current(marker: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(marker)
        .ok()
        .filter(|marker| {
            marker.get("python").and_then(serde_json::Value::as_str) == Some(PYTHON_VERSION)
                && marker
                    .get("requirementsSha256")
                    .and_then(serde_json::Value::as_str)
                    == Some(requirements_hash())
                && marker
                    .get("encoderManifestSha256")
                    .and_then(serde_json::Value::as_str)
                    == Some(encoder_manifest_hash().as_str())
        })
        .and_then(|marker| {
            marker
                .get("packages")
                .and_then(|packages| packages.as_array().cloned())
        })
        .is_some_and(|packages| {
            packages
                .iter()
                .map(|package| package.as_str())
                .eq(PACKAGES.iter().map(|package| Some(*package)))
        })
}

/// Whether `python` runs and can find mlx-lm. Found, not imported: importing
/// it loads MLX, which takes seconds; finding it takes a fraction of one.
fn has_engine(python: &Path) -> bool {
    static VERIFIED: std::sync::OnceLock<Mutex<Option<PathBuf>>> = std::sync::OnceLock::new();
    if !python.is_file() {
        return false;
    }
    let cache = VERIFIED.get_or_init(|| Mutex::new(None));
    if cache
        .lock()
        .ok()
        .is_some_and(|cached| cached.as_deref() == Some(python))
    {
        return true;
    }
    let ok = std::process::Command::new(python)
        .args([
            "-c",
            "import importlib.util, sys; sys.exit(importlib.util.find_spec('mlx_lm') is None)",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if ok {
        if let Ok(mut cached) = cache.lock() {
            *cached = Some(python.to_path_buf());
        }
    }
    ok
}

/// The interpreter the core should be started with, if there is one.
pub fn python(app: &AppHandle) -> Option<PathBuf> {
    find(app).map(|(_, python)| python)
}

#[tauri::command]
pub async fn engine_status(app: AppHandle) -> Result<EngineStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The bundled `uv`; in development, the one on PATH.
fn uv(app: &AppHandle) -> Result<PathBuf, String> {
    let bin = format!("uv{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(&bin))
        .filter(|path| path.is_file())
    {
        return Ok(path);
    }
    for dir in std::env::split_paths(&crate::login_path()) {
        let path = dir.join(&bin);
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
        let _reset = RunningReset(running);
        let updates = app.state::<crate::updater::Updates>();
        let _activity = updates
            .activity
            .try_lock()
            .map_err(|_| "Wait for the current native operation to finish.".to_string())?;
        updates.allow_activity()?;
        if app
            .state::<crate::Core>()
            .0
            .lock()
            .map_err(|_| "Core state unavailable".to_string())?
            .is_some()
        {
            return Err("Stop the workspace before installing the engine.".into());
        }
        install(&app, &child, &cancelled)
    })
    .await
    .map_err(|error| error.to_string())?;
    result
}

fn isolate_step(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}
fn kill_step(child: &mut Child) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        rustix::process::kill_process_group(
            rustix::process::Pid::from_child(child),
            rustix::process::Signal::KILL,
        )
        .map_err(Into::into)
    }
    #[cfg(not(unix))]
    {
        child.kill()
    }
}

struct RunningReset(Arc<AtomicBool>);
impl Drop for RunningReset {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
struct StepCleanup<'a>(&'a Mutex<Option<Child>>);
impl StepCleanup<'_> {
    fn stop(&self) {
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(mut child) = slot.take() {
            let _ = kill_step(&mut child);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for StepCleanup<'_> {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Stops an install; what it left is replaced by the next one.
#[tauri::command]
pub fn engine_cancel(setup: State<'_, Setup>) {
    setup.cancelled.store(true, Ordering::SeqCst);
    if let Ok(mut guard) = setup.child.lock() {
        if let Some(child) = guard.as_mut() {
            let _ = kill_step(child);
        }
    }
}

fn install(
    app: &AppHandle,
    child: &Mutex<Option<Child>>,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let root = root(app)?;
    let uv = uv(app)?;
    std::fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    let mut previous = crate::engine_install::Transaction::begin(&root, MARKER)
        .map_err(|error| format!("Cannot preserve the previous engine: {error}"))?;
    // Declared after the transaction: on unwind, stop children before restoring
    // the old prefix. A surviving installer must never write into that prefix.
    let steps = StepCleanup(child);
    let result = install_into(app, child, cancelled, &root, &uv);
    steps.stop();
    match result {
        Ok(()) => {
            previous.commit();
            Ok(())
        }
        Err(error) => match previous.rollback() {
            Ok(()) => Err(error),
            Err(restore) => Err(format!(
                "{error}\nCould not restore the previous engine: {restore}. Backup retained at {}",
                previous.backup().display()
            )),
        },
    }
}

fn install_into(
    app: &AppHandle,
    child: &Mutex<Option<Child>>,
    cancelled: &AtomicBool,
    root: &Path,
    uv: &Path,
) -> Result<(), String> {
    let venv = root.join("venv");
    let python = venv_python(root);

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

    let mut venv_command = base(uv);
    venv_command
        .args(["venv", "--python", PYTHON_VERSION])
        .arg(&venv);
    run(app, 1, venv_command, child, cancelled)?;

    // Use the bytes compiled into this app, not a mutable external lock file.
    let requirements = root.join(format!("requirements-{}.lock", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&requirements)
        .map_err(|error| error.to_string())?;
    file.write_all(REQUIREMENTS.as_bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    let mut pip = base(uv);
    pip.args([
        "pip",
        "install",
        "--require-hashes",
        "--only-binary",
        ":all:",
        "--no-deps",
        "--python",
    ])
    .arg(&python)
    .arg("--requirement")
    .arg(&requirements);
    let installed = run(app, 2, pip, child, cancelled);
    let _ = std::fs::remove_file(&requirements);
    installed?;

    let mut assets = app
        .path()
        .resolve(
            "sidecar/encoder_assets.py",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|error| error.to_string())?;
    if !assets.is_file() && cfg!(debug_assertions) {
        assets = crate::repository()
            .ok_or("Cannot locate encoder setup assets")?
            .join("crates/pwr-mlx/sidecar/encoder_assets.py");
    }
    if !assets.is_file() {
        return Err("Encoder setup assets are missing from this copy of PWR.".into());
    }
    let mut encoder = base(&python);
    encoder.arg(assets).arg("--install");
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
        "requirementsSha256": requirements_hash(),
        "encoderManifestSha256": encoder_manifest_hash(),
        "installedBy": env!("CARGO_PKG_VERSION"),
    });
    std::fs::write(
        root.join(MARKER),
        serde_json::to_vec_pretty(&marker).unwrap_or_default(),
    )
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
        let _ = app.emit(
            "engine-setup",
            Progress {
                step,
                total: STEPS.len(),
                label,
                line,
            },
        );
    };
    emit(None);
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled.".into());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_step(&mut command);
    let mut current = slot.lock().map_err(|_| "installer state poisoned")?;
    let mut spawned = command
        .spawn()
        .map_err(|error| format!("{label}: {error}"))?;
    let stdout = spawned.stdout.take();
    let stderr = spawned.stderr.take();
    *current = Some(spawned);
    // Cover cancellation between the pre-spawn check and publication.
    if cancelled.load(Ordering::SeqCst) {
        if let Some(child) = current.as_mut() {
            let _ = kill_step(child);
        }
    }
    drop(current);

    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let readers: Vec<_> = [
        stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
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
                    Progress {
                        step,
                        total: STEPS.len(),
                        label,
                        line: Some(line),
                    },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn cancellation_stops_a_step_and_its_ordinary_descendants() {
        let marker =
            std::env::temp_dir().join(format!("pwr-install-child-{}", uuid::Uuid::new_v4()));
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "(sleep 2; printf escaped > \"$PWR_TEST_MARKER\") & echo ready; wait",
            ])
            .env("PWR_TEST_MARKER", &marker)
            .stdout(Stdio::piped());
        isolate_step(&mut command);
        let mut child = command.spawn().unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        kill_step(&mut child).unwrap();
        let _ = child.wait();
        std::thread::sleep(std::time::Duration::from_secs(3));
        assert!(
            !marker.exists(),
            "descendant survived cancellation and wrote its marker"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unwind_stops_children_before_restoring_the_old_prefix() {
        let root = std::env::temp_dir().join(format!("pwr-engine-unwind-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("venv")).unwrap();
        std::fs::write(root.join("venv/original"), b"old bytes").unwrap();
        std::fs::write(root.join(MARKER), b"old marker").unwrap();
        let result = std::panic::catch_unwind(|| {
            let _previous = crate::engine_install::Transaction::begin(&root, MARKER).unwrap();
            std::fs::create_dir(root.join("venv")).unwrap();
            let mut command = Command::new("/bin/sh");
            command
                .args([
                    "-c",
                    "(sleep 2; printf escaped > \"$PWR_TEST_MARKER\") & echo ready; wait",
                ])
                .env("PWR_TEST_MARKER", root.join("venv/escaped"))
                .stdout(Stdio::piped());
            isolate_step(&mut command);
            let mut child = command.spawn().unwrap();
            let mut ready = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut ready)
                .unwrap();
            assert_eq!(ready.trim(), "ready");
            let slot = Mutex::new(Some(child));
            let _cleanup = StepCleanup(&slot);
            panic!("injected installer unwind");
        });
        assert!(result.is_err());
        std::thread::sleep(std::time::Duration::from_secs(3));
        assert_eq!(
            std::fs::read(root.join("venv/original")).unwrap(),
            b"old bytes"
        );
        assert_eq!(std::fs::read(root.join(MARKER)).unwrap(), b"old marker");
        assert!(!root.join("venv/escaped").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_install_is_current_only_with_the_pinned_packages() {
        let current = serde_json::json!({ "python": PYTHON_VERSION, "packages": PACKAGES, "requirementsSha256": requirements_hash(), "encoderManifestSha256": encoder_manifest_hash() });
        assert!(marker_current(&serde_json::to_vec(&current).unwrap()));
        let mut wrong_encoder = current.clone();
        wrong_encoder["encoderManifestSha256"] = serde_json::json!("different");
        assert!(!marker_current(
            &serde_json::to_vec(&wrong_encoder).unwrap()
        ));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(ENCODER_MANIFEST).unwrap()["model"],
            ENCODER
        );
        let legacy = serde_json::json!({ "python": PYTHON_VERSION, "packages": PACKAGES });
        assert!(!marker_current(&serde_json::to_vec(&legacy).unwrap()));
        let wrong_lock = serde_json::json!({ "python": PYTHON_VERSION, "packages": PACKAGES, "requirementsSha256": "different" });
        assert!(!marker_current(&serde_json::to_vec(&wrong_lock).unwrap()));
        for package in PACKAGES {
            assert!(
                REQUIREMENTS
                    .lines()
                    .any(|line| line.split_whitespace().next() == Some(*package)),
                "direct pin missing from lock: {package}"
            );
        }
        let earlier = serde_json::json!({
            "python": PYTHON_VERSION, "requirementsSha256": requirements_hash(), "encoderManifestSha256": encoder_manifest_hash(),
            "packages": ["mlx==0.32.3", "mlx-lm==0.32.0", "mlx-embeddings==0.1.0", "mlx-vlm==0.6.17"]
        });
        assert!(!marker_current(&serde_json::to_vec(&earlier).unwrap()));
        assert!(!marker_current(br#"{"packages":["mlx==0.32.3"]}"#));
        assert!(!marker_current(br#"{"python":"3.11"}"#));
        let older_python = serde_json::json!({"python":"3.11","packages":PACKAGES,"requirementsSha256":requirements_hash(),"encoderManifestSha256":encoder_manifest_hash()});
        assert!(!marker_current(&serde_json::to_vec(&older_python).unwrap()));
        assert!(!marker_current(b""));
    }
}
