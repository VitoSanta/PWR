//! Reject ambiguous legacy setup state. Old rollback fixtures remain for regression history.
use std::{
    io,
    path::{Path, PathBuf},
};

#[cfg(test)]
pub(super) struct Transaction {
    root: PathBuf,
    backup: PathBuf,
    pending: Vec<(&'static str, bool)>,
    done: bool,
    _ownership: InstallOwnership,
}

// A directory is an atomic, portable exclusion claim. It deliberately survives
// process death: a new installer cannot know whether orphaned workers still
// write into this prefix, so it must preserve it for assisted recovery.

const INSTALL_CLAIM: &str = ".install-in-progress";

fn previous_backup(root: &Path) -> io::Result<Option<PathBuf>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("previous-engine-")
        {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

/// Even a current package marker cannot establish readiness while installation
/// ownership or an unresolved previous environment remains on disk.
pub(super) fn unsettled(root: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(root.join(INSTALL_CLAIM)) {
        Ok(_) => return Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(previous_backup(root)?.is_some())
}

#[cfg(test)]
struct InstallOwnership(PathBuf);
#[cfg(test)]
impl InstallOwnership {
    fn acquire(root: &Path) -> io::Result<Self> {
        let path = root.join(INSTALL_CLAIM);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                io::Error::new(io::ErrorKind::WouldBlock,
                    "An engine installation is active or was interrupted. Preserve the engine directory and its previous-engine-* backups. Confirm all installers have stopped before assisted recovery; retrying cannot safely restore it automatically.")
            } else {
                error
            }
        })?;
        Ok(Self(path))
    }
}
#[cfg(test)]
impl Drop for InstallOwnership {
    fn drop(&mut self) {
        // Never recursively delete unexpected contents or follow a link.
        if let Err(error) = std::fs::remove_dir(&self.0) {
            log::warn!(
                "Engine installation ownership retained at {}: {error}",
                self.0.display()
            );
        }
    }
}
#[cfg(test)]
fn remove_entry(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            std::fs::remove_dir_all(path)
        }
        Ok(_) => std::fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
#[cfg(test)]
impl Transaction {
    pub(super) fn begin(root: &Path, marker: &'static str) -> io::Result<Self> {
        let ownership = InstallOwnership::acquire(root)?;
        // Also recognize backups left by older builds, which had no claim.
        // A failed rollback or backup cleanup must never become the next
        // installation's purported original environment.
        if let Some(backup) = previous_backup(root)? {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, format!(
                "An engine backup requires assisted recovery at {}. No installation files were changed; preserve this backup and confirm all installers have stopped before recovery.", backup.display())));
        }
        let backup = root.join(format!("previous-engine-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&backup)?;
        let mut transaction = Self {
            root: root.to_owned(),
            backup,
            pending: Vec::new(),
            done: false,
            _ownership: ownership,
        };
        for name in ["venv", marker] {
            let existed = match std::fs::rename(root.join(name), transaction.backup.join(name)) {
                Ok(()) => true,
                Err(error) if error.kind() == io::ErrorKind::NotFound => false,
                Err(error) => {
                    return match transaction.rollback() {
                        Ok(()) => Err(error),
                        Err(restore) => Err(io::Error::other(format!("{error}; restoring previous engine failed: {restore}; backup retained at {}",transaction.backup.display()))),
                    };
                }
            };
            transaction.pending.push((name, existed));
        }
        Ok(transaction)
    }
    pub(super) fn rollback(&mut self) -> io::Result<()> {
        if self.done {
            return Ok(());
        }
        // Remove each new entry only until its original has been restored.
        // Retrying after an error must never delete an already restored entry.
        while let Some((name, existed)) = self.pending.last().copied() {
            // Never mistake a lost/moved original for an originally absent entry.
            if existed {
                std::fs::symlink_metadata(self.backup.join(name))?;
            }
            remove_entry(&self.root.join(name))?;
            if existed {
                std::fs::rename(self.backup.join(name), self.root.join(name))?;
            }
            self.pending.pop();
        }
        std::fs::remove_dir(&self.backup)?;
        self.done = true;
        Ok(())
    }
    pub(super) fn backup(&self) -> &Path {
        &self.backup
    }
    pub(super) fn commit(mut self) {
        self.done = true;
        if let Err(error) = std::fs::remove_dir_all(&self.backup) {
            log::warn!(
                "New engine installed; previous backup retained at {}: {error}",
                self.backup.display()
            );
        }
    }
}
#[cfg(test)]
impl Drop for Transaction {
    fn drop(&mut self) {
        if let Err(error) = self.rollback() {
            log::error!(
                "Previous engine backup retained at {} after restore error: {error}",
                self.backup.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("pwr-engine-transaction-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("venv/lib")).unwrap();
        std::fs::write(root.join("venv/lib/original"), b"original package bytes").unwrap();
        std::fs::write(root.join("marker.json"), b"original marker bytes").unwrap();
        root
    }
    #[test]
    fn failed_or_cancelled_setup_restores_exact_originals() {
        let root = root();
        let mut tx = Transaction::begin(&root, "marker.json").unwrap();
        std::fs::create_dir(root.join("venv")).unwrap();
        std::fs::write(root.join("venv/incomplete"), b"new bytes").unwrap();
        std::fs::write(root.join("marker.json"), b"incomplete marker").unwrap();
        tx.rollback().unwrap();
        tx.rollback().unwrap();
        assert_eq!(
            std::fs::read(root.join("venv/lib/original")).unwrap(),
            b"original package bytes"
        );
        assert_eq!(
            std::fs::read(root.join("marker.json")).unwrap(),
            b"original marker bytes"
        );
        assert!(!root.join("venv/incomplete").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn successful_setup_keeps_new_environment_and_removes_backup() {
        let root = root();
        let tx = Transaction::begin(&root, "marker.json").unwrap();
        let backup = tx.backup().to_owned();
        std::fs::create_dir(root.join("venv")).unwrap();
        std::fs::write(root.join("venv/new"), b"new bytes").unwrap();
        std::fs::write(root.join("marker.json"), b"new marker").unwrap();
        tx.commit();
        assert!(!backup.exists());
        assert_eq!(
            std::fs::read(root.join("marker.json")).unwrap(),
            b"new marker"
        );
        assert!(root.join("venv/new").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn dropped_setup_restores_previous_environment() {
        let root = root();
        {
            let _tx = Transaction::begin(&root, "marker.json").unwrap();
            std::fs::create_dir(root.join("venv")).unwrap();
        }
        assert!(root.join("venv/lib/original").exists());
        assert_eq!(
            std::fs::read(root.join("marker.json")).unwrap(),
            b"original marker bytes"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restoration_retry_does_not_remove_an_already_restored_marker() {
        let root = root();
        let mut tx = Transaction::begin(&root, "marker.json").unwrap();
        let hidden = root.join("held-original");
        std::fs::rename(tx.backup.join("venv"), &hidden).unwrap();
        std::fs::create_dir(root.join("venv")).unwrap();
        std::fs::write(root.join("venv/incomplete"), b"new").unwrap();
        assert!(tx.rollback().is_err());
        assert_eq!(
            std::fs::read(root.join("marker.json")).unwrap(),
            b"original marker bytes"
        );
        assert!(root.join("venv/incomplete").exists());
        std::fs::rename(hidden, tx.backup.join("venv")).unwrap();
        tx.rollback().unwrap();
        assert_eq!(
            std::fs::read(root.join("marker.json")).unwrap(),
            b"original marker bytes"
        );
        assert_eq!(
            std::fs::read(root.join("venv/lib/original")).unwrap(),
            b"original package bytes"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn another_setup_cannot_move_an_active_installation() {
        let root = root();
        let tx = Transaction::begin(&root, "marker.json").unwrap();
        std::fs::create_dir(root.join("venv")).unwrap();
        std::fs::write(root.join("venv/incomplete"), b"pending install").unwrap();
        let second = Transaction::begin(&root, "marker.json");
        assert!(
            second.is_err(),
            "second installer acquired an active prefix"
        );
        assert_eq!(
            std::fs::read(root.join("venv/incomplete")).unwrap(),
            b"pending install"
        );
        drop(tx);
        assert!(root.join("venv/lib/original").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_setup_child_fixture() {
        let Some(root) = std::env::var_os("PWR_ENGINE_CRASH_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(root);
        let _tx = Transaction::begin(&root, "marker.json").unwrap();
        std::fs::create_dir(root.join("venv")).unwrap();
        std::fs::write(root.join("venv/incomplete"), b"partial new engine").unwrap();
        std::fs::write(root.join("fixture-ready"), b"ready").unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    #[test]
    fn forced_termination_preserves_backup_and_blocks_retry_without_mutation() {
        let root = root();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "engine_install::tests::interrupted_setup_child_fixture",
                "--nocapture",
            ])
            .env("PWR_ENGINE_CRASH_FIXTURE", &root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !root.join("fixture-ready").exists() && std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ready = root.join("fixture-ready").exists();
        let active_blocked = ready && Transaction::begin(&root, "marker.json").is_err();
        let _ = child.kill();
        child.wait().unwrap();
        assert!(ready, "child did not enter the partial installation phase");
        assert!(
            active_blocked,
            "another process acquired the active installation"
        );
        assert!(root.join(INSTALL_CLAIM).is_dir());
        let backups: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("previous-engine-")
            })
            .map(|entry| entry.path())
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(Transaction::begin(&root, "marker.json").is_err());
        assert_eq!(
            std::fs::read(backups[0].join("venv/lib/original")).unwrap(),
            b"original package bytes"
        );
        assert_eq!(
            std::fs::read(backups[0].join("marker.json")).unwrap(),
            b"original marker bytes"
        );
        assert_eq!(
            std::fs::read(root.join("venv/incomplete")).unwrap(),
            b"partial new engine"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_backup_blocks_setup_and_retains_current_files() {
        let root = root();
        let backup = root.join("previous-engine-legacy");
        std::fs::create_dir(&backup).unwrap();
        std::fs::write(backup.join("original"), b"retained").unwrap();
        assert!(Transaction::begin(&root, "marker.json").is_err());
        assert!(root.join("venv/lib/original").exists());
        assert_eq!(std::fs::read(backup.join("original")).unwrap(), b"retained");
        assert!(!root.join(INSTALL_CLAIM).exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn preexisting_claim_symlink_is_never_followed_or_removed() {
        let root = root();
        let outside = root.with_extension("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("keep"), b"untouched").unwrap();
        std::os::unix::fs::symlink(&outside, root.join(INSTALL_CLAIM)).unwrap();
        assert!(Transaction::begin(&root, "marker.json").is_err());
        assert!(root
            .join(INSTALL_CLAIM)
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"untouched");
        assert!(root.join("venv/lib/original").exists());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn readiness_rejects_unsettled_installations_without_mutating_them() {
        let root = root();
        assert!(!unsettled(&root).unwrap());
        {
            let _tx = Transaction::begin(&root, "marker.json").unwrap();
            std::fs::create_dir(root.join("venv")).unwrap();
            std::fs::write(root.join("marker.json"), b"apparently current").unwrap();
            assert!(unsettled(&root).unwrap());
            assert_eq!(
                std::fs::read(root.join("marker.json")).unwrap(),
                b"apparently current"
            );
        }
        assert!(!unsettled(&root).unwrap());
        std::fs::create_dir(root.join("previous-engine-legacy")).unwrap();
        assert!(unsettled(&root).unwrap());
        std::fs::remove_dir_all(&root).unwrap();
        assert!(!unsettled(&root).unwrap());
    }
}
