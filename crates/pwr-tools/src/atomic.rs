//! Atomic, checked writes to workspace files.
//!
//! Every file-writing tool used `std::fs::write` after reading the file and
//! checking its hash. That leaves two holes the technical review of 2026-09-30
//! named and this module closes as far as a file system allows:
//!
//! - a crash or error part-way through leaves a truncated file, and the model's
//!   whole-file rewrite of a good one is gone; and
//! - a change made between the hash check and the write -- a person saving in
//!   their editor, a build -- is overwritten without either side knowing.
//!
//! The new bytes are written to a temporary file beside the target, flushed,
//! given the target's permissions, and moved over it with `rename`, which
//! replaces a file whole or not at all. Immediately before the move the target
//! is read once more and compared with what the caller checked.
//!
//! What this does not do. The comparison and the `rename` are two system calls,
//! so a change landing between them is still lost; that window is
//! microseconds, not the seconds between a tool's read and its write, and it is
//! stated in SECURITY.md rather than hidden. A file created new is different:
//! it is linked into place, which fails if the name is taken, so two writers
//! cannot both believe they created it. Nothing here promises rollback of an
//! edit that completed.

use crate::ToolError;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The prefix of a file that is being written and is not yet the file.
///
/// Excluded from the repository index and from what the tools list, so a
/// crash that leaves one behind does not put it in front of the model.
pub const TEMPORARY_PREFIX: &str = pwr_repo::TEMPORARY_WRITE_PREFIX;

/// What the target must hold when the new bytes are moved over it.
#[derive(Debug, Clone, Copy)]
pub enum Expect<'a> {
    /// Whatever it holds, or nothing: a restore or a rewind, which decided
    /// beforehand that the file is theirs to put back.
    Anything,
    /// No file at all: the name is linked into place and the write fails if
    /// another writer took it first.
    Absent,
    /// A file whose content hashes to this, as the caller read and checked it.
    Hash(&'a str),
}

/// A point inside a write, for tests that make it fail or that change the
/// target underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The temporary file is complete and flushed.
    TemporaryWritten,
    /// Everything is checked and only the move remains.
    BeforeMove,
}

/// Writes `bytes` to `target`, whole or not at all. See the module.
pub fn write_atomic(target: &Path, bytes: &[u8], expect: Expect<'_>) -> Result<(), ToolError> {
    write_atomic_at(target, bytes, expect, &mut |_| Ok(()))
}

/// [`write_atomic`] with a hook called at each [`Stage`]. An error from the
/// hook stops the write there, as a failing disk would.
pub fn write_atomic_at(
    target: &Path,
    bytes: &[u8],
    expect: Expect<'_>,
    hook: &mut dyn FnMut(Stage) -> std::io::Result<()>,
) -> Result<(), ToolError> {
    let directory = target
        .parent()
        .ok_or_else(|| ToolError::Denied(format!("{} has no parent folder", target.display())))?;
    // `fs::write` refused a read-only file; `rename` would replace it, because
    // permission to replace a name belongs to the folder. The refusal stays.
    if let Ok(existing) = std::fs::metadata(target)
        && existing.permissions().readonly()
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is read-only", target.display()),
        )
        .into());
    }
    let temporary = temporary_beside(target);
    let outcome = write_through_temporary(&temporary, target, directory, bytes, expect, hook);
    // Whatever happened, the temporary name must not stay behind: after a
    // successful move it is already gone, and `remove_file` says so harmlessly.
    let _ = std::fs::remove_file(&temporary);
    outcome
}

fn write_through_temporary(
    temporary: &Path,
    target: &Path,
    directory: &Path,
    bytes: &[u8],
    expect: Expect<'_>,
    hook: &mut dyn FnMut(Stage) -> std::io::Result<()>,
) -> Result<(), ToolError> {
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary)?;
        file.write_all(bytes)?;
        // The target's permissions, so an executable script stays one.
        if let Ok(existing) = std::fs::metadata(target) {
            file.set_permissions(existing.permissions())?;
        }
        file.sync_all()?;
    }
    hook(Stage::TemporaryWritten)?;

    match expect {
        Expect::Anything => {}
        Expect::Absent => {}
        Expect::Hash(expected) => {
            let now = match std::fs::read(target) {
                Ok(current) => Some(pwr_domain::hash_bytes(&current)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            if now.as_deref() != Some(expected) {
                return Err(changed_underneath(target));
            }
        }
    }
    hook(Stage::BeforeMove)?;

    match expect {
        Expect::Absent => match std::fs::hard_link(temporary, target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(changed_underneath(target));
            }
            Err(error) => return Err(error.into()),
        },
        _ => std::fs::rename(temporary, target)?,
    }
    // The move is only durable once the folder that holds it is.
    if let Ok(folder) = std::fs::File::open(directory) {
        let _ = folder.sync_all();
    }
    Ok(())
}

fn changed_underneath(target: &Path) -> ToolError {
    ToolError::Denied(format!(
        "{} changed while this edit was being applied, so nothing was written. Read it again \
         and repeat the edit against what it holds now.",
        target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    ))
}

/// A name for the temporary file that no other writer is using: the process,
/// a counter, and the target's own name so a stray one says what it was for.
fn temporary_beside(target: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(format!(
        "{TEMPORARY_PREFIX}{}-{unique}-{name}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(bytes: &[u8]) -> String {
        pwr_domain::hash_bytes(bytes)
    }

    fn folder_holds_only(dir: &Path, names: &[&str]) {
        let mut found: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        assert_eq!(found, names, "a temporary file was left behind");
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_leaves_nothing_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.txt");
        std::fs::write(&target, "old").unwrap();
        write_atomic(&target, b"new", Expect::Hash(&hash(b"old"))).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        folder_holds_only(dir.path(), &["a.txt"]);
    }

    /// The defect: a failure part-way left a truncated or empty file.
    #[test]
    fn a_failure_before_the_move_leaves_the_original_intact() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.txt");
        std::fs::write(&target, "the original, complete").unwrap();
        for stage in [Stage::TemporaryWritten, Stage::BeforeMove] {
            let failure =
                write_atomic_at(&target, b"half of a new fi", Expect::Anything, &mut |at| {
                    if at == stage {
                        Err(std::io::Error::other("the disk failed"))
                    } else {
                        Ok(())
                    }
                });
            assert!(failure.is_err(), "{stage:?}");
            assert_eq!(
                std::fs::read_to_string(&target).unwrap(),
                "the original, complete",
                "{stage:?}"
            );
            folder_holds_only(dir.path(), &["a.txt"]);
        }
    }

    /// The defect: a change between the check and the write was overwritten.
    #[test]
    fn a_change_made_after_the_check_is_refused_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.txt");
        std::fs::write(&target, "as read").unwrap();
        let checked = hash(b"as read");
        let result = write_atomic_at(&target, b"the model's", Expect::Hash(&checked), &mut |at| {
            if at == Stage::TemporaryWritten {
                // The person saves in their editor.
                std::fs::write(dir.path().join("a.txt"), "as edited").unwrap();
            }
            Ok(())
        });
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("changed while this edit was being applied"),
            "{message}"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "as edited");
        folder_holds_only(dir.path(), &["a.txt"]);
    }

    #[test]
    fn a_file_deleted_after_the_check_is_not_recreated_by_a_checked_write() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.txt");
        std::fs::write(&target, "as read").unwrap();
        let checked = hash(b"as read");
        let result = write_atomic_at(&target, b"new", Expect::Hash(&checked), &mut |at| {
            if at == Stage::TemporaryWritten {
                std::fs::remove_file(dir.path().join("a.txt")).unwrap();
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(!target.exists());
    }

    #[test]
    fn a_read_only_file_is_still_refused() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("locked.txt");
        std::fs::write(&target, "locked").unwrap();
        let mut permissions = std::fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).unwrap();
        assert!(write_atomic(&target, b"new", Expect::Anything).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "locked");
        folder_holds_only(dir.path(), &["locked.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn an_executable_stays_executable() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("run.sh");
        std::fs::write(&target, "#!/bin/sh\necho one\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        write_atomic(&target, b"#!/bin/sh\necho two\n", Expect::Anything).unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "#!/bin/sh\necho two\n"
        );
    }

    /// `resolve` hands the tools the real path of a link's target, so a write
    /// lands in the file the link points to and the link stays a link.
    #[cfg(unix)]
    #[test]
    fn a_link_resolved_to_its_target_stays_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        let link = dir.path().join("link.txt");
        std::fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let resolved = link.canonicalize().unwrap();
        write_atomic(&resolved, b"new", Expect::Hash(&hash(b"old"))).unwrap();
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "new");
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn a_new_file_is_created_and_a_taken_name_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("new.txt");
        write_atomic(&target, b"first", Expect::Absent).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "first");
        folder_holds_only(dir.path(), &["new.txt"]);

        // Another writer got there first, between the caller's check and this.
        let target = dir.path().join("race.txt");
        let result = write_atomic_at(&target, b"mine", Expect::Absent, &mut |at| {
            if at == Stage::TemporaryWritten {
                std::fs::write(dir.path().join("race.txt"), "theirs").unwrap();
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "theirs");
        folder_holds_only(dir.path(), &["new.txt", "race.txt"]);
    }
}
