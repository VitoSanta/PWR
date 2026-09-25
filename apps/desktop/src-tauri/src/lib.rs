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

mod engine;

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
    #[cfg(debug_assertions)]
    trace_path: PathBuf,
    #[cfg(debug_assertions)]
    trace_owned: bool,
}

impl Running {
    fn stop(mut self) {
        // Closing its input is how the core is told to finish; the kill is for
        // one that does not.
        drop(self.stdin);
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
    root.join("Cargo.toml").is_file().then(|| root.to_path_buf())
}

/// A file shipped in the app's resources, if this copy of the app has it.
fn bundled(app: &AppHandle, relative: &str) -> Option<PathBuf> {
    app.path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(relative))
        .filter(|path| path.is_file())
}

/// `PWR_CORE`, the bundled core, the checkout build, then `pwr` on PATH.
fn core_path(app: &AppHandle) -> PathBuf {
    if let Some(path) = std::env::var_os("PWR_CORE") {
        return PathBuf::from(path);
    }
    if let Some(path) = bundled(app, "pwr") {
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
    let path = std::env::var_os("PWR_CHAT_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".pwr/chat")))
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

/// `PWR_WORKSPACE`, else the workspace opened last (if it still exists),
/// else the checkout's site, else the home folder.
#[tauri::command]
fn default_workspace(app: AppHandle) -> String {
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
    let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    // On a Mac the app runs MLX only; llama.cpp arrives with Windows. A
    // development build still honours `PWR_BACKEND` so the GGUF path can be
    // worked on; a release build never switches engine from the environment.
    let default_backend = if cfg!(target_os = "macos") { "mlx" } else { "llama" };
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
            let path = directory.join(format!("core-{}-{generation}-{nonce}.jsonl", std::process::id()));
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
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let stdin = child.stdin.take().ok_or("the core has no input")?;
    let stdout = child.stdout.take().ok_or("the core has no output")?;
    let stderr = child.stderr.take().ok_or("the core has no error output")?;

    if let Some(previous) = core.0.lock().map_err(|_| "core state poisoned")?.replace(Running {
        child,
        stdin,
        #[cfg(debug_assertions)]
        trace_path,
        #[cfg(debug_assertions)]
        trace_owned,
    }) {
        previous.stop();
    }

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

/// Opens a web page in the person's browser. Only http(s): the interface
/// links to the Hub and to what models write, never to local files or apps.
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    let parsed = url.trim();
    let scheme = parsed.split_once("://").map(|(scheme, _)| scheme.to_ascii_lowercase());
    if !matches!(scheme.as_deref(), Some("http" | "https")) || parsed.chars().any(char::is_control) {
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
    use super::trace_records_in_window;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(Core::default())
        .manage(engine::Setup::default())
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
        });
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        default_workspace, chat_home_path, workspace_is_trusted, trust_workspace,
        core_start, core_send, core_stop, open_external,
        engine::engine_status, engine::engine_install, engine::engine_cancel,
        debug_export_chat,
    ]);
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        default_workspace, chat_home_path, workspace_is_trusted, trust_workspace,
        core_start, core_send, core_stop, open_external,
        engine::engine_status, engine::engine_install, engine::engine_cancel,
    ]);
    builder
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
