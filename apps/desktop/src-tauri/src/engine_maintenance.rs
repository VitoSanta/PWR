//! Explicit offline maintenance. No UI startup, interpreter execution or model loading.
use crate::engine_generation::{lease, selected};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    version: u32,
    root: PathBuf,
    identity: (u64, u64),
    selection: Option<PathBuf>,
    remove: Vec<Entry>,
    retained: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    fingerprint: String,
    bytes: u64,
}

fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

fn identity(path: &Path) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(invalid("Expected an ordinary directory, never a symlink"));
    }
    Ok((meta.dev(), meta.ino()))
}

fn regular_marker(path: &Path) -> bool {
    use std::io::Read;
    let result = (|| -> io::Result<bool> {
        use std::os::unix::fs::OpenOptionsExt;
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != 2 {
            return Ok(false);
        }
        let mut bytes = Vec::new();
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)?
            .take(3)
            .read_to_end(&mut bytes)?;
        Ok(bytes == b"1\n")
    })();
    result.unwrap_or(false)
}

// Do not follow links while inventorying or deleting an unused environment.
fn fingerprint(path: &Path) -> io::Result<(String, u64)> {
    use std::os::unix::fs::MetadataExt;
    fn walk(path: &Path, hash: &mut Sha256, bytes: &mut u64) -> io::Result<()> {
        use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
        let meta = fs::symlink_metadata(path)?;
        hash.update(path.file_name().unwrap_or_default().as_bytes());
        hash.update(format!(
            "{:?}",
            (
                meta.dev(),
                meta.ino(),
                meta.mode(),
                meta.len(),
                meta.mtime(),
                meta.mtime_nsec(),
                meta.ctime(),
                meta.ctime_nsec()
            )
        ));
        if meta.file_type().is_symlink() {
            hash.update(fs::read_link(path)?.as_os_str().as_bytes());
        } else if meta.is_dir() {
            let mut children = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                walk(&child.path(), hash, bytes)?;
            }
        } else if meta.is_file() {
            *bytes = bytes.saturating_add(meta.len());
        } else {
            return Err(invalid("Special files require manual inspection"));
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let _ = fs::symlink_metadata(path)?.ino();
    walk(path, &mut hash, &mut bytes)?;
    Ok((format!("{:x}", hash.finalize()), bytes))
}

fn references(path: &Path, candidate: &Path) -> io::Result<bool> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if meta.file_type().is_symlink() {
        // A broken link cannot establish independence: retain conservatively.
        return Ok(fs::canonicalize(path).map_or(true, |target| target.starts_with(candidate)));
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            if references(&entry?.path(), candidate)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn inventory(root: &Path) -> io::Result<Plan> {
    let root = fs::canonicalize(root)?;
    let identity = identity(&root)?;
    let selection = selected(&root)?;
    if selection
        .as_ref()
        .is_some_and(|prefix| !crate::engine::maintenance_ready(prefix))
    {
        return Err(invalid("Selected engine is incomplete or has incompatible verification metadata; repair it before removing previous environments"));
    }
    let parent = root.join("generations");
    let mut remove = Vec::new();
    let mut retained = Vec::new();
    for entry in fs::read_dir(&root)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with("previous-engine-")
            || name == ".install-in-progress"
            || matches!(name.as_str(), "venv" | "python" | "cache")
        {
            retained.push(format!("{name}: legacy recovery material, never removed"));
        }
    }
    match fs::symlink_metadata(&parent) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
        Ok(_) => {
            self::identity(&parent)?;
            for child in fs::read_dir(parent)? {
                let child = child?;
                let path = child.path();
                let id = child.file_name().to_string_lossy().into_owned();
                let canonical_id =
                    uuid::Uuid::parse_str(&id).is_ok_and(|value| value.to_string() == id);
                let ordinary = self::identity(&path).is_ok();
                if selection.as_ref() == Some(&path) {
                    retained.push(format!("{id}: selected"));
                    continue;
                }
                let empty = ordinary && fs::read_dir(&path)?.next().is_none();
                if !canonical_id
                    || !ordinary
                    || (!regular_marker(&path.join(".pwr-generation")) && !empty)
                {
                    retained.push(format!("{id}: unknown, linked or unmarked"));
                    continue;
                }
                if references(&root.join("venv"), &path)?
                    || selection
                        .as_ref()
                        .is_some_and(|live| references(live, &path).unwrap_or(true))
                {
                    retained.push(format!("{id}: referenced by a retained environment"));
                    continue;
                }
                let (fingerprint, bytes) = fingerprint(&path)?;
                remove.push(Entry {
                    id,
                    fingerprint,
                    bytes,
                });
            }
        }
    }
    remove.sort_by(|a, b| a.id.cmp(&b.id));
    retained.sort();
    Ok(Plan {
        version: 1,
        root,
        identity,
        selection,
        remove,
        retained,
    })
}

fn unused_by_external_processes(root: &Path) -> io::Result<()> {
    // lsof's snapshot may catch an unrelated thread's child between fork and
    // exec, before CLOEXEC closes its copy of our own directory lock. Recheck
    // for a bounded interval; a process still using the root remains a refusal.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
    loop {
        match external_process_check(root) {
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => return result,
        }
    }
}

fn external_process_check(root: &Path) -> io::Result<()> {
    let output = std::process::Command::new("/usr/sbin/lsof")
        .args(["-nP", "-t", "+D"])
        .arg(root)
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.stderr.is_empty() || !matches!(output.status.code(), Some(0 | 1)) {
        return Err(invalid(
            "Cannot establish offline state with lsof; no files removed",
        ));
    }
    let pids = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    for line in pids.lines() {
        let pid = line.parse::<u32>().map_err(io::Error::other)?;
        if pid != std::process::id() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Engine files are open in another process; close it and make a new plan",
            ));
        }
    }
    Ok(())
}

pub(super) fn plan(root: &Path) -> io::Result<Plan> {
    identity(root)?;
    let _lock = lease(root, false)?;
    inventory(root)
}

pub(super) fn apply(expected: &Plan) -> io::Result<Vec<String>> {
    identity(&expected.root)?;
    let _lock = lease(&expected.root, false)?;
    if inventory(&expected.root)? != *expected {
        return Err(invalid(
            "Maintenance plan is stale or edited; create a new plan",
        ));
    }
    unused_by_external_processes(&expected.root)?;
    let mut removed = Vec::new();
    for entry in &expected.remove {
        if identity(&expected.root)? != expected.identity
            || selected(&expected.root)? != expected.selection
        {
            return Err(invalid(
                "Engine identity or selection changed; no further removal",
            ));
        }
        let path = expected.root.join("generations").join(&entry.id);
        if fingerprint(&path)?.0 != entry.fingerprint {
            return Err(invalid(
                "Environment changed after planning; create a new plan",
            ));
        }
        // Keep ownership until the last child is gone. A crash mid-delete leaves
        // a marked prefix that a fresh plan can resume; the final gap is empty.
        for child in fs::read_dir(&path)? {
            let child = child?;
            if child.file_name() == ".pwr-generation" {
                continue;
            }
            let meta = fs::symlink_metadata(child.path())?;
            if meta.is_dir() && !meta.file_type().is_symlink() {
                fs::remove_dir_all(child.path())?;
            } else {
                fs::remove_file(child.path())?;
            }
            #[cfg(test)]
            checkpoint();
        }
        match fs::remove_file(path.join(".pwr-generation")) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::remove_dir(&path)?;
        removed.push(entry.id.clone());
    }
    fs::File::open(&expected.root)?.sync_all()?;
    Ok(removed)
}

pub(super) fn cli(args: &[String]) -> io::Result<()> {
    match args {
        [mode, root] if mode == "plan" => println!(
            "{}",
            serde_json::to_string_pretty(&plan(Path::new(root))?).map_err(io::Error::other)?
        ),
        [mode, file] if mode == "apply" => {
            use std::io::Read;
            let mut bytes = Vec::new();
            fs::File::open(file)?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err(invalid("Plan exceeds 1 MiB"));
            }
            let expected: Plan = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            println!(
                "{}",
                serde_json::to_string(&apply(&expected)?).map_err(io::Error::other)?
            );
        }
        _ => {
            return Err(invalid(
                "Usage: --engine-maintenance plan ABSOLUTE_ENGINE_ROOT | apply PLAN_JSON",
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
fn checkpoint() {
    if let Some(path) = std::env::var_os("PWR_TEST_MAINTENANCE_PAUSE") {
        fs::write(path, b"paused").unwrap();
        loop {
            std::thread::park();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_generation::Generation;
    fn fixture() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("pwr-maintenance-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let old = Generation::begin(&root).unwrap();
        fs::write(old.prefix().join("one"), b"old bytes").unwrap();
        fs::write(old.prefix().join("two"), b"more bytes").unwrap();
        let abandoned = old.prefix().to_owned();
        drop(old);
        let active = Generation::begin(&root).unwrap();
        fs::create_dir_all(active.prefix().join("venv/bin")).unwrap();
        fs::write(active.prefix().join("venv/bin/python"), b"fixture only").unwrap();
        fs::write(active.prefix().join("marker.json"), b"verified fixture").unwrap();
        fs::write(
            active.prefix().join("pwr-engine.json"),
            serde_json::to_vec(&crate::engine::current_marker()).unwrap(),
        )
        .unwrap();
        let selected = active.prefix().to_owned();
        active.commit("marker.json").unwrap();
        (
            fs::canonicalize(root).unwrap(),
            fs::canonicalize(abandoned).unwrap(),
            fs::canonicalize(selected).unwrap(),
        )
    }
    #[test]
    fn explicit_plan_removes_only_unused_owned_prefixes_and_never_follows_links() {
        let (root, old, active) = fixture();
        let unknown = root
            .join("generations")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&unknown).unwrap();
        fs::write(unknown.join("keep"), b"unowned").unwrap();
        let legacy = root.join("previous-engine-old");
        fs::create_dir(&legacy).unwrap();
        fs::write(legacy.join("keep"), b"original backup").unwrap();
        let outside = root.with_extension("outside");
        fs::write(&outside, b"external bytes").unwrap();
        std::os::unix::fs::symlink(&outside, old.join("outside-link")).unwrap();
        let before = plan(&root).unwrap();
        assert_eq!(before.remove.len(), 1);
        assert!(old.is_dir());
        assert_eq!(apply(&before).unwrap().len(), 1);
        assert!(!old.exists());
        assert_eq!(fs::read(&outside).unwrap(), b"external bytes");
        assert!(active.join("venv/bin/python").is_file());
        assert!(unknown.join("keep").is_file());
        assert_eq!(fs::read(legacy.join("keep")).unwrap(), b"original backup");
        assert_eq!(selected(&root).unwrap(), Some(active));
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }
    #[test]
    fn edited_stale_and_symlinked_plans_are_refused_without_deletion() {
        let (root, old, active) = fixture();
        let mut edited = plan(&root).unwrap();
        edited.remove[0].id = active.file_name().unwrap().to_str().unwrap().into();
        assert!(apply(&edited).is_err());
        assert!(active.is_dir());
        let stale = plan(&root).unwrap();
        fs::write(old.join("later"), b"changed").unwrap();
        assert!(apply(&stale).is_err());
        assert!(old.join("one").is_file());
        let linked = root.with_extension("link");
        std::os::unix::fs::symlink(&root, &linked).unwrap();
        assert!(plan(&linked).is_err());
        fs::remove_file(linked).unwrap();
        let moved = root.with_extension("moved");
        fs::rename(&root, &moved).unwrap();
        fs::create_dir(&root).unwrap();
        assert!(apply(&stale).is_err());
        assert!(moved.join("generations").is_dir());
        fs::remove_dir_all(moved).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn an_empty_directory_left_after_the_last_cleanup_step_can_be_finished() {
        let (root, old, active) = fixture();
        for entry in fs::read_dir(&old).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let preview = plan(&root).unwrap();
        assert_eq!(preview.remove.len(), 1);
        apply(&preview).unwrap();
        assert!(!old.exists());
        assert!(active.is_dir());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn an_incomplete_or_corrupt_selected_engine_preserves_all_previous_environments() {
        let (root, old, active) = fixture();
        let before = plan(&root).unwrap();
        fs::write(active.join("pwr-engine.json"), b"corrupt").unwrap();
        assert!(plan(&root).is_err());
        assert!(apply(&before).is_err());
        assert!(old.join("one").is_file());
        fs::write(
            active.join("pwr-engine.json"),
            serde_json::to_vec(&crate::engine::current_marker()).unwrap(),
        )
        .unwrap();
        fs::remove_file(active.join("venv/bin/python")).unwrap();
        assert!(plan(&root).is_err());
        assert!(old.join("one").is_file());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retained_python_dependencies_and_unmarked_prefixes_are_not_cleanup_candidates() {
        let (root, old, active) = fixture();
        std::os::unix::fs::symlink(old.join("one"), active.join("dependency")).unwrap();
        assert!(plan(&root).unwrap().remove.is_empty());
        fs::remove_file(active.join("dependency")).unwrap();
        fs::remove_file(old.join(".pwr-generation")).unwrap();
        assert!(plan(&root).unwrap().remove.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn inherited_lease_blocks_maintenance_after_the_parent_releases_its_handle() {
        let (root, _, _) = fixture();
        for shared in [true, false] {
            let locked = lease(&root, shared).unwrap();
            let mut command = std::process::Command::new("/bin/sleep");
            command.arg("30");
            crate::engine_generation::inherit(&locked, &mut command);
            let mut child = command.spawn().unwrap();
            drop(locked);
            let result = plan(&root);
            child.kill().unwrap();
            child.wait().unwrap();
            assert!(result.is_err(), "descendant lost the use/installer lease");
            assert!(plan(&root).is_ok());
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn supervised_installer_retains_its_lease_until_the_step_has_stopped() {
        use std::os::unix::process::CommandExt;
        let (root, _, _) = fixture();
        let generation = Generation::begin(&root).unwrap();
        let mut step = std::process::Command::new("/bin/sleep");
        step.arg("30");
        let mut wrapper = crate::supervision::installer(step);
        wrapper
            .process_group(0)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        generation.inherit(&mut wrapper);
        let mut child = wrapper.spawn().unwrap();
        drop(generation);
        let blocked = plan(&root).is_err();
        // Closing the actual host lifetime pipe makes the wrapper watcher kill
        // the whole owned group, including children that inherited the lease.
        drop(child.stdin.take());
        child.wait().unwrap();
        assert!(blocked);
        assert!(plan(&root).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn an_external_process_using_the_engine_blocks_cleanup_even_without_a_pwr_lease() {
        let (root, old, _) = fixture();
        let before = plan(&root).unwrap();
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .current_dir(&old)
            .spawn()
            .unwrap();
        let result = apply(&before);
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(result.is_err());
        assert!(old.join("one").is_file());
        assert!(apply(&before).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_cleanup_child_fixture() {
        let Some(root) = std::env::var_os("PWR_TEST_MAINTENANCE_ROOT") else {
            return;
        };
        apply(&plan(Path::new(&root)).unwrap()).unwrap();
    }
    #[test]
    fn killed_cleanup_can_resume_from_a_new_plan_and_preserves_the_selection() {
        let (root, old, active) = fixture();
        let ready = root.with_extension("ready");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "engine_maintenance::tests::interrupted_cleanup_child_fixture",
                "--nocapture",
            ])
            .env("PWR_TEST_MAINTENANCE_ROOT", &root)
            .env("PWR_TEST_MAINTENANCE_PAUSE", &ready)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !ready.exists() && std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let paused = ready.exists();
        let _ = child.kill();
        child.wait().unwrap();
        assert!(paused, "child never reached partial deletion");
        assert!(old.join(".pwr-generation").is_file());
        assert!(active.join("venv/bin/python").is_file());
        assert_eq!(apply(&plan(&root).unwrap()).unwrap().len(), 1);
        assert!(!old.exists());
        assert_eq!(selected(&root).unwrap(), Some(active));
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(ready).unwrap();
    }
}
