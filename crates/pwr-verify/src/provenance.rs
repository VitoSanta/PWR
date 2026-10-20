//! Bounded source identity for checks. A green process is not a timeless proof.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path};

const EXCLUDED: &[&str] = &[
    ".git",
    ".pwr-scratch",
    ".pwr",
    "target",
    "node_modules",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    "dist",
    "build",
    "coverage",
];
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub files: BTreeMap<String, String>,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub schema_version: u32,
    pub root: String,
    pub scope: String,
    pub commands: Vec<(String, Vec<String>)>,
    pub policy_hash: String,
    /// Counts from recognized runner summaries; None is unknown, not zero.
    pub executed_tests: Vec<Option<usize>>,
    pub before: SourceSnapshot,
    pub after: SourceSnapshot,
}
impl Provenance {
    pub fn validate_current(&self, root: &Path) -> Result<(), String> {
        if self.before != self.after {
            return Err("source scope changed while checks ran; results are stale".into());
        }
        if root
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            != self.root
        {
            return Err("verification belongs to a different workspace".into());
        }
        if snapshot(root, &self.commands)? != self.after {
            return Err("source scope changed after verification; rerun the checks".into());
        }
        Ok(())
    }
}
pub fn snapshot(root: &Path, commands: &[(String, Vec<String>)]) -> Result<SourceSnapshot, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let walker = ignore::WalkBuilder::new(&root)
        .hidden(false)
        .parents(false)
        .require_git(false)
        .git_global(false)
        .follow_links(false)
        .filter_entry(|entry| {
            !EXCLUDED.iter().any(|skip| entry.file_name() == *skip)
                && !entry.file_name().to_string_lossy().starts_with(".pwr-tmp-")
        })
        .build();
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for (visited, entry) in walker.enumerate() {
        if visited >= 100_000 {
            return Err("verification scope exceeds 100000 entries".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.depth() > 64 {
            return Err("verification scope exceeds 64 directory levels".into());
        }
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            continue;
        }
        add(&root, entry.path(), &mut files, &mut total)?;
    }
    // Internal state is excluded, but the verifier declaration is evidence.
    let declaration = root.join(".pwr/checks.json");
    if declaration.exists() {
        add(&root, &declaration, &mut files, &mut total)?;
    }
    // Explicit local script/fixture arguments remain covered even if ignored.
    for (executable, args) in commands {
        for arg in std::iter::once(executable).chain(args) {
            let path = root.join(arg);
            if path.is_file() && path.starts_with(&root) {
                add(&root, &path, &mut files, &mut total)?;
            }
        }
    }
    let digest = pwr_domain::hash_bytes(serde_json::to_vec(&files).map_err(|e| e.to_string())?);
    Ok(SourceSnapshot { files, digest })
}
fn add(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<String, String>,
    total: &mut u64,
) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    if files.contains_key(&relative) {
        return Ok(());
    }
    // Symlinks (including in explicit arguments) cannot import unbounded or
    // external dependencies into what this snapshot claims to have examined.
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("{relative}: {e}"))?;
    if !canonical.starts_with(root) {
        return Err(format!("verification scope escapes workspace: {relative}"));
    }
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return Err(format!("verification scope contains symlink: {relative}"));
    }
    if !meta.is_file() {
        return Err(format!(
            "verification scope is not a regular file: {relative}"
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    *total += bytes.len() as u64;
    if bytes.len() > 64 * 1024 * 1024 || *total > 256 * 1024 * 1024 {
        return Err("verification scope exceeds file/document byte limits".into());
    }
    files.insert(relative, pwr_domain::hash_bytes(bytes));
    Ok(())
}
