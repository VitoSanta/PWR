//! Stable, isolated prefixes; only the verified selection is replaced.
//! An orphan installer may still write its own prefix after the app dies.
//! Never reuse a prefix or move an environment. Offline maintenance owns the
//! root exclusively; users and installer descendants retain inherited leases.
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const SELECTION: &str = "active-engine.json";
const GENERATIONS: &str = "generations";

/// The directory inode is the common lock for publishers, users and maintenance.
#[cfg(unix)]
pub(super) fn lease(root: &Path, shared: bool) -> io::Result<File> {
    let file: File = rustix::fs::open(
        root,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?
    .into();
    let operation = if shared {
        rustix::fs::FlockOperation::NonBlockingLockShared
    } else {
        rustix::fs::FlockOperation::NonBlockingLockExclusive
    };
    // Another app thread may fork before exec closes an unrelated CLOEXEC
    // copy. Allow the same bounded handoff as publication; never bypass a lock.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
    loop {
        match rustix::fs::flock(&file, operation) {
            Ok(()) => break,
            Err(error)
                if error == rustix::io::Errno::AGAIN && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(file)
}

#[cfg(unix)]
pub(super) fn inherit(file: &File, command: &mut std::process::Command) {
    use std::os::{fd::AsRawFd, unix::process::CommandExt};
    let fd = file.as_raw_fd();
    // SAFETY: fcntl is async-signal-safe; the owner retains file through spawn.
    // Only the child's descriptor changes. Its descendants keep the same flock
    // open-file description, so host death cannot release their use lease.
    unsafe {
        command.pre_exec(move || {
            use std::os::fd::BorrowedFd;
            rustix::io::fcntl_setfd(BorrowedFd::borrow_raw(fd), rustix::io::FdFlags::empty())?;
            Ok(())
        });
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    version: u32,
    generation: String,
}

fn directory(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

#[cfg(unix)]
fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => directory(path),
        Err(error) => Err(error),
    }
}

/// None means a legacy installation; an invalid selection is never a fallback.
pub(super) fn selected(root: &Path) -> io::Result<Option<PathBuf>> {
    let selection = isolated_selection(root)?;
    if selection.is_none() && crate::engine_install::unsettled(root)? {
        return Err(io::Error::other(
            "Previous engine installation requires assisted recovery",
        ));
    }
    Ok(selection)
}

fn isolated_selection(root: &Path) -> io::Result<Option<PathBuf>> {
    match std::fs::symlink_metadata(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
        Ok(_) => directory(root)?,
    }
    let path = root.join(SELECTION);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let mut bytes = Vec::new();
    options.open(path)?.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let selection: Selection = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    let id = uuid::Uuid::parse_str(&selection.generation).map_err(io::Error::other)?;
    if selection.version != 1 || id.to_string() != selection.generation {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let parent = root.join(GENERATIONS);
    directory(&parent)?;
    let prefix = parent.join(selection.generation);
    directory(&prefix)?;
    Ok(Some(prefix))
}

pub(super) struct Generation {
    root: PathBuf,
    prefix: PathBuf,
    id: String,
    // A lock on the root directory, not a removable lock-file name. Normally
    // CLOEXEC; specific installer commands inherit it to block maintenance
    // until their descendants have stopped, even after the host dies.
    _root: File,
}

impl Generation {
    pub(super) fn begin(root: &Path) -> io::Result<Self> {
        #[cfg(not(unix))]
        {
            let _ = root;
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Managed MLX installation requires Unix",
            ));
        }
        #[cfg(unix)]
        {
            let pinned: File = rustix::fs::open(
                root,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )?
            .into();
            // Another thread can fork while we own this descriptor. CLOEXEC
            // closes the child's inherited copy at exec, not at fork. Allow a
            // short bounded handoff rather than misreport that transient as a
            // persistent install; a live publisher still excludes this one.
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
            loop {
                match rustix::fs::flock(
                    &pinned,
                    rustix::fs::FlockOperation::NonBlockingLockExclusive,
                ) {
                    Ok(()) => break,
                    Err(error)
                        if error == rustix::io::Errno::AGAIN
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            // Validate an isolated selection, but never reuse/move the legacy
            // venv or backup. An ambiguous legacy transaction can be bypassed
            // safely by reinstalling into a fresh UUID, even if its worker lives.
            isolated_selection(root)?;
            let parent = root.join(GENERATIONS);
            private_directory(&parent)?;
            let id = uuid::Uuid::new_v4().to_string();
            let prefix = parent.join(&id);
            // UUID collision is an error, not permission to reuse old bytes.
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(&prefix)?;
            std::fs::write(prefix.join(".pwr-generation"), b"1\n")?;
            Ok(Self {
                root: root.to_owned(),
                prefix,
                id,
                _root: pinned,
            })
        }
    }

    pub(super) fn prefix(&self) -> &Path {
        &self.prefix
    }

    pub(super) fn inherit(&self, command: &mut std::process::Command) {
        #[cfg(unix)]
        inherit(&self._root, command);
        #[cfg(not(unix))]
        let _ = command;
    }

    pub(super) fn commit(self, marker: &str) -> io::Result<()> {
        directory(&self.prefix)?;
        directory(&self.prefix.join("venv"))?;
        for path in [
            self.prefix.join(marker),
            self.prefix.join("venv/bin/python"),
        ] {
            // uv's venv Python may legitimately be a symlink to its managed
            // Python within this fixed prefix; setup verified that interpreter.
            if !path.is_file() {
                return Err(io::ErrorKind::NotFound.into());
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let current = std::fs::symlink_metadata(&self.root)?;
            let pinned = self._root.metadata()?;
            if !current.is_dir()
                || current.file_type().is_symlink()
                || (current.dev(), current.ino()) != (pinned.dev(), pinned.ino())
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        let temporary = self
            .root
            .join(format!(".engine-selection-{}.tmp", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(
            &serde_json::to_vec(&Selection {
                version: 1,
                generation: self.id.clone(),
            })
            .map_err(io::Error::other)?,
        )?;
        file.sync_all()?;
        drop(file);
        #[cfg(test)]
        publication_checkpoint("before-rename");
        std::fs::rename(&temporary, self.root.join(SELECTION))?;
        #[cfg(test)]
        publication_checkpoint("after-rename");
        self._root.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
fn publication_checkpoint(point: &str) {
    if std::env::var("PWR_ENGINE_PUBLICATION_PAUSE").as_deref() == Ok(point) {
        let ready = std::env::var_os("PWR_ENGINE_PUBLICATION_READY").unwrap();
        std::fs::write(ready, b"ready").unwrap();
        loop {
            std::thread::park();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("pwr-engine-generation-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        populate(&root, b"original");
        root
    }
    fn populate(prefix: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(prefix.join("venv/bin")).unwrap();
        std::fs::write(prefix.join("venv/bin/python"), bytes).unwrap();
        std::fs::write(prefix.join("marker.json"), bytes).unwrap();
    }
    fn assert_original(root: &Path) {
        assert_eq!(
            std::fs::read(root.join("venv/bin/python")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(root.join("marker.json")).unwrap(),
            b"original"
        );
    }
    #[test]
    fn verified_publication_keeps_legacy_and_previous_generations_unchanged() {
        let root = root();
        assert!(selected(&root).unwrap().is_none());
        let generation = Generation::begin(&root).unwrap();
        let first = generation.prefix().to_owned();
        populate(&first, b"first");
        assert!(selected(&root).unwrap().is_none());
        generation.commit("marker.json").unwrap();
        assert_eq!(selected(&root).unwrap(), Some(first.clone()));
        assert_original(&root);
        let generation = Generation::begin(&root).unwrap();
        let second = generation.prefix().to_owned();
        populate(&second, b"second");
        generation.commit("marker.json").unwrap();
        assert_eq!(selected(&root).unwrap(), Some(second));
        assert_eq!(
            std::fs::read(first.join("venv/bin/python")).unwrap(),
            b"first"
        );
        assert_original(&root);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_install_and_late_writes_never_change_the_selected_engine() {
        let root = root();
        let active = Generation::begin(&root).unwrap();
        populate(active.prefix(), b"active");
        let path = active.prefix().to_owned();
        active.commit("marker.json").unwrap();
        let attempt = Generation::begin(&root).unwrap();
        let incomplete = attempt.prefix().to_owned();
        assert_ne!(path, incomplete);
        assert!(Generation::begin(&root).is_err());
        assert!(attempt.commit("marker.json").is_err());
        let next = Generation::begin(&root).unwrap();
        assert_ne!(next.prefix(), incomplete);
        populate(&incomplete, b"late orphan writes");
        assert_eq!(selected(&root).unwrap(), Some(path.clone()));
        assert_eq!(
            std::fs::read(path.join("venv/bin/python")).unwrap(),
            b"active"
        );
        drop(next);
        assert_original(&root);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_or_external_selection_is_rejected_without_legacy_fallback() {
        let root = root();
        for bytes in [
            br#"{"version":1,"generation":"../../outside"}"#.as_slice(),
            b"{",
            &[b'x'; 4097],
        ] {
            std::fs::write(root.join(SELECTION), bytes).unwrap();
            assert!(selected(&root).is_err());
            assert!(Generation::begin(&root).is_err());
            assert_original(&root);
        }
        std::fs::remove_file(root.join(SELECTION)).unwrap();
        let outside = root.with_extension("outside");
        std::fs::write(&outside, b"preserve").unwrap();
        std::os::unix::fs::symlink(&outside, root.join(SELECTION)).unwrap();
        assert!(selected(&root).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"preserve");
        std::fs::remove_file(outside).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn isolated_retry_recovers_legacy_without_moving_or_deleting_its_files() {
        let root = root();
        std::fs::create_dir(root.join(".install-in-progress")).unwrap();
        let backup = root.join("previous-engine-legacy");
        std::fs::create_dir(&backup).unwrap();
        std::fs::write(backup.join("original"), b"preserve legacy backup").unwrap();
        assert!(selected(&root).is_err());
        let aborted = Generation::begin(&root).unwrap();
        drop(aborted);
        assert!(selected(&root).is_err());
        assert_original(&root);
        let recovery = Generation::begin(&root).unwrap();
        populate(recovery.prefix(), b"verified recovery");
        let prefix = recovery.prefix().to_owned();
        recovery.commit("marker.json").unwrap();
        assert_eq!(selected(&root).unwrap(), Some(prefix));
        assert_original(&root);
        assert!(root.join(".install-in-progress").is_dir());
        assert_eq!(
            std::fs::read(backup.join("original")).unwrap(),
            b"preserve legacy backup"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_worker_late_writes_cannot_change_the_recovered_selection() {
        let root = root();
        std::fs::create_dir(root.join(".install-in-progress")).unwrap();
        let mut worker = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "sleep 0.1; printf legacy-worker > \"$1/venv/bin/python\"",
                "fixture",
            ])
            .arg(&root)
            .spawn()
            .unwrap();
        let recovered = Generation::begin(&root).unwrap();
        populate(recovered.prefix(), b"recovered interpreter");
        let prefix = recovered.prefix().to_owned();
        recovered.commit("marker.json").unwrap();
        assert!(worker.wait().unwrap().success());
        assert_eq!(
            std::fs::read(root.join("venv/bin/python")).unwrap(),
            b"legacy-worker"
        );
        assert_eq!(
            std::fs::read(prefix.join("venv/bin/python")).unwrap(),
            b"recovered interpreter"
        );
        assert_eq!(selected(&root).unwrap(), Some(prefix));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn generation_parent_symlink_is_not_followed() {
        let root = root();
        let outside = root.with_extension("outside");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join(GENERATIONS)).unwrap();
        assert!(Generation::begin(&root).is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir(outside).unwrap();
    }
    #[test]
    fn killed_publication_child_fixture() {
        let Some(root) = std::env::var_os("PWR_GENERATION_CRASH_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(root);
        let generation = Generation::begin(&root).unwrap();
        populate(generation.prefix(), b"verified new engine");
        std::fs::write(
            root.join("fixture-generation"),
            generation.prefix().file_name().unwrap().as_encoded_bytes(),
        )
        .unwrap();
        if std::env::var_os("PWR_GENERATION_LATE_WRITER").is_some() {
            let mut worker = std::process::Command::new("/bin/sh")
                .args(["-c", "i=0; while [ ! -f \"$2\" ] && [ \"$i\" -lt 500 ]; do sleep 0.01; i=$((i+1)); done; if [ -f \"$2\" ]; then printf late-worker > \"$1\"; printf done > \"$3\"; fi", "fixture"])
                .arg(generation.prefix().join("venv/bin/python"))
                .arg(root.join("fixture-release"))
                .arg(root.join("fixture-worker-done"))
                .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                .spawn().unwrap();
            std::fs::write(root.join("fixture-worker"), worker.id().to_string()).unwrap();
            std::fs::write(root.join("fixture-ready"), b"ready").unwrap();
            // Keep the handle alive without waiting; killing this fixture
            // deliberately leaves the bounded worker for the parent probe.
            loop {
                assert!(worker.try_wait().unwrap().is_none());
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        generation.commit("marker.json").unwrap();
        panic!("publication checkpoint was not reached");
    }
    #[test]
    fn kill_before_or_after_atomic_publication_preserves_a_complete_selection() {
        for checkpoint in ["before-rename", "after-rename"] {
            let root = root();
            let initial = Generation::begin(&root).unwrap();
            populate(initial.prefix(), b"initial generation");
            let old = initial.prefix().to_owned();
            initial.commit("marker.json").unwrap();
            let ready = root.join("fixture-ready");
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "engine_generation::tests::killed_publication_child_fixture",
                    "--nocapture",
                ])
                .env("PWR_GENERATION_CRASH_FIXTURE", &root)
                .env("PWR_ENGINE_PUBLICATION_PAUSE", checkpoint)
                .env("PWR_ENGINE_PUBLICATION_READY", &ready)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !ready.exists() && std::time::Instant::now() < deadline {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let reached = ready.exists();
            let concurrent_blocked = Generation::begin(&root).is_err();
            let _ = child.kill();
            child.wait().unwrap();
            assert!(reached, "fixture never reached {checkpoint}");
            assert!(concurrent_blocked);
            let id = std::fs::read_to_string(root.join("fixture-generation")).unwrap();
            let candidate = root.join(GENERATIONS).join(id);
            let expected = if checkpoint == "before-rename" {
                &old
            } else {
                &candidate
            };
            assert_eq!(selected(&root).unwrap().as_ref(), Some(expected));
            assert_eq!(
                std::fs::read(old.join("venv/bin/python")).unwrap(),
                b"initial generation"
            );
            assert_original(&root);
            // The dead publisher's lock is released; a new attempt gets a
            // third, distinct prefix, never the orphan's old directory.
            let next = Generation::begin(&root).unwrap();
            assert_ne!(next.prefix(), candidate);
            assert_ne!(next.prefix(), old);
            drop(next);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn orphan_worker_can_only_write_its_abandoned_prefix_after_restart() {
        let root = root();
        let initial = Generation::begin(&root).unwrap();
        populate(initial.prefix(), b"working engine");
        let active = initial.prefix().to_owned();
        initial.commit("marker.json").unwrap();
        let ready = root.join("fixture-ready");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "engine_generation::tests::killed_publication_child_fixture",
                "--nocapture",
            ])
            .env("PWR_GENERATION_CRASH_FIXTURE", &root)
            .env("PWR_GENERATION_LATE_WRITER", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
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
        assert!(reached);
        let candidate = root
            .join(GENERATIONS)
            .join(std::fs::read_to_string(root.join("fixture-generation")).unwrap());
        let next = Generation::begin(&root).unwrap();
        assert_ne!(next.prefix(), candidate);
        std::fs::write(root.join("fixture-release"), b"go").unwrap();
        let done = root.join("fixture-worker-done");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !done.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            done.exists(),
            "orphan worker did not write after parent death"
        );
        assert_eq!(
            std::fs::read(candidate.join("venv/bin/python")).unwrap(),
            b"late-worker"
        );
        assert_eq!(selected(&root).unwrap(), Some(active.clone()));
        assert_eq!(
            std::fs::read(active.join("venv/bin/python")).unwrap(),
            b"working engine"
        );
        assert_original(&root);
        drop(next);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn selected_generation_must_not_be_a_symlink() {
        let root = root();
        let outside = root.with_extension("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::create_dir(root.join(GENERATIONS)).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        std::os::unix::fs::symlink(&outside, root.join(GENERATIONS).join(&id)).unwrap();
        std::fs::write(
            root.join(SELECTION),
            serde_json::to_vec(&Selection {
                version: 1,
                generation: id,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(selected(&root).is_err());
        assert!(Generation::begin(&root).is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir(outside).unwrap();
    }

    #[test]
    #[ignore = "requires an explicitly selected local Python; run separately without model inference"]
    fn real_python_venv_keeps_its_absolute_prefix_after_publication() {
        let python =
            std::env::var_os("PWR_TEST_ENGINE_PYTHON").expect("select a local Python explicitly");
        let root = root();
        std::fs::create_dir(root.join(".install-in-progress")).unwrap();
        let backup = root.join("previous-engine-interrupted");
        std::fs::create_dir(&backup).unwrap();
        std::fs::write(backup.join("original"), b"legacy recovery copy").unwrap();
        assert!(selected(&root).is_err());
        let generation = Generation::begin(&root).unwrap();
        let prefix = generation.prefix().to_owned();
        let created = std::process::Command::new(python)
            .args(["-m", "venv", "--without-pip"])
            .arg(prefix.join("venv"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .unwrap();
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        std::fs::write(prefix.join("marker.json"), b"verified fixture").unwrap();
        std::fs::write(
            prefix.join("pwr-engine.json"),
            serde_json::to_vec(&crate::engine::current_marker()).unwrap(),
        )
        .unwrap();
        generation.commit("marker.json").unwrap();
        assert_eq!(selected(&root).unwrap(), Some(prefix.clone()));
        // A subsequent abandoned attempt must not relocate the first venv.
        let next = Generation::begin(&root).unwrap();
        let abandoned = next.prefix().to_owned();
        std::fs::write(abandoned.join("partial"), b"incomplete install").unwrap();
        drop(next);
        #[cfg(target_os = "macos")]
        {
            let plan = crate::engine_maintenance::plan(&root).unwrap();
            assert_eq!(crate::engine_maintenance::apply(&plan).unwrap().len(), 1);
            assert!(!abandoned.exists());
        }
        let output = std::process::Command::new(prefix.join("venv/bin/python"))
            .args(["-c", "import sys; print(sys.prefix)"])
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert_eq!(
            actual.canonicalize().unwrap(),
            prefix.join("venv").canonicalize().unwrap()
        );
        assert_original(&root);
        assert!(root.join(".install-in-progress").is_dir());
        assert_eq!(
            std::fs::read(backup.join("original")).unwrap(),
            b"legacy recovery copy"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
