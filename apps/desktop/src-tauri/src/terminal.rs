//! Visible PTYs: private login shells and separate model processes.
//! Model launches receive the core's policy-derived sandbox and a clean
//! environment; the frontend enforces conversation ownership and input control.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct Terminals {
    sessions: Mutex<HashMap<u32, Session>>,
    next: AtomicU32,
}

impl Terminals {
    /// Ends every shell: the terminals must not outlive the window.
    pub fn close_all(&self) {
        let ending: Vec<Session> = match self.sessions.lock() {
            Ok(mut sessions) => sessions.drain().map(|(_, session)| session).collect(),
            Err(_) => return,
        };
        for session in ending {
            end(session);
        }
    }
}

/// Ends a shell and reaps it: killed and never waited for, it stayed a
/// zombie until the app quit.
fn end(mut session: Session) {
    if !matches!(session.child.try_wait(), Ok(Some(_))) {
        let _ = session.child.kill();
    }
    let _ = session.child.wait();
}

#[derive(Clone, Serialize)]
struct Output {
    id: u32,
    data: String,
}

/// The shell the person uses: `$SHELL`, else the platform's usual one.
fn shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".into()
            } else if cfg!(windows) {
                "powershell.exe".into()
            } else {
                "/bin/bash".into()
            }
        })
}

/// The longest prefix of `bytes` that is whole UTF-8, so a character split
/// across two reads is shown once, whole, rather than as two replacement
/// marks.
fn complete_utf8(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => bytes.len(),
    }
}

#[derive(Clone, serde::Deserialize)]
pub struct Launch {
    pub executable: String,
    #[serde(default)] pub args: Vec<String>,
    pub profile: Option<String>,
    pub sandboxed: bool,
    pub path: Option<String>,
}

pub fn open_terminal(
    app: AppHandle, terminals: &Terminals, cwd: String, cols: u16, rows: u16, launch: Option<Launch>,
) -> Result<u32, String> {
    if let Some(launch) = &launch {
        if launch.executable.trim().is_empty() || launch.executable.contains('\0') || launch.args.iter().any(|arg| arg.contains('\0')) { return Err("Invalid process arguments.".into()); }
        if launch.sandboxed && (!cfg!(target_os="macos") || launch.profile.is_none()) { return Err("A sandbox was required but no profile was provided.".into()); }
    }
    if !Path::new(&cwd).is_dir() {
        return Err(format!("{cwd} is not a folder"));
    }
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(2),
            cols: cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("could not open a terminal: {error}"))?;
    let mut command = if let Some(launch) = &launch {
        let mut command = if launch.sandboxed {
            let mut command = CommandBuilder::new("/usr/bin/sandbox-exec");
            command.args(["-p", launch.profile.as_deref().ok_or("Missing sandbox profile")?]);
            command.arg(&launch.executable);
            command
        } else { CommandBuilder::new(&launch.executable) };
        command.args(&launch.args);
        command.env_clear();
        command.env("LANG", "en_US.UTF-8");
        let scratch = Path::new(&cwd).join(".pwr-scratch");
        std::fs::create_dir_all(scratch.join("home")).map_err(|why| why.to_string())?;
        std::fs::create_dir_all(scratch.join("tmp")).map_err(|why| why.to_string())?;
        command.env("HOME", scratch.join("home"));
        command.env("TMPDIR", scratch.join("tmp"));
        command.env("PATH", launch.path.clone().unwrap_or_else(crate::login_path));
        command
    } else {
        let mut command = CommandBuilder::new(shell());
        if !cfg!(windows) { command.arg("-l"); }
        command
    };
    command.cwd(&cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("could not start the shell: {error}"))?;
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| error.to_string())?;
    let writer = pair.master.take_writer().map_err(|error| error.to_string())?;
    let id = terminals.next.fetch_add(1, Ordering::Relaxed) + 1;
    terminals
        .sessions
        .lock()
        .map_err(|_| "the terminals are unavailable")?
        .insert(
            id,
            Session {
                master: pair.master,
                writer,
                child,
            },
        );
    std::thread::spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    if !pending.is_empty() {
                        let data = String::from_utf8_lossy(&pending).into_owned();
                        let _ = app.emit("term-output", Output { id, data });
                    }
                    break;
                }
                Ok(read) => {
                    pending.extend_from_slice(&buffer[..read]);
                    let whole = complete_utf8(&pending);
                    if whole == 0 {
                        continue;
                    }
                    let data = String::from_utf8_lossy(&pending[..whole]).into_owned();
                    pending.drain(..whole);
                    let _ = app.emit("term-output", Output { id, data });
                }
            }
        }
        // The shell ended on its own (exit, or the card closed it): let go of
        // the pseudo-terminal and reap the process rather than keep both.
        let session = app
            .state::<Terminals>()
            .sessions
            .lock()
            .ok()
            .and_then(|mut sessions| sessions.remove(&id));
        if let Some(session) = session {
            end(session);
        }
        let _ = app.emit("term-exit", id);
    });
    Ok(id)
}

#[tauri::command]
pub fn term_write(terminals: State<'_, Terminals>, id: u32, data: String) -> Result<(), String> {
    let mut sessions = terminals.sessions.lock().map_err(|_| "unavailable")?;
    let session = sessions.get_mut(&id).ok_or("the terminal has closed")?;
    session
        .writer
        .write_all(data.as_bytes())
        .and_then(|()| session.writer.flush())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn term_resize(
    terminals: State<'_, Terminals>,
    id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sessions = terminals.sessions.lock().map_err(|_| "unavailable")?;
    let session = sessions.get(&id).ok_or("the terminal has closed")?;
    session
        .master
        .resize(PtySize {
            rows: rows.max(2),
            cols: cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn term_close(terminals: State<'_, Terminals>, id: u32) {
    let session = terminals
        .sessions
        .lock()
        .ok()
        .and_then(|mut sessions| sessions.remove(&id));
    // Off the interface's thread: a shell is given a moment to end on its own
    // before it is killed.
    if let Some(session) = session {
        std::thread::spawn(move || end(session));
    }
}

#[cfg(test)]
mod tests {
    use super::complete_utf8;

    #[test]
    fn a_character_split_across_reads_waits_for_its_end() {
        let e_acute = "è".as_bytes();
        assert_eq!(complete_utf8(b"abc"), 3);
        assert_eq!(complete_utf8(&[b'a', e_acute[0]]), 1);
        assert_eq!(complete_utf8(&[b'a', e_acute[0], e_acute[1]]), 3);
        // Not UTF-8 at all is shown as it is, not held forever.
        assert_eq!(complete_utf8(&[0xff, b'a']), 2);
    }
}

#[tauri::command]
pub fn term_open(app: AppHandle, caller: tauri::Webview, terminals: State<'_, Terminals>, cwd: String, cols: u16, rows: u16, launch: Option<Launch>) -> Result<u32, String> {
    if caller.label() != "main" { return Err("Only PWR controls its terminals.".into()); }
    open_terminal(app, &terminals, cwd, cols, rows, launch)
}
