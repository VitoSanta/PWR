//! Download storage anchored once, before network I/O.
#[cfg(unix)]
use std::sync::Arc;
use std::{
    fs::File,
    io,
    path::{Component, Path, PathBuf},
};
#[derive(Clone)]
pub(super) struct Root {
    path: PathBuf,
    #[cfg(unix)]
    directory: Arc<File>,
}
#[derive(Clone)]
pub(super) struct Location {
    pub path: PathBuf,
    pub part: PathBuf,
    root: Root,
    #[cfg(unix)]
    directory: Arc<File>,
}
fn regular(file: File) -> io::Result<File> {
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(io::ErrorKind::PermissionDenied.into())
    }
}
impl Root {
    pub fn new(path: &Path) -> io::Result<Self> {
        if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        std::fs::create_dir_all(path)?;
        let root = Self {
            path: path.to_owned(),
            #[cfg(unix)]
            directory: Arc::new(super::pin_partial_directory(&path.join("unused"))?),
        };
        #[cfg(unix)]
        rustix::fs::flock(
            &*root.directory,
            rustix::fs::FlockOperation::NonBlockingLockExclusive,
        )?;
        root.ensure_current()?;
        Ok(root)
    }
    pub fn ensure_current(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let current = std::fs::symlink_metadata(&self.path)?;
            let pinned = self.directory.metadata()?;
            if !current.is_dir() || (current.dev(), current.ino()) != (pinned.dev(), pinned.ino()) {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        Ok(())
    }
    pub fn location(&self, destination: &Path) -> io::Result<Location> {
        let relative = destination
            .strip_prefix(&self.path)
            .map_err(|_| io::ErrorKind::PermissionDenied)?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        #[cfg(unix)]
        let directory = {
            use rustix::fs::{Mode, OFlags, mkdirat, openat};
            let mut dir = self.directory.try_clone()?;
            for component in relative.parent().unwrap_or(Path::new("")).components() {
                let Component::Normal(name) = component else {
                    return Err(io::ErrorKind::PermissionDenied.into());
                };
                if let Err(error) = mkdirat(&dir, name, Mode::RWXU)
                    && error != rustix::io::Errno::EXIST
                {
                    return Err(error.into());
                }
                dir = openat(
                    &dir,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?
                .into();
            }
            Arc::new(dir)
        };
        #[cfg(not(unix))]
        std::fs::create_dir_all(destination.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        Ok(Location {
            path: destination.to_owned(),
            part: super::part_path(destination),
            root: self.clone(),
            #[cfg(unix)]
            directory,
        })
    }
    pub fn revision(&self, revision: &str) -> io::Result<()> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        self.ensure_current()?;
        let temporary = format!(
            ".pwr-revision.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        #[cfg(unix)]
        {
            use rustix::fs::{AtFlags, Mode, OFlags, openat, renameat, unlinkat};
            let mut file: File = openat(
                &*self.directory,
                &temporary,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )?
            .into();
            let result = (|| {
                file.write_all(revision.as_bytes())?;
                file.sync_all()?;
                renameat(
                    &*self.directory,
                    &temporary,
                    &*self.directory,
                    super::REVISION_FILE,
                )?;
                Ok(())
            })();
            let _ = unlinkat(&*self.directory, &temporary, AtFlags::empty());
            result
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(temporary);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            let result = (|| {
                file.write_all(revision.as_bytes())?;
                file.sync_all()?;
                std::fs::rename(&path, self.path.join(super::REVISION_FILE))
            })();
            let _ = std::fs::remove_file(path);
            result
        }
    }
}
impl Location {
    pub fn ensure_current(&self) -> io::Result<()> {
        self.root.ensure_current()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let current =
                std::fs::symlink_metadata(self.path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
            let pinned = self.directory.metadata()?;
            if !current.is_dir() || (current.dev(), current.ino()) != (pinned.dev(), pinned.ino()) {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
        }
        Ok(())
    }
    pub fn read(&self, partial: bool) -> io::Result<File> {
        let path = if partial { &self.part } else { &self.path };
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
            regular(
                openat(
                    &*self.directory,
                    name,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                    Mode::empty(),
                )?
                .into(),
            )
        }
        #[cfg(not(unix))]
        {
            regular(File::open(path)?)
        }
    }
    pub fn partial_len(&self) -> io::Result<u64> {
        match self.read(true) {
            Ok(f) => Ok(f.metadata()?.len()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e),
        }
    }
    pub fn writer(&self, append: bool, existing: u64) -> io::Result<File> {
        #[cfg(unix)]
        {
            super::open_partial_at(&self.directory, &self.part, append, existing)
        }
        #[cfg(not(unix))]
        {
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&self.part)
        }
    }
    pub fn remove_partial(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            use rustix::fs::{AtFlags, unlinkat};
            Ok(unlinkat(
                &*self.directory,
                self.part.file_name().ok_or(io::ErrorKind::InvalidInput)?,
                AtFlags::empty(),
            )?)
        }
        #[cfg(not(unix))]
        {
            std::fs::remove_file(&self.part)
        }
    }
    pub fn publish(&self) -> io::Result<()> {
        self.ensure_current()?;
        #[cfg(unix)]
        {
            use rustix::fs::{AtFlags, linkat};
            Ok(linkat(
                &*self.directory,
                self.part.file_name().ok_or(io::ErrorKind::InvalidInput)?,
                &*self.directory,
                self.path.file_name().ok_or(io::ErrorKind::InvalidInput)?,
                AtFlags::empty(),
            )?)
        }
        #[cfg(not(unix))]
        {
            std::fs::hard_link(&self.part, &self.path)
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn independent_downloads_cannot_write_the_same_root_concurrently() {
        let folder = tempfile::tempdir().unwrap();
        let first = Root::new(folder.path()).unwrap();
        assert!(Root::new(folder.path()).is_err());
        drop(first);
        assert!(Root::new(folder.path()).is_ok());
    }
}
