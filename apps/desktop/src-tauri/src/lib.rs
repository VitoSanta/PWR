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

#[derive(Default)]
struct Core(Mutex<Option<Running>>);

/// Which start of the core an event belongs to. Opening another workspace
/// stops the old core, and its exit arriving after the new one started was
/// read as the new one dying (2026-09-22: "Error: the core stopped" on every
/// workspace change, with the new core healthy).
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Running {
    child: Child,
    stdin: ChildStdin,
}

impl Running {
    fn stop(mut self) {
        // Closing its input is how the core is told to finish; the kill is for
        // one that does not.
        drop(self.stdin);
        let _ = self.child.kill();
        let _ = self.child.wait();
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
    root.join("Cargo.toml").is_file().then(|| root.to_path_buf())
}

/// `POORAI_CORE`, the bundled core, the checkout build, then `pwr` on PATH.
fn core_path(app: &AppHandle) -> PathBuf {
    if let Some(path) = std::env::var_os("POORAI_CORE") {
        return PathBuf::from(path);
    }
    if let Some(path) = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("pwr"))
        .filter(|path| path.is_file())
    {
        return path;
    }
    repository()
        .map(|root| {
            root.join(if cfg!(debug_assertions) {
                "target/debug/pwr"
            } else {
                "target/release/pwr"
            })
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("pwr"))
}

/// The PATH a terminal would have. An app opened from the Finder inherits
/// launchd's `/usr/bin:/bin:/usr/sbin:/sbin`, where the checks a workspace
/// declares (`npm`, `node`, `cargo`) are not found.
fn login_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let from_shell = Command::new(shell)
        .args(["-lc", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let known = [
        "/opt/homebrew/bin".to_owned(),
        "/usr/local/bin".to_owned(),
        format!("{home}/.cargo/bin"),
    ];
    let mut parts: Vec<String> = Vec::new();
    for part in from_shell
        .split(':')
        .chain(current.split(':'))
        .map(str::to_owned)
        .chain(known)
    {
        if !part.is_empty() && !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join(":")
}

/// Where the app remembers the workspace it last opened.
fn last_workspace_file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("last-workspace"))
}

fn chat_home() -> Result<PathBuf, String> {
    let path = std::env::var_os("POORAI_CHAT_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".poorai/chat")))
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
    let paths: Vec<String> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: {error}", file.display()))?;
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
    Ok(read_trusted_workspaces(app)?.iter().any(|path| path == workspace))
}

#[tauri::command]
fn chat_home_path() -> Result<String, String> {
    chat_home().map(|path| path.display().to_string())
}

#[tauri::command]
fn workspace_is_trusted(app: AppHandle, workspace: String) -> Result<bool, String> {
    let path = canonical_workspace(&workspace)?;
    is_workspace_trusted(&app, &path)
}

#[tauri::command]
fn trust_workspace(app: AppHandle, workspace: String) -> Result<(), String> {
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
    std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let bytes = serde_json::to_vec_pretty(&paths).map_err(|error| error.to_string())?;
    let temporary = file.with_extension("json.tmp");
    std::fs::write(&temporary, bytes).map_err(|error| format!("{}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, &file).map_err(|error| format!("{}: {error}", file.display()))
}

/// `POORAI_WORKSPACE`, else the workspace opened last (if it still exists),
/// else the checkout's site, else the home folder.
#[tauri::command]
fn default_workspace(app: AppHandle) -> String {
    std::env::var("POORAI_WORKSPACE")
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
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_else(|| ".".into())
}

#[tauri::command]
fn core_start(app: AppHandle, core: State<'_, Core>, workspace: String) -> Result<Started, String> {
    let workspace = canonical_workspace(&workspace)?;
    if !is_workspace_trusted(&app, &workspace)? {
        return Err(format!("Workspace has not been trusted: {}", workspace.display()));
    }
    let program = core_path(&app);
    let backend = std::env::var("POORAI_BACKEND").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") { "mlx" } else { "llama" }.into()
    });
    let mut command = Command::new(&program);
    command
        .args(["--backend", &backend, "serve", "--stdio"])
        .current_dir(&workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env("PATH", login_path());
    // As the launcher does: the checkout's MLX interpreter when the
    // environment names none -- the one `scripts/setup-mlx.sh` creates, then
    // the engine spike's on the maintainer's machine.
    if std::env::var_os("POORAI_MLX_PYTHON").is_none() {
        if let Some(python) = repository().and_then(|root| {
            [
                ".venv-mlx/bin/python",
                "experiments/engine-spike-mlx-20260917/.venv/bin/python",
            ]
            .into_iter()
            .map(|candidate| root.join(candidate))
            .find(|path| path.is_file())
        }) {
            command.env("POORAI_MLX_PYTHON", python);
        }
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let stdin = child.stdin.take().ok_or("the core has no input")?;
    let stdout = child.stdout.take().ok_or("the core has no output")?;
    let stderr = child.stderr.take().ok_or("the core has no error output")?;

    if let Some(previous) = core.0.lock().map_err(|_| "core state poisoned")?.replace(Running { child, stdin }) {
        previous.stop();
    }

    let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let events = app.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(message) => {
                    let _ = events.emit("acp", message);
                }
                Err(_) => {
                    let _ = events.emit("core-log", line);
                }
            }
        }
        let _ = events.emit("core-exit", generation);
    });
    let logs = app.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = logs.emit("core-log", line);
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

/// Restore one path captured by a conversation diff, constrained to its workspace.
#[tauri::command]
fn restore_workspace_file(workspace: String, path: String, content: String, remove: bool) -> Result<(), String> {
    let root = PathBuf::from(&workspace).canonicalize().map_err(|e| e.to_string())?;
    let relative = Path::new(&path);
    if relative.is_absolute() || relative.components().any(|part| !matches!(part, std::path::Component::Normal(_))) {
        return Err("The changed path is not a safe workspace-relative path".into());
    }
    let target = root.join(relative);
    let parent = target.parent().ok_or("The changed path has no parent")?.canonicalize().map_err(|e| e.to_string())?;
    if !parent.starts_with(&root) || target.is_symlink() {
        return Err("The changed path resolves outside the workspace".into());
    }
    if remove {
        if target.exists() { std::fs::remove_file(target).map_err(|e| e.to_string())?; }
    } else {
        std::fs::write(target, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn core_stop(core: State<'_, Core>) {
    if let Ok(mut guard) = core.0.lock() {
        if let Some(running) = guard.take() {
            running.stop();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Core::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            default_workspace,
            chat_home_path,
            workspace_is_trusted,
            trust_workspace,
            core_start,
            core_send,
            restore_workspace_file,
            core_stop
        ])
        .build(tauri::generate_context!())
        .expect("error while building the PWR app")
        .run(|app, event| {
            // The core must not outlive the window: it holds a model.
            if let RunEvent::Exit = event {
                if let Ok(mut guard) = app.state::<Core>().0.lock() {
                    if let Some(running) = guard.take() {
                        running.stop();
                    }
                }
            }
        });
}
