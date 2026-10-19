//! Visible PTYs: private login shells and separate model processes.
//! Model launches receive the core's policy-derived sandbox and a clean
//! environment; the frontend enforces conversation ownership and input control.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};

type TerminalInput = (Vec<u8>, mpsc::Sender<Result<(), String>>);

fn input_queue(mut writer: Box<dyn Write + Send>) -> mpsc::SyncSender<TerminalInput> {
    let (sender, receiver) = mpsc::sync_channel::<TerminalInput>(16);
    std::thread::spawn(move || {
        while let Ok((bytes, reply)) = receiver.recv() {
            let result = writer
                .write_all(&bytes)
                .and_then(|()| writer.flush())
                .map_err(|error| error.to_string());
            let failed = result.is_err();
            let _ = reply.send(result);
            if failed {
                break;
            }
        }
    });
    sender
}
use tauri::{AppHandle, Emitter, Manager, State};

/// One output frame in flight per PTY. Reading resumes only after xterm has
/// consumed it; a hidden or stalled webview cannot queue unbounded IPC output.
#[derive(Default)]
struct OutputFlow {
    state: Mutex<(u64, u64, bool)>, // sent, consumed, closed
    ready: Condvar,
}
impl OutputFlow {
    fn wait(&self) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        let Ok(state) = self.ready.wait_while(state, |(sent, consumed, closed)| {
            sent > consumed && !*closed
        }) else {
            return false;
        };
        !state.2
    }
    fn sent(&self) -> Option<u64> {
        let mut state = self.state.lock().ok()?;
        if state.2 {
            return None;
        }
        state.0 += 1;
        Some(state.0)
    }
    fn acknowledge(&self, sequence: u64) {
        if let Ok(mut state) = self.state.lock() {
            if sequence == state.0 && sequence > state.1 {
                state.1 = sequence;
                self.ready.notify_all();
            }
        }
    }
    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.2 = true;
            self.ready.notify_all();
        }
    }
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: mpsc::SyncSender<TerminalInput>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<OutputFlow>,
}

#[derive(Default)]
pub struct Terminals {
    sessions: Mutex<HashMap<u32, Session>>,
    next: AtomicU32,
}

impl Terminals {
    pub fn has_sessions(&self) -> bool {
        self.sessions.lock().map(|s| !s.is_empty()).unwrap_or(true)
    }
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
    session.output.close();
    #[cfg(unix)]
    {
        let mut groups = Vec::new();
        if let Some(pid) = session.child.process_id() {
            groups.push(i64::from(pid));
        }
        if let Some(group) = session.master.process_group_leader() {
            groups.push(i64::from(group));
        }
        groups.sort_unstable();
        groups.dedup();
        for group in groups.into_iter().filter(|group| *group > 1) {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-KILL", &format!("-{group}")])
                .output();
        }
    }
    #[cfg(windows)]
    if let Some(pid) = session.child.process_id() {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
    if !matches!(session.child.try_wait(), Ok(Some(_))) {
        let _ = session.child.kill();
    }
    let _ = session.child.wait();
}

#[derive(Clone, Serialize)]
struct Output {
    id: u32,
    sequence: u64,
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
    #[serde(default)]
    pub args: Vec<String>,
    pub profile: Option<String>,
    pub sandboxed: bool,
    pub path: Option<String>,
}

pub fn open_terminal(
    app: AppHandle,
    terminals: &Terminals,
    cwd: String,
    cols: u16,
    rows: u16,
    launch: Option<Launch>,
) -> Result<u32, String> {
    if let Some(launch) = &launch {
        if launch.executable.trim().is_empty()
            || launch.executable.contains('\0')
            || launch.args.iter().any(|arg| arg.contains('\0'))
        {
            return Err("Invalid process arguments.".into());
        }
        if launch.sandboxed && (!cfg!(target_os = "macos") || launch.profile.is_none()) {
            return Err("A sandbox was required but no profile was provided.".into());
        }
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
            command.args([
                "-p",
                launch.profile.as_deref().ok_or("Missing sandbox profile")?,
            ]);
            command.arg(&launch.executable);
            command
        } else {
            CommandBuilder::new(&launch.executable)
        };
        command.args(&launch.args);
        command.env_clear();
        command.env("LANG", "en_US.UTF-8");
        let scratch = Path::new(&cwd).join(".pwr-scratch");
        std::fs::create_dir_all(scratch.join("home")).map_err(|why| why.to_string())?;
        std::fs::create_dir_all(scratch.join("tmp")).map_err(|why| why.to_string())?;
        command.env("HOME", scratch.join("home"));
        command.env("USERPROFILE", scratch.join("home"));
        command.env("TMPDIR", scratch.join("tmp"));
        command.env("TMP", scratch.join("tmp"));
        command.env("TEMP", scratch.join("tmp"));
        #[cfg(windows)]
        for key in [
            "SystemRoot",
            "WINDIR",
            "COMSPEC",
            "PATHEXT",
            "SYSTEMDRIVE",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "CommonProgramFiles",
            "APPDATA",
            "LOCALAPPDATA",
        ] {
            if let Some(val) = std::env::var_os(key) {
                command.env(key, val);
            }
        }
        command.env(
            "PATH",
            launch.path.clone().unwrap_or_else(crate::login_path),
        );
        command
    } else {
        let mut command = CommandBuilder::new(shell());
        if !cfg!(windows) {
            command.arg("-l");
        }
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
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| error.to_string())?;
    let id = terminals.next.fetch_add(1, Ordering::Relaxed) + 1;
    let output = Arc::new(OutputFlow::default());
    terminals
        .sessions
        .lock()
        .map_err(|_| "the terminals are unavailable")?
        .insert(
            id,
            Session {
                master: pair.master,
                writer: input_queue(writer),
                child,
                output: output.clone(),
            },
        );
    std::thread::spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            if !output.wait() {
                break;
            }
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    if !pending.is_empty() {
                        let data = String::from_utf8_lossy(&pending).into_owned();
                        if let Some(sequence) = output.sent() {
                            let _ =
                                app.emit_to("main", "term-output", Output { id, sequence, data });
                        }
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
                    let Some(sequence) = output.sent() else {
                        break;
                    };
                    if app
                        .emit_to("main", "term-output", Output { id, sequence, data })
                        .is_err()
                    {
                        break;
                    }
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
        let _ = app.emit_to("main", "term-exit", id);
    });
    Ok(id)
}

#[tauri::command]
pub fn term_ack(
    caller: tauri::Webview,
    terminals: State<'_, Terminals>,
    id: u32,
    sequence: u64,
) -> Result<(), String> {
    if caller.label() != "main" {
        return Err("Only PWR consumes terminal output.".into());
    }
    let sessions = terminals.sessions.lock().map_err(|_| "unavailable")?;
    if let Some(session) = sessions.get(&id) {
        session.output.acknowledge(sequence);
    }
    Ok(())
}

#[tauri::command]
pub async fn term_write(
    terminals: State<'_, Terminals>,
    id: u32,
    data: String,
) -> Result<(), String> {
    if data.len() > 64 * 1024 {
        return Err("Terminal input exceeds 64 KiB; send smaller chunks.".into());
    }
    let writer = {
        let sessions = terminals.sessions.lock().map_err(|_| "unavailable")?;
        sessions
            .get(&id)
            .ok_or("the terminal has closed")?
            .writer
            .clone()
    };
    let (reply, received) = mpsc::channel();
    writer
        .try_send((data.into_bytes(), reply))
        .map_err(|error| format!("Terminal input queue unavailable: {error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        received.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "Terminal input did not finish; it may still be pending. Inspect or close the terminal before retrying.".to_string())?
    }).await.map_err(|error| error.to_string())?
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
    use super::*;

    #[cfg(unix)]
    #[test]
    fn closing_a_pty_terminates_its_worker_group() {
        use std::io::BufRead as _;
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "trap '' HUP; sleep 60 & echo $!; wait"]);
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut reader = std::io::BufReader::new(pair.master.try_clone_reader().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let worker: u32 = line.trim().parse().unwrap();
        assert!(worker > 1);
        let writer = input_queue(pair.master.take_writer().unwrap());
        end(Session {
            master: pair.master,
            writer,
            child,
            output: Default::default(),
        });
        let state = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &worker.to_string()])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&state.stdout);
        assert!(
            state.trim().is_empty() || state.trim().starts_with('Z'),
            "worker remains active: {state}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn pty_output_waits_for_consumption_and_close_releases_backpressure() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let child = pair
            .slave
            .spawn_command(CommandBuilder::new("/usr/bin/yes"))
            .unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = input_queue(pair.master.take_writer().unwrap());
        let output = Arc::new(OutputFlow::default());
        let flow = output.clone();
        let (sent, frames) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut buffer = [0u8; 16 * 1024];
            while flow.wait() {
                let Ok(count) = reader.read(&mut buffer) else {
                    break;
                };
                if count == 0 {
                    break;
                }
                let Some(sequence) = flow.sent() else {
                    break;
                };
                sent.send((sequence, count)).unwrap();
            }
        });
        let (sequence, bytes) = frames
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert_eq!(sequence, 1);
        assert!(bytes <= 16 * 1024);
        assert!(frames
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        output.acknowledge(99); // Future and duplicate acknowledgements grant no credit.
        assert!(frames
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        output.acknowledge(1);
        assert_eq!(
            frames
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap()
                .0,
            2
        );
        output.acknowledge(1);
        assert!(frames
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        end(Session {
            master: pair.master,
            writer,
            child,
            output,
        });
        worker.join().unwrap();
    }

    struct TestWriter {
        output: std::sync::Arc<Mutex<Vec<u8>>>,
        gate: Option<mpsc::Receiver<()>>,
        entered: Option<mpsc::Sender<()>>,
    }
    impl Write for TestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
            }
            if let Some(gate) = self.gate.take() {
                gate.recv().map_err(std::io::Error::other)?;
            }
            self.output.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn input_queues_are_bounded_ordered_and_independent() {
        let output = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (release, gate) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        let queue = input_queue(Box::new(TestWriter {
            output: output.clone(),
            gate: Some(gate),
            entered: Some(entered),
        }));
        let (reply, replies) = mpsc::channel();
        queue.try_send((vec![0], reply.clone())).unwrap();
        started
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        for byte in 1..=16 {
            queue.try_send((vec![byte], reply.clone())).unwrap();
        }
        assert!(matches!(
            queue.try_send((vec![17], reply)),
            Err(mpsc::TrySendError::Full(_))
        ));
        let other = input_queue(Box::new(TestWriter {
            output: Default::default(),
            gate: None,
            entered: None,
        }));
        let (answer, received) = mpsc::channel();
        other.try_send((vec![42], answer)).unwrap();
        received
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
            .unwrap();
        release.send(()).unwrap();
        for _ in 0..17 {
            replies
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap()
                .unwrap();
        }
        assert_eq!(*output.lock().unwrap(), (0..=16).collect::<Vec<u8>>());
    }
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
pub fn term_open(
    app: AppHandle,
    caller: tauri::Webview,
    terminals: State<'_, Terminals>,
    cwd: String,
    cols: u16,
    rows: u16,
    launch: Option<Launch>,
) -> Result<u32, String> {
    if caller.label() != "main" {
        return Err("Only PWR controls its terminals.".into());
    }
    let updates = app.state::<crate::updater::Updates>();
    let _activity = updates
        .activity
        .lock()
        .map_err(|_| "Activity state unavailable")?;
    updates.allow_activity()?;
    open_terminal(app.clone(), &terminals, cwd, cols, rows, launch)
}
