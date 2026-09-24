//! Models already on this machine: listed from the engines' folders, and
//! deleted from them on request.
//!
//! Deletion is the one operation here that destroys data, so it is narrow: a
//! model is named by the reference the engine knows it by, resolved inside
//! the models folder, and refused unless it is exactly one model -- an MLX
//! directory holding a `config.json`, or a GGUF file with the shards of the
//! same split -- that stays inside that folder after every symlink is
//! resolved. Nothing else in the folder is touched; empty parent folders the
//! deletion leaves behind are removed up to, never including, the root.

use crate::catalog::Format;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// A model on disk, as the Model Manager lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    /// The reference the engine knows it by, relative to its models folder.
    pub model_ref: String,
    pub format: Format,
    pub path: String,
    pub bytes: u64,
    pub files: usize,
    /// An unfinished download: `.part` files and no complete model.
    pub partial: bool,
}

/// What a deletion removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    pub model_ref: String,
    pub freed_bytes: u64,
    pub removed: Vec<String>,
}

/// Every model of `format` under `root`: MLX folders at `<owner>/<name>`
/// (with a `config.json`, or holding only `.part` files from a download that
/// did not finish), and GGUF files anywhere below, one entry per split.
pub fn list(root: &Path, format: Format) -> Vec<LocalModel> {
    let mut found = match format {
        Format::Mlx => list_mlx(root),
        Format::Gguf => list_gguf(root),
    };
    found.sort_by(|a, b| a.model_ref.cmp(&b.model_ref));
    found
}

fn list_mlx(root: &Path) -> Vec<LocalModel> {
    let mut found = Vec::new();
    for owner in read_dirs(root) {
        for dir in read_dirs(&owner) {
            let files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect();
            let has_config = dir.join("config.json").is_file();
            let has_weights = files.iter().any(|path| extension(path) == "safetensors");
            let has_parts = files.iter().any(|path| extension(path) == "part");
            if !(has_config && has_weights) && !has_parts {
                continue;
            }
            // A GGUF folder of the same shape is not an MLX model.
            if files.iter().any(|path| extension(path) == "gguf") && !has_config {
                continue;
            }
            let Some(model_ref) = relative_ref(root, &dir) else {
                continue;
            };
            found.push(LocalModel {
                model_ref,
                format: Format::Mlx,
                path: dir.display().to_string(),
                bytes: files.iter().map(|path| size(path)).sum(),
                files: files.len(),
                partial: !(has_config && has_weights),
            });
        }
    }
    found
}

fn list_gguf(root: &Path) -> Vec<LocalModel> {
    let mut files = Vec::new();
    collect_gguf(root, &mut files, 0);
    let mut groups: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name.starts_with("mmproj") || name.contains("imatrix") {
            continue;
        }
        let key = shard_key(&path);
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, members)) => members.push(path),
            None => groups.push((key, vec![path])),
        }
    }
    groups
        .into_iter()
        .filter_map(|(_, mut members)| {
            members.sort();
            let first = members.first()?;
            let partial = members.iter().all(|path| extension(path) == "part");
            let model_ref = relative_ref(root, &gguf_final(first))?;
            Some(LocalModel {
                model_ref,
                format: Format::Gguf,
                path: gguf_final(first).display().to_string(),
                bytes: members.iter().map(|path| size(path)).sum(),
                files: members.len(),
                partial,
            })
        })
        .collect()
}

fn collect_gguf(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_gguf(&path, out, depth + 1);
        } else if kind.is_file() {
            let name = path.to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".gguf") || name.ends_with(".gguf.part") {
                out.push(path);
            }
        }
    }
}

/// `x.gguf.part` → `x.gguf`.
fn gguf_final(path: &Path) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_suffix(".part")) {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
}

/// The path with the `.part` and shard suffixes removed, so a split and its
/// partial downloads group together.
fn shard_key(path: &Path) -> String {
    let text = gguf_final(path).display().to_string();
    let stem = text.strip_suffix(".gguf").unwrap_or(&text);
    match stem.rsplit_once("-of-") {
        Some((head, total)) if total.len() == 5 && total.bytes().all(|b| b.is_ascii_digit()) => {
            match head.rsplit_once('-') {
                Some((base, part))
                    if part.len() == 5 && part.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    base.to_owned()
                }
                _ => stem.to_owned(),
            }
        }
        _ => stem.to_owned(),
    }
}

/// Deletes the model `model_ref` of `format` from `root`, and only it.
pub fn delete(root: &Path, format: Format, model_ref: &str) -> Result<Deleted, String> {
    if !safe_ref(model_ref) {
        return Err(format!("{model_ref:?} is not a model in the models folder"));
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let named = root.join(model_ref);
    // Resolved, so a symlink cannot lead the deletion outside the folder.
    let target = match format {
        Format::Mlx => named.canonicalize(),
        Format::Gguf => named
            .canonicalize()
            .or_else(|_| PathBuf::from(format!("{}.part", named.display())).canonicalize()),
    }
    .map_err(|_| format!("{model_ref} is not on this machine"))?;
    if !target.starts_with(&root) || target == root {
        return Err(format!("{model_ref} is outside the models folder"));
    }
    let removed = match format {
        Format::Mlx => delete_mlx(&target)?,
        Format::Gguf => delete_gguf(&target)?,
    };
    let freed_bytes = removed.iter().map(|(_, bytes)| bytes).sum();
    prune_empty_parents(&target, &root);
    Ok(Deleted {
        model_ref: model_ref.to_owned(),
        freed_bytes,
        removed: removed
            .into_iter()
            .map(|(path, _)| path.display().to_string())
            .collect(),
    })
}

fn delete_mlx(dir: &Path) -> Result<Vec<(PathBuf, u64)>, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not an MLX model folder", dir.display()));
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    let is_model =
        dir.join("config.json").is_file() || entries.iter().any(|path| extension(path) == "part");
    if !is_model {
        return Err(format!(
            "{} holds no model (no config.json), so it was left alone",
            dir.display()
        ));
    }
    // One model is one folder of files: a folder inside it is something the
    // model manager did not put there, and is not deleted with it.
    if let Some(sub) = entries
        .iter()
        .find(|path| path.symlink_metadata().is_ok_and(|meta| meta.is_dir()))
    {
        return Err(format!(
            "{} contains a folder ({}); move it out before deleting the model",
            dir.display(),
            sub.display()
        ));
    }
    let mut removed = Vec::new();
    for path in entries {
        let bytes = path.symlink_metadata().map(|meta| meta.len()).unwrap_or(0);
        std::fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        removed.push((path, bytes));
    }
    std::fs::remove_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    Ok(removed)
}

fn delete_gguf(target: &Path) -> Result<Vec<(PathBuf, u64)>, String> {
    if !target.is_file() {
        return Err(format!("{} is not a GGUF file", target.display()));
    }
    let key = shard_key(target);
    let dir = target
        .parent()
        .ok_or_else(|| format!("{} has no folder", target.display()))?;
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .flatten()
    {
        let path = entry.path();
        let name = path.to_string_lossy().to_ascii_lowercase();
        if !(name.ends_with(".gguf") || name.ends_with(".gguf.part")) {
            continue;
        }
        if shard_key(&path) != key || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let bytes = size(&path);
        std::fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        removed.push((path, bytes));
    }
    if removed.is_empty() {
        return Err(format!("{} is not a GGUF model", target.display()));
    }
    Ok(removed)
}

/// Removes the folders a deletion emptied, up to but never including `root`.
fn prune_empty_parents(target: &Path, root: &Path) {
    let mut dir = if target.is_dir() {
        Some(target)
    } else {
        target.parent()
    };
    while let Some(current) = dir {
        if current == root || !current.starts_with(root) {
            break;
        }
        if std::fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

/// A reference relative to the models folder, made of plain names only.
fn safe_ref(model_ref: &str) -> bool {
    let path = Path::new(model_ref);
    !model_ref.is_empty()
        && !model_ref.contains('\\')
        && path.components().count() >= 2
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn relative_ref(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.display().to_string())
}

fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect()
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn size(path: &Path) -> u64 {
    path.metadata().map(|meta| meta.len()).unwrap_or(0)
}
