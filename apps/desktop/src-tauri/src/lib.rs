//! The desktop app's only native code: a bridge to `pwr serve --stdio`.
//!
//! Everything the harness decides stays in the core. This process starts the
//! core in a workspace, hands every protocol line it writes to the interface as
//! an `acp` event, writes the interface's messages to the core's input, and
//! stops the core when the app closes.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

mod browser;
mod drafts;
mod engine;
mod engine_generation;
mod engine_install;
#[cfg(target_os = "macos")]
mod engine_maintenance;
mod supervision;
mod terminal;
#[cfg(target_os = "macos")]
mod titlebar;
mod updater;

#[derive(Default)]
struct Core(Mutex<Option<Running>>);

#[derive(Default)]
struct Closing(std::sync::atomic::AtomicBool);

#[tauri::command]
fn app_close_confirmed(app: AppHandle, closing: State<'_, Closing>) -> Result<(), String> {
    let updates = app.state::<updater::Updates>();
    let _activity = updates
        .activity
        .try_lock()
        .map_err(|_| "Wait for the current native operation to finish.")?;
    if updates.installing() {
        return Err("Wait for update installation to finish.".into());
    }
    closing.0.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

/// Which start of the core an event belongs to. Opening another workspace
/// stops the old core, and its exit arriving after the new one started was
/// read as the new one dying (2026-09-22: "Error: the core stopped" on every
/// workspace change, with the new core healthy).
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Running {
    child: Child,
    stdin: ChildStdin,
    #[cfg(unix)]
    _guardian: Option<supervision::Guardian>,
    #[cfg(unix)]
    _engine_usage: Option<std::fs::File>,
    #[cfg(debug_assertions)]
    trace_path: PathBuf,
    #[cfg(debug_assertions)]
    trace_owned: bool,
}

impl Running {
    fn stop(mut self) {
        let pid = self.child.id();
        drop(self.stdin);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while matches!(self.child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        #[cfg(unix)]
        {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{pid}")])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        #[cfg(debug_assertions)]
        if self.trace_owned {
            let _ = std::fs::remove_file(self.trace_path);
        }
    }
}

#[derive(serde::Serialize)]
struct Started {
    core: String,
    workspace: String,
    generation: u64,
}

/// The repository this app was built in, when it runs from a checkout.
fn repository() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent()?.parent()?.parent()?;
    root.join("Cargo.toml")
        .is_file()
        .then(|| root.to_path_buf())
}

/// A file shipped in the app's resources, if this copy of the app has it.
fn bundled(app: &AppHandle, relative: &str) -> Option<PathBuf> {
    app.path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(relative))
        .filter(|path| path.is_file())
}

/// The user's home folder on Unix (`HOME`) or Windows (`USERPROFILE`).
pub(crate) fn user_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// `PWR_CORE`, the bundled core, the checkout build, then `pwr` on PATH.
fn core_path(app: &AppHandle) -> PathBuf {
    if let Some(path) = std::env::var_os("PWR_CORE") {
        return PathBuf::from(path);
    }
    let bin = format!("pwr{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = bundled(app, &bin) {
        return path;
    }
    repository()
        .map(|root| {
            root.join(if cfg!(debug_assertions) {
                "target/debug"
            } else {
                "target/release"
            })
            .join(&bin)
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(bin))
}

/// The PATH a terminal would have. An app opened from the Finder inherits
/// launchd's `/usr/bin:/bin:/usr/sbin:/sbin`, where the checks a workspace
/// declares (`npm`, `node`, `cargo`) are not found.
fn login_path() -> String {
    static LOGIN_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LOGIN_PATH
        .get_or_init(|| {
            let current = std::env::var_os("PATH").unwrap_or_default();
            #[cfg(unix)]
            let from_shell = {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
                Command::new(shell)
                    .args(["-lc", "printf %s \"$PATH\""])
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .output()
                    .ok()
                    .filter(|output| output.status.success())
                    .map(|output| {
                        std::ffi::OsString::from(String::from_utf8_lossy(&output.stdout).trim())
                    })
                    .unwrap_or_default()
            };
            #[cfg(not(unix))]
            let from_shell = std::ffi::OsString::new();
            let mut known: Vec<PathBuf> = Vec::new();
            #[cfg(unix)]
            {
                known.push(PathBuf::from("/opt/homebrew/bin"));
                known.push(PathBuf::from("/usr/local/bin"));
            }
            if let Some(home) = user_home() {
                known.push(home.join(".cargo/bin"));
            }
            let mut parts: Vec<PathBuf> = Vec::new();
            for part in std::env::split_paths(&from_shell)
                .chain(std::env::split_paths(&current))
                .chain(known)
            {
                if !part.as_os_str().is_empty() && !parts.contains(&part) {
                    parts.push(part);
                }
            }
            std::env::join_paths(parts)
                .ok()
                .and_then(|joined| joined.into_string().ok())
                .unwrap_or_else(|| current.to_string_lossy().into_owned())
        })
        .clone()
}

/// Where the app remembers the workspace it last opened.
fn last_workspace_file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("last-workspace"))
}

fn chat_home() -> Result<PathBuf, String> {
    let path = std::env::var_os("PWR_CHAT_HOME")
        .map(PathBuf::from)
        .or_else(|| user_home().map(|home| home.join(".pwr/chat")))
        .ok_or("HOME is not set")?;
    std::fs::create_dir_all(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    path.canonicalize()
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn trusted_workspaces_file(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("trusted-workspaces.json"))
        .map_err(|error| error.to_string())
}

fn read_trusted_workspaces(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let file = trusted_workspaces_file(app)?;
    let Ok(bytes) = std::fs::read(&file) else {
        return Ok(Vec::new());
    };
    let paths: Vec<String> =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

fn canonical_workspace(workspace: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(workspace)
        .canonicalize()
        .map_err(|error| format!("{workspace}: {error}"))?;
    if !path.is_dir() {
        return Err(format!("Not a folder: {}", path.display()));
    }
    Ok(path)
}

fn is_workspace_trusted(app: &AppHandle, workspace: &Path) -> Result<bool, String> {
    if chat_home().is_ok_and(|home| home == workspace) {
        return Ok(true);
    }
    Ok(read_trusted_workspaces(app)?
        .iter()
        .any(|path| path == workspace))
}

#[tauri::command]
async fn chat_home_path() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| chat_home().map(|path| path.display().to_string()))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn workspace_is_trusted(app: AppHandle, workspace: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = canonical_workspace(&workspace)?;
        is_workspace_trusted(&app, &path)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn trust_workspace(app: AppHandle, workspace: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = canonical_workspace(&workspace)?;
        if chat_home().is_ok_and(|home| home == path) {
            return Ok(());
        }
        let mut paths = read_trusted_workspaces(&app)?;
        if paths.iter().any(|trusted| trusted == &path) {
            return Ok(());
        }
        paths.push(path);
        let file = trusted_workspaces_file(&app)?;
        let parent = file.parent().ok_or("app config directory has no parent")?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        let bytes = serde_json::to_vec_pretty(&paths).map_err(|error| error.to_string())?;
        let temporary = file.with_extension("json.tmp");
        std::fs::write(&temporary, bytes)
            .map_err(|error| format!("{}: {error}", temporary.display()))?;
        std::fs::rename(&temporary, &file).map_err(|error| format!("{}: {error}", file.display()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// `PWR_WORKSPACE`, else the workspace opened last (if it still exists),
/// else the checkout's site, else the home folder.
#[tauri::command]
async fn default_workspace(app: AppHandle) -> String {
    tauri::async_runtime::spawn_blocking(move || {
        std::env::var("PWR_WORKSPACE")
            .ok()
            .or_else(|| {
                last_workspace_file(&app)
                    .and_then(|file| std::fs::read_to_string(file).ok())
                    .map(|path| path.trim().to_owned())
                    .filter(|path| Path::new(path).is_dir())
            })
            .or_else(|| {
                repository()
                    .map(|root| root.join("pwr-website"))
                    .filter(|path| path.is_dir())
                    .map(|path| path.display().to_string())
            })
            .or_else(|| user_home().map(|home| home.display().to_string()))
            .unwrap_or_else(|| ".".into())
    })
    .await
    .unwrap_or_else(|_| ".".into())
}

#[tauri::command]
async fn core_start(app: AppHandle, workspace: String) -> Result<Started, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let updates = app.state::<updater::Updates>();
        let _activity = updates
            .activity
            .lock()
            .map_err(|_| "Activity state unavailable")?;
        updates.allow_activity(&app.state::<crate::Closing>())?;
        let core = app.state::<Core>();
        let workspace = canonical_workspace(&workspace)?;
        if !is_workspace_trusted(&app, &workspace)? {
            return Err(format!(
                "Workspace has not been trusted: {}",
                workspace.display()
            ));
        }
        let program = core_path(&app);
        let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        // On a Mac the app runs MLX only; llama.cpp arrives with Windows. A
        // development build still honours `PWR_BACKEND` so the GGUF path can be
        // worked on; a release build never switches engine from the environment.
        let default_backend = if cfg!(target_os = "macos") {
            "mlx"
        } else {
            "llama"
        };
        let backend = if cfg!(debug_assertions) {
            std::env::var("PWR_BACKEND").unwrap_or_else(|_| default_backend.into())
        } else {
            default_backend.into()
        };
        let mut command = Command::new(&program);
        command
            .args(["--backend", &backend, "serve", "--stdio"])
            .current_dir(&workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.env("PATH", login_path());
        #[cfg(debug_assertions)]
        let (trace_path, trace_owned) = match std::env::var_os("PWR_MLX_TRACE") {
            Some(path) => (PathBuf::from(path), false),
            None => {
                let directory = app
                    .path()
                    .app_cache_dir()
                    .map_err(|error| error.to_string())?
                    .join("debug-traces");
                std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                        .map_err(|error| error.to_string())?;
                }
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| error.to_string())?
                    .as_nanos();
                let path = directory.join(format!(
                    "core-{}-{generation}-{nonce}.jsonl",
                    std::process::id()
                ));
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                options.open(&path).map_err(|error| error.to_string())?;
                (path, true)
            }
        };
        #[cfg(debug_assertions)]
        command.env("PWR_MLX_TRACE", &trace_path);
        // The MLX interpreter: named by the environment, installed by this app on
        // first run, or (in development) the checkout's. See `engine`.
        #[cfg(unix)]
        let engine_usage = engine::usage(&app)?;
        #[cfg(unix)]
        if let Some(lease) = &engine_usage {
            engine_generation::inherit(lease, &mut command);
        }
        if let Some(python) = engine::python(&app) {
            command.env("PWR_MLX_PYTHON", python);
        }
        // The engine's scripts, bundled beside the core. Without this the core
        // falls back to the path of the checkout it was compiled in, which exists
        // only on the machine that built it -- so a downloaded app found an
        // engine to run and no script to run in it. The environment still wins,
        // and a development build without bundled scripts keeps the checkout's.
        for (variable, script) in [
            ("PWR_MLX_SIDECAR", "sidecar/pwr_mlx.py"),
            ("PWR_EMBED_SIDECAR", "sidecar/pwr_embed.py"),
        ] {
            if std::env::var_os(variable).is_none() {
                if let Some(path) = bundled(&app, script) {
                    command.env(variable, path);
                }
            }
        }
        if std::env::var_os("PWR_LLAMA_SERVER").is_none() {
            let llama_bin = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
            let llama_nested = format!("llama/{llama_bin}");
            if let Some(path) = bundled(&app, &llama_bin).or_else(|| bundled(&app, &llama_nested)) {
                command.env("PWR_LLAMA_SERVER", path);
            }
        }
        if let Some(previous) = core.0.lock().map_err(|_| "core state poisoned")?.take() {
            previous.stop();
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", program.display()))?;
        #[cfg(unix)]
        let guardian = match supervision::Guardian::group(child.id()) {
            Ok(guardian) => guardian,
            Err(error) => {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", &format!("-{}", child.id())])
                    .output();
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not supervise the core: {error}"));
            }
        };
        let stdin = child.stdin.take().ok_or("the core has no input")?;
        let stdout = child.stdout.take().ok_or("the core has no output")?;
        let stderr = child.stderr.take().ok_or("the core has no error output")?;

        if let Some(previous) = core
            .0
            .lock()
            .map_err(|_| "core state poisoned")?
            .replace(Running {
                child,
                stdin,
                #[cfg(unix)]
                _guardian: Some(guardian),
                #[cfg(unix)]
                _engine_usage: engine_usage,
                #[cfg(debug_assertions)]
                trace_path,
                #[cfg(debug_assertions)]
                trace_owned,
            })
        {
            // Stop the old core on a background thread so starting the new one never waits.
            std::thread::spawn(move || previous.stop());
        }

        let events = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(message) => {
                        let _ = events.emit(
                            "acp",
                            serde_json::json!({"generation": generation, "message": message}),
                        );
                    }
                    Err(_) => {
                        let _ = events.emit(
                            "core-log",
                            serde_json::json!({"generation": generation, "line": line}),
                        );
                    }
                }
            }
            let _ = events.emit("core-exit", generation);
        });
        let logs = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = logs.emit(
                    "core-log",
                    serde_json::json!({"generation": generation, "line": line}),
                );
            }
        });
        // Remembered only once the core has started there, so a folder that
        // cannot be opened is not reopened next time.
        if !chat_home().is_ok_and(|home| home == workspace) {
            if let Some(file) = last_workspace_file(&app) {
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(file, workspace.display().to_string());
            }
        }
        Ok(Started {
            core: program.display().to_string(),
            workspace: workspace.display().to_string(),
            generation,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// One protocol message, written as one line.
#[tauri::command]
fn core_send(core: State<'_, Core>, message: serde_json::Value) -> Result<(), String> {
    let mut guard = core.0.lock().map_err(|_| "core state poisoned")?;
    let running = guard.as_mut().ok_or("the core is not running")?;
    let mut line = serde_json::to_string(&message).map_err(|error| error.to_string())?;
    line.push('\n');
    running
        .stdin
        .write_all(line.as_bytes())
        .and_then(|()| running.stdin.flush())
        .map_err(|error| format!("the core stopped reading: {error}"))
}

/// Opens a web page in the person's browser. Only http(s): the interface
/// links to the Hub and to what models write, never to local files or apps.
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    let parsed = url.trim();
    let scheme = parsed
        .split_once("://")
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    if !matches!(scheme.as_deref(), Some("http" | "https")) || parsed.chars().any(char::is_control)
    {
        return Err(format!("Not a web address: {parsed}"));
    }
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    command
        .arg(parsed)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open the browser: {error}"))
}

#[tauri::command]
fn core_stop(core: State<'_, Core>) {
    if let Ok(mut guard) = core.0.lock() {
        if let Some(running) = guard.take() {
            running.stop();
        }
    }
}

/// A diagnostic export exists only in a debug app. The UI gate is for
/// discoverability; omitting this command from release builds is the actual
/// access boundary. The sidecar's private raw output is filtered to the
/// conversation's timeline before it is written to the chosen destination.
#[cfg(debug_assertions)]
#[tauri::command]
fn debug_export_chat(
    core: State<'_, Core>,
    destination: String,
    conversation: serde_json::Value,
) -> Result<(), String> {
    let timeline = conversation["timeline"]
        .as_array()
        .ok_or("conversation timeline is missing")?;
    let start = timeline
        .first()
        .and_then(|entry| entry["at"].as_u64())
        .ok_or("conversation has no dated entries")?;
    let end = timeline
        .last()
        .and_then(|entry| entry["at"].as_u64())
        .ok_or("conversation has no dated entries")?
        .saturating_add(2_000);
    let trace_path = core
        .0
        .lock()
        .map_err(|_| "core state poisoned")?
        .as_ref()
        .ok_or("the core is not running")?
        .trace_path
        .clone();
    let mlx_raw = match std::fs::File::open(&trace_path) {
        Ok(file) => trace_records_in_window(BufReader::new(file), start, end)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.to_string()),
    };
    let export = serde_json::json!({
        "schema_version": 1,
        "kind": "pwr_development_chat_diagnostic",
        "conversation": conversation,
        "mlx_raw": mlx_raw,
    });
    let bytes = serde_json::to_vec_pretty(&export).map_err(|error| error.to_string())?;
    std::fs::write(destination, bytes).map_err(|error| error.to_string())
}

#[cfg(debug_assertions)]
fn trace_records_in_window(
    reader: impl BufRead,
    start: u64,
    end: u64,
) -> Result<Vec<serde_json::Value>, String> {
    let mut mlx_raw = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue; // A generation may still be appending its last line.
        };
        if record["at_ms"]
            .as_u64()
            .is_some_and(|at| (start..=end).contains(&at))
        {
            mlx_raw.push(record);
        }
    }
    Ok(mlx_raw)
}

#[cfg(all(test, debug_assertions))]
mod diagnostic_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn core_shutdown_allows_eof_cleanup_before_escalation() {
        use std::os::unix::process::CommandExt;
        let path = std::env::temp_dir().join(format!(
            "pwr-shutdown-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut child = Command::new("/bin/sh")
            .args(["-c", "cat >/dev/null; printf cleaned > \"$1\"", "fixture"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        let guardian = supervision::Guardian::group(child.id()).unwrap();
        let stdin = child.stdin.take().unwrap();
        Running {
            child,
            stdin,
            _guardian: Some(guardian),
            _engine_usage: None,
            trace_path: path.clone(),
            trace_owned: false,
        }
        .stop();
        let actual = std::fs::read_to_string(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(actual.unwrap(), "cleaned");
    }

    #[cfg(unix)]
    fn assert_core_group_stops(script: &str, forced: bool) {
        use std::os::unix::process::CommandExt;
        let path = std::env::temp_dir().join(format!("pwr-core-worker-{}", uuid::Uuid::new_v4()));
        let mut child = Command::new("/bin/sh")
            .args(["-c", script, "fixture"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        let guardian = supervision::Guardian::group(child.id()).unwrap();
        let stdin = child.stdin.take().unwrap();
        let running = Running {
            child,
            stdin,
            _guardian: Some(guardian),
            _engine_usage: None,
            trace_path: path.clone(),
            trace_owned: false,
        };
        let ready_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !path.exists() && std::time::Instant::now() < ready_deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Always stop the owned group, including when fixture readiness fails.
        let start = std::time::Instant::now();
        running.stop();
        let elapsed = start.elapsed();
        let worker: u32 = std::fs::read_to_string(&path)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let _ = std::fs::remove_file(path);
        assert!(worker > 1);
        let gone_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let state = Command::new("ps")
                .args(["-o", "stat=", "-p", &worker.to_string()])
                .output()
                .unwrap();
            let state = String::from_utf8_lossy(&state.stdout);
            if state.trim().is_empty() || state.trim().starts_with('Z') {
                break;
            }
            assert!(
                std::time::Instant::now() < gone_deadline,
                "core worker survived shutdown: {state}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "shutdown exceeded its bound: {elapsed:?}"
        );
        if forced {
            assert!(
                elapsed >= std::time::Duration::from_secs(3),
                "fixture did not require escalation"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn core_shutdown_removes_worker_left_after_cooperative_exit() {
        assert_core_group_stops("sleep 60 & echo $! > \"$1\"; cat >/dev/null", false);
    }

    #[cfg(unix)]
    #[test]
    fn core_shutdown_escalates_when_eof_is_ignored_and_removes_workers() {
        assert_core_group_stops(
            "trap '' TERM HUP; sleep 60 & echo $! > \"$1\"; while :; do sleep 1; done",
            true,
        );
    }

    #[test]
    fn raw_model_output_is_scoped_to_the_exported_chat() {
        let lines = concat!(
            "{\"at_ms\":90,\"raw\":\"other chat\"}\n",
            "{\"at_ms\":150,\"raw\":\"current chat\"}\n",
            "{\"raw\":\"undated\"}\n",
            "{\"at_ms\":230,\"raw\":\"later chat\"}\n",
        );
        let selected = trace_records_in_window(lines.as_bytes(), 100, 200).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0]["raw"], "current chat");
    }
}

fn main_only(
    handler: impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    move |invoke| {
        if invoke.message.webview().label() != "main" {
            invoke
                .resolver
                .reject("Remote browser pages cannot invoke PWR host commands.");
            true
        } else {
            handler(invoke)
        }
    }
}

#[cfg(target_os = "macos")]
fn guarded_quit_menu(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let menu = Menu::default(app)?;
    // Tauri 2.11's first submenu is the application menu, ending in its
    // predefined Quit. That native item invokes NSApp terminate directly,
    // bypassing the asynchronous unsaved-buffer confirmation.
    if let Some(MenuItemKind::Submenu(application)) = menu.items()?.first() {
        let items = application.items()?;
        if let Some(MenuItemKind::Predefined(quit)) = items.last() {
            let title = quit.text()?;
            if title.starts_with("Quit ") || title == "Quit" {
                application.remove(quit)?;
                application.append(&MenuItem::with_id(
                    app,
                    "pwr-guarded-quit",
                    title,
                    true,
                    Some("CmdOrCtrl+Q"),
                )?)?;
            } else {
                return Err(std::io::Error::other(
                    "Unexpected application menu: Quit guard not installed",
                )
                .into());
            }
        } else {
            return Err(std::io::Error::other("Application menu has no Quit item to guard").into());
        }
    } else {
        return Err(std::io::Error::other("Application menu missing").into());
    }
    app.set_menu(menu)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main"
                    && !window
                        .state::<Closing>()
                        .0
                        .load(std::sync::atomic::Ordering::SeqCst)
                {
                    api.prevent_close();
                    let _ = window.emit("app-close-requested", ());
                }
            }
        })
        .on_page_load(|webview, payload| {
            // A frontend reload must not leave a browser page alive
            // after component disposal is skipped.
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Started
            {
                if let Some(browser) = webview.app_handle().try_state::<browser::Browser>() {
                    browser.close_all(webview.app_handle());
                }
            }
        })
        .manage(Core::default())
        .manage(updater::Updates::default())
        .manage(drafts::Drafts::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Closing::default())
        .manage(engine::Setup::default())
        .manage(terminal::Terminals::default())
        .manage(browser::Browser::default())
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "pwr-guarded-quit" {
                let _ = app.emit_to("main", "app-close-requested", ());
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            guarded_quit_menu(app.handle())?;
            // Warm up login PATH on a background thread so the first core launch never waits.
            std::thread::spawn(|| {
                let _ = login_path();
            });
            // Full screen gets a glass title bar over the app, not a white strip.
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                let watched = window.clone();
                window.on_window_event(move |event| {
                    if matches!(
                        event,
                        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Focused(true)
                    ) {
                        titlebar::follow(&watched);
                    }
                });
            }
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        });
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(main_only(tauri::generate_handler![
        drafts::drafts_read,
        drafts::drafts_write,
        updater::update_status,
        updater::update_check,
        updater::update_download,
        updater::update_install,
        updater::update_restart,
        default_workspace,
        chat_home_path,
        workspace_is_trusted,
        trust_workspace,
        core_start,
        core_send,
        core_stop,
        open_external,
        app_close_confirmed,
        engine::engine_status,
        engine::engine_install,
        engine::engine_cancel,
        terminal::term_open,
        terminal::term_ack,
        terminal::term_write,
        terminal::term_resize,
        terminal::term_close,
        browser::browser_open,
        browser::browser_sync,
        browser::browser_action,
        browser::browser_close,
        debug_export_chat,
    ]));
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(main_only(tauri::generate_handler![
        drafts::drafts_read,
        drafts::drafts_write,
        updater::update_status,
        updater::update_check,
        updater::update_download,
        updater::update_install,
        updater::update_restart,
        default_workspace,
        chat_home_path,
        workspace_is_trusted,
        trust_workspace,
        core_start,
        core_send,
        core_stop,
        open_external,
        app_close_confirmed,
        engine::engine_status,
        engine::engine_install,
        engine::engine_cancel,
        terminal::term_open,
        terminal::term_ack,
        terminal::term_write,
        terminal::term_resize,
        terminal::term_close,
        browser::browser_open,
        browser::browser_sync,
        browser::browser_action,
        browser::browser_close,
    ]));
    builder
        .build(tauri::generate_context!())
        .expect("error while building the PWR app")
        .run(|app, event| {
            if let RunEvent::ExitRequested { api, .. } = &event {
                if !app
                    .state::<Closing>()
                    .0
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    api.prevent_exit();
                    let _ = app.emit_to("main", "app-close-requested", ());
                }
            }
            // The core must not outlive the window: it holds a model.
            if let RunEvent::Exit = event {
                app.state::<browser::Browser>().close_all(app);
                // Nor may the person's shells.
                app.state::<terminal::Terminals>().close_all();
                if let Ok(mut guard) = app.state::<Core>().0.lock() {
                    if let Some(running) = guard.take() {
                        running.stop();
                    }
                }
            }
        });
}

/// Support-only offline maintenance, before initializing Tauri or starting a core.
pub fn engine_maintenance_cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--engine-maintenance") {
        return None;
    }
    #[cfg(target_os = "macos")]
    let result = engine_maintenance::cli(&args[1..]);
    #[cfg(not(target_os = "macos"))]
    let result: std::io::Result<()> = Err(std::io::Error::other(
        "Managed MLX maintenance is supported on macOS only",
    ));
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Engine maintenance refused: {error}");
            1
        }
    })
}
