//! Preserve the previous managed environment across ordinary setup failures.
use std::{
    io,
    path::{Path, PathBuf},
};

pub(super) struct Transaction {
    root: PathBuf,
    backup: PathBuf,
    pending: Vec<(&'static str, bool)>,
    done: bool,
}
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
impl Transaction {
    pub(super) fn begin(root: &Path, marker: &'static str) -> io::Result<Self> {
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
}
