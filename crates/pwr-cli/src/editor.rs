//! Human editor operations: checked writes, confined paths, recoverable removal.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn target(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.split(['/', '\\']).any(|part| matches!(part, ".pwr" | ".git" | ".pwr-scratch")) {
        return Err("Choose a project path outside PWR and Git internal folders.".into());
    }
    let path=crate::serve::rewind_target(root, relative).map_err(|why| why.to_string())?;
    let canonical_root=root.canonicalize().map_err(|why|why.to_string())?;
    let mut existing=path.as_path();
    while !existing.exists() { existing=existing.parent().ok_or("Invalid path")?; }
    let resolved=existing.canonicalize().map_err(|why|why.to_string())?;
    if resolved.strip_prefix(&canonical_root).map_err(|_|"Path outside project")?.components().any(|part|matches!(part.as_os_str().to_str(),Some(".pwr"|".git"|".pwr-scratch"))) {
        return Err("PWR and Git internal folders are not editable.".into());
    }
    Ok(path)
}

pub fn request(params: &Value) -> Result<Value, String> {
    let root = PathBuf::from(params["cwd"].as_str().ok_or("cwd is required")?);
    if params["operation"] == "search" {
        let query=params["query"].as_str().ok_or("query is required")?;
        if query.trim().is_empty() || query.len()>500 { return Err("Enter a search query up to 500 characters.".into()); }
        let policy=pwr_tools::PolicyProfile::Safe.build(root);
        let result=pwr_tools::search_query(&policy,&pwr_tools::SearchQuery::literal(query,100)).map_err(|why|why.to_string())?;
        return serde_json::to_value(result).map_err(|why|why.to_string());
    }
    let path = params["path"].as_str().ok_or("path is required")?;
    let file = target(&root, path)?;
    let op = params["operation"].as_str().ok_or("operation is required")?;
    match op {
        "save" | "create_file" => {
            let text = params["text"].as_str().ok_or("text is required")?;
            if text.len() > 4 * 1024 * 1024 { return Err("The editor saves files up to 4 MB.".into()); }
            let expect = if op == "create_file" { pwr_tools::atomic::Expect::Absent } else {
                pwr_tools::atomic::Expect::Hash(params["hash"].as_str().ok_or("The version read from disk is required before saving.")?)
            };
            pwr_tools::atomic::write_atomic(&file, text.as_bytes(), expect).map_err(|why| why.to_string())?;
            Ok(json!({"path":path,"hash":pwr_domain::hash_bytes(text.as_bytes()),"text":text}))
        }
        "create_directory" => {
            std::fs::create_dir(&file).map_err(|why| why.to_string())?;
            Ok(json!({"path":path}))
        }
        "rename" => {
            let to = params["to"].as_str().ok_or("to is required")?;
            let destination = target(&root, to)?;
            if destination.starts_with(&file) || std::fs::symlink_metadata(&destination).is_ok() {
                return Err("Choose a new name that does not exist and is outside the source folder.".into());
            }
            // A rename must never replace a file modified since the dialog opened.
            if file.is_file() {
                let hash = params["hash"].as_str().ok_or("The current file version is required.")?;
                let bytes = std::fs::read(&file).map_err(|why| why.to_string())?;
                if pwr_domain::hash_bytes(&bytes) != hash { return Err("The file changed on disk. Reload before renaming.".into()); }
            }
            std::fs::rename(file, destination).map_err(|why| why.to_string())?;
            Ok(json!({"path":to}))
        }
        "trash" => {
            if !file.exists() { return Err("The path no longer exists.".into()); }
            let trash = root.join(".pwr/editor-trash");
            if root.join(".pwr").exists() && !root.join(".pwr").canonicalize().map_err(|why|why.to_string())?.starts_with(root.canonicalize().map_err(|why|why.to_string())?) { return Err("Invalid project state folder.".into()); }
            std::fs::create_dir_all(&trash).map_err(|why| why.to_string())?;
            if !trash.canonicalize().map_err(|why|why.to_string())?.starts_with(root.canonicalize().map_err(|why|why.to_string())?) {return Err("Invalid project trash folder.".into());}
            // Keep at most the latest 20 removed items per workspace.
            if let Ok(entries) = std::fs::read_dir(&trash) {
                let mut slots: Vec<_> = entries
                    .flatten()
                    .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                    .collect();
                if slots.len() >= 20 {
                    slots.sort_by(|a, b| a.0.cmp(&b.0));
                    let remove = slots.len() - 19;
                    for (_, old) in slots.into_iter().take(remove) {
                        let _ = std::fs::remove_dir_all(old);
                    }
                }
            }
            let token = pwr_domain::new_id().to_string();
            let slot = trash.join(&token);
            std::fs::create_dir(&slot).map_err(|why| why.to_string())?;
            std::fs::write(slot.join("path.json"), serde_json::to_vec(&path).unwrap()).map_err(|why| why.to_string())?;
            std::fs::rename(file, slot.join("content")).map_err(|why| why.to_string())?;
            Ok(json!({"path":path,"restoreToken":token}))
        }
        "restore" => {
            let token = params["restoreToken"].as_str().ok_or("restoreToken is required")?;
            if token.is_empty() || !token.chars().all(|c| c.is_ascii_hexdigit() || c == '-') { return Err("Invalid restore token.".into()); }
            let slot = root.join(".pwr/editor-trash").join(token);
            let original: String = serde_json::from_slice(&std::fs::read(slot.join("path.json")).map_err(|why| why.to_string())?).map_err(|why| why.to_string())?;
            let destination = target(&root, &original)?;
            if std::fs::symlink_metadata(&destination).is_ok() { return Err("The original name is occupied. Rename it before restoring.".into()); }
            std::fs::rename(slot.join("content"), destination).map_err(|why| why.to_string())?;
            Ok(json!({"path":original}))
        }
        _ => Err("Unknown editor operation.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saves_are_version_checked_and_creates_never_overwrite() {
        let root=tempfile::tempdir().unwrap();
        let args=json!({"cwd":root.path(),"path":"a.txt","operation":"create_file","text":"first"});
        let created=request(&args).unwrap();
        assert!(request(&args).is_err());
        let saved=request(&json!({"cwd":root.path(),"path":"a.txt","operation":"save","text":"second","hash":created["hash"]})).unwrap();
        assert!(request(&json!({"cwd":root.path(),"path":"a.txt","operation":"save","text":"stale","hash":created["hash"]})).is_err());
        assert_eq!(std::fs::read_to_string(root.path().join("a.txt")).unwrap(),"second");
        assert_ne!(created["hash"],saved["hash"]);
    }
    #[test]
    fn removal_is_recoverable_and_restore_does_not_overwrite() {
        let root=tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"),"keep me").unwrap();
        let removed=request(&json!({"cwd":root.path(),"path":"a.txt","operation":"trash"})).unwrap();
        assert!(!root.path().join("a.txt").exists());
        let restore=json!({"cwd":root.path(),"path":"a.txt","operation":"restore","restoreToken":removed["restoreToken"]});
        std::fs::write(root.path().join("a.txt"),"new file").unwrap();
        assert!(request(&restore).is_err());
        std::fs::remove_file(root.path().join("a.txt")).unwrap();
        request(&restore).unwrap();
        assert_eq!(std::fs::read_to_string(root.path().join("a.txt")).unwrap(),"keep me");
    }
    #[test]
    fn creation_and_rename_stay_inside_the_project() {
        let root=tempfile::tempdir().unwrap();
        for path in ["../escape","/tmp/escape",".git/config",".pwr/chat-config.json",""] {
            assert!(request(&json!({"cwd":root.path(),"path":path,"operation":"create_file","text":"no"})).is_err(),"{path}");
        }
        #[cfg(unix)] {
            let outside=tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(),root.path().join("link")).unwrap();
            assert!(request(&json!({"cwd":root.path(),"path":"link/escape","operation":"create_file","text":"no"})).is_err());
            std::fs::create_dir(root.path().join(".pwr")).unwrap();
            std::os::unix::fs::symlink(root.path().join(".pwr"),root.path().join("internal")).unwrap();
            assert!(request(&json!({"cwd":root.path(),"path":"internal/escape","operation":"create_file","text":"no"})).is_err());
        }
    }
}
