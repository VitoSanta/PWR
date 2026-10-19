//! Private recovery copies; these commands never modify workspace files.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State};
const LIMIT: u64 = 16 * 1024 * 1024;
#[derive(Default)]
pub struct Drafts(pub Mutex<()>);
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    path: String,
    text: String,
    saved: String,
    hash: String,
    conflict: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    version: u32,
    workspace: String,
    active: String,
    documents: Vec<Document>,
}
fn validate(value: &Snapshot, workspace: &str) -> Result<(), String> {
    if value.version != 1 || value.workspace != workspace || value.documents.len() > 64 {
        return Err("Invalid editor recovery record; original retained.".into());
    }
    let mut paths = std::collections::HashSet::new();
    for doc in &value.documents {
        if doc.path.is_empty()
            || doc.path.contains('\\')
            || !Path::new(&doc.path)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            || !paths.insert(&doc.path)
            || doc.text == doc.saved
        {
            return Err("Invalid editor recovery document; original retained.".into());
        }
    }
    if !value.active.is_empty() && !paths.contains(&value.active) {
        return Err("Invalid editor recovery selection; original retained.".into());
    }
    Ok(())
}
fn location(app: &AppHandle, workspace: &str) -> Result<(PathBuf, String), String> {
    let cwd = crate::canonical_workspace(workspace)?;
    if !crate::is_workspace_trusted(app, &cwd)? {
        return Err("Trust the workspace before accessing its editor recovery.".into());
    }
    let canonical = cwd.to_string_lossy().into_owned();
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("editor-drafts");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    if fs::symlink_metadata(&root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Editor recovery directory is a symlink.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok((
        root.join(format!("{:x}.json", Sha256::digest(canonical.as_bytes()))),
        canonical,
    ))
}
fn read(path: &Path, workspace: &str) -> Result<Option<Snapshot>, String> {
    #[cfg(unix)]
    let file = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(std::io::Error::from);
    #[cfg(not(unix))]
    let file = File::open(path);
    let file = match file {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > LIMIT {
        return Err(
            "Editor recovery record is not a regular file or exceeds 16 MiB; original retained."
                .into(),
        );
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Editor recovery exceeds 16 MiB; original retained.".into());
    }
    let value: Snapshot = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Cannot read editor recovery; original retained: {e}"))?;
    validate(&value, workspace)?;
    Ok(Some(value))
}
fn write(path: &Path, workspace: &str, snapshot: Option<Snapshot>) -> Result<(), String> {
    // Preserve unreadable/corrupt records instead of silently replacing them.
    read(path, workspace)?;
    let Some(value) = snapshot else {
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
        return Ok(());
    };
    validate(&value, workspace)?;
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Editor drafts exceed 16 MiB; the previous recovery copy was retained.".into());
    }
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        #[cfg(unix)]
        File::open(path.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result.map_err(|e| e.to_string())
}
#[tauri::command]
pub fn drafts_read(
    app: AppHandle,
    state: State<'_, Drafts>,
    workspace: String,
) -> Result<Option<Snapshot>, String> {
    let _guard = state.0.lock().map_err(|_| "Editor recovery unavailable")?;
    let (path, canonical) = location(&app, &workspace)?;
    read(&path, &canonical)
}
#[tauri::command]
pub fn drafts_write(
    app: AppHandle,
    state: State<'_, Drafts>,
    workspace: String,
    mut snapshot: Option<Snapshot>,
) -> Result<(), String> {
    let _guard = state.0.lock().map_err(|_| "Editor recovery unavailable")?;
    let (path, canonical) = location(&app, &workspace)?;
    if let Some(value) = snapshot.as_mut() {
        if value.workspace != workspace {
            return Err("Editor recovery workspace mismatch".into());
        }
        value.workspace = canonical.clone();
    }
    write(&path, &canonical, snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Snapshot {
        Snapshot {
            version: 1,
            workspace: "/project".into(),
            active: "a.ts".into(),
            documents: vec![Document {
                path: "a.ts".into(),
                text: "draft".into(),
                saved: "base".into(),
                hash: "v1".into(),
                conflict: false,
            }],
        }
    }
    #[test]
    fn durable_roundtrip_and_explicit_clear() {
        let root = std::env::temp_dir().join(format!("pwr-drafts-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("draft.json");
        write(&path, "/project", Some(sample())).unwrap();
        assert_eq!(
            read(&path, "/project").unwrap().unwrap().documents[0].text,
            "draft"
        );
        assert!(read(&path, "/other").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        write(&path, "/project", None).unwrap();
        assert!(read(&path, "/project").unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corruption_and_unsafe_documents_are_preserved() {
        let root = std::env::temp_dir().join(format!("pwr-drafts-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("draft.json");
        fs::write(&path, b"corrupt").unwrap();
        assert!(write(&path, "/project", Some(sample())).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"corrupt");
        fs::remove_file(&path).unwrap();
        let mut value = sample();
        value.documents[0].path = "../escape".into();
        assert!(write(&path, "/project", Some(value)).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("outside"), &path).unwrap();
            assert!(write(&path, "/project", Some(sample())).is_err());
            assert!(!root.join("outside").exists());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
