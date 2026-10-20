//! Independent pipe watchers survive the native host long enough to clean its
//! ordinary process groups. They never discover processes by executable name.
#[cfg(unix)]
use std::process::{Command, Stdio};

/// The wrapper's stdin is a lifetime pipe held by the host. The actual command
/// still receives /dev/null. Isolate the returned command in its own group.
#[cfg(unix)]
pub(super) fn installer(command: Command) -> Command {
    const SCRIPT: &str = r#"
exec 3<&0
group=$$
( IFS= read -r ignored <&3; /bin/kill -KILL "-$group" 2>/dev/null ) <&3 &
watcher=$!
"$@" </dev/null 3<&-
result=$?
/bin/kill -KILL "$watcher" 2>/dev/null
wait "$watcher" 2>/dev/null
exit "$result"
"#;
    let mut wrapper = Command::new("/bin/sh");
    wrapper
        .args(["-c", SCRIPT, "pwr-engine-step"])
        .arg(command.get_program())
        .args(command.get_args());
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            wrapper.env(key, value);
        } else {
            wrapper.env_remove(key);
        }
    }
    if let Some(directory) = command.get_current_dir() {
        wrapper.current_dir(directory);
    }
    wrapper.stdin(Stdio::piped());
    wrapper
}

#[cfg(unix)]
pub(super) struct Guardian {
    child: std::process::Child,
    group: Option<u32>,
}

#[cfg(unix)]
impl Guardian {
    pub(super) fn group(pid: u32) -> std::io::Result<Self> {
        if pid <= 1 {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        const SCRIPT: &str = r#"
IFS= read -r ignored
i=0
while [ "$i" -lt 30 ]; do
    /bin/kill -0 "-$1" 2>/dev/null || exit 0
    sleep 0.1
    i=$((i+1))
done
/bin/kill -KILL "-$1" 2>/dev/null
"#;
        let child = Command::new("/bin/sh")
            .args(["-c", SCRIPT, "pwr-core-guardian"])
            .arg(pid.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self {
            child,
            group: Some(pid),
        })
    }

    pub(super) fn terminal(master: &dyn portable_pty::MasterPty) -> std::io::Result<Self> {
        use std::os::fd::BorrowedFd;
        let fd = master.as_raw_fd().ok_or(std::io::ErrorKind::Unsupported)?;
        let tty = master.tty_name().ok_or(std::io::ErrorKind::Unsupported)?;
        // SAFETY: master is borrowed throughout duplication and still owns fd.
        let master_copy =
            rustix::io::fcntl_dupfd_cloexec(unsafe { BorrowedFd::borrow_raw(fd) }, 3)?;
        // Holding a master copy keeps the controlling terminal alive when the
        // host crashes; otherwise HUP can detach jobs before enumeration.
        const SCRIPT: &str = r#"
set -f
IFS= read -r ignored
groups=$(/bin/ps -t "$1" -o pgid= 2>/dev/null)
for group in $groups; do
    case "$group" in ''|*[!0-9]*) continue ;; esac
    if [ "$group" -gt 1 ]; then /bin/kill -KILL "-$group" 2>/dev/null; fi
done
"#;
        let child = Command::new("/bin/sh")
            .args(["-c", SCRIPT, "pwr-terminal-guardian"])
            .arg(tty)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(master_copy))
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self { child, group: None })
    }
}

#[cfg(unix)]
impl Drop for Guardian {
    fn drop(&mut self) {
        if let Some(group) = self.group {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{group}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        drop(self.child.stdin.take());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while matches!(self.child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::os::unix::process::CommandExt;
    use std::path::PathBuf;

    #[test]
    fn supervised_installer_preserves_arguments_environment_and_exit_status() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "if IFS= read -r ignored; then exit 9; fi; printf '%s|%s' \"$1\" \"$PWR_FIXTURE_VALUE\"; exit 7", "fixture", "argument with spaces"])
            .env("PWR_FIXTURE_VALUE", "literal $value");
        let mut command = installer(command);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        // Child::wait closes its own stdin: keep the lifetime handle outside it.
        let _lifetime = child.stdin.take().unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"argument with spaces|literal $value");
    }

    #[test]
    fn native_host_crash_child_fixture() {
        let Some(root) = std::env::var_os("PWR_SUPERVISION_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(root);
        let mode = std::env::var("PWR_SUPERVISION_MODE").unwrap();
        if mode == "terminal" {
            let pair = portable_pty::native_pty_system()
                .openpty(portable_pty::PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .unwrap();
            let mut command = portable_pty::CommandBuilder::new("/bin/zsh");
            command.args([
                "-f",
                "-m",
                "-c",
                "trap '' HUP; /bin/sh -c 'trap \"\" HUP; echo $$; exec sleep 60' & wait",
            ]);
            let child = pair.slave.spawn_command(command).unwrap();
            drop(pair.slave);
            let mut reader = std::io::BufReader::new(pair.master.try_clone_reader().unwrap());
            let worker: u32 = loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if let Ok(pid) = line.trim().parse() {
                    break pid;
                }
            };
            let guardian = if std::env::var_os("PWR_SUPERVISION_NEGATIVE_CONTROL").is_some() {
                None
            } else {
                Some(Guardian::terminal(&*pair.master).unwrap())
            };
            let mut owned = vec![child.process_id().unwrap(), worker];
            if let Some(guardian) = &guardian {
                owned.push(guardian.child.id());
            }
            std::fs::write(root.join("ready.json"), serde_json::to_vec(&owned).unwrap()).unwrap();
            loop {
                std::thread::park();
            }
        } else {
            let worker_file = root.join("worker");
            let mut command = Command::new("/bin/sh");
            command
                .args([
                    "-c",
                    "trap '' HUP; sleep 60 & echo $! > \"$1\"; while :; do sleep 1; done",
                    "fixture",
                ])
                .arg(&worker_file);
            let mut command = if mode == "core" {
                command
            } else {
                installer(command)
            };
            let mut child = command
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
                .unwrap();
            let _core_guardian = if mode == "core" {
                Some(Guardian::group(child.id()).unwrap())
            } else {
                None
            };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !worker_file.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let worker: u32 = std::fs::read_to_string(worker_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let mut owned = vec![child.id(), worker];
            if let Some(guardian) = &_core_guardian {
                owned.push(guardian.child.id());
            }
            std::fs::write(root.join("ready.json"), serde_json::to_vec(&owned).unwrap()).unwrap();
            loop {
                assert!(child.try_wait().unwrap().is_none());
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }

    #[test]
    fn host_sigkill_stops_supervised_installer_and_terminal_jobs() {
        for mode in ["terminal", "installer", "core"] {
            let root =
                std::env::temp_dir().join(format!("pwr-supervision-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "supervision::tests::native_host_crash_child_fixture",
                    "--nocapture",
                ])
                .env("PWR_SUPERVISION_FIXTURE", &root)
                .env("PWR_SUPERVISION_MODE", mode)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let ready = root.join("ready.json");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !ready.exists() && std::time::Instant::now() < deadline {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let reached = ready.exists();
            let _ = child.kill();
            child.wait().unwrap();
            assert!(reached, "{mode} fixture was not ready");
            let pids: Vec<u32> = serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let survived = loop {
                let alive: Vec<_> = pids
                    .iter()
                    .copied()
                    .filter(|pid| {
                        let output = Command::new("/bin/ps")
                            .args(["-o", "stat=", "-p", &pid.to_string()])
                            .output()
                            .unwrap();
                        let state = String::from_utf8_lossy(&output.stdout);
                        !state.trim().is_empty() && !state.trim().starts_with('Z')
                    })
                    .collect();
                if alive.is_empty() || std::time::Instant::now() >= deadline {
                    break alive;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            // Clean owned fixtures even if this test detects a regression.
            for pid in &pids {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", &pid.to_string()])
                    .stderr(Stdio::null())
                    .status();
            }
            std::fs::remove_dir_all(root).unwrap();
            assert!(
                survived.is_empty(),
                "{mode} workers survived host SIGKILL: {survived:?}"
            );
        }
    }
}
