//! The owner's acceptance evidence, captured before any model action.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceSnapshot {
    pub contract_hash: String,
    pub artifacts: BTreeMap<String, Option<String>>,
    pub inferred: bool,
}

/// Compare both the declaration and selected evidence against the frozen
/// session baseline. Authorization is per file, never a blanket reset.
pub fn changed_contract(
    root: &Path,
    before: &AcceptanceSnapshot,
    authorized: &std::collections::BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let current = snapshot(root)?;
    let mut changed = current
        .as_ref()
        .map(|after| before.changed_files(after))
        .unwrap_or_else(|| before.artifacts.keys().cloned().collect());
    if current
        .as_ref()
        .is_none_or(|after| before.contract_hash != after.contract_hash)
    {
        changed.push(".pwr/checks.json".into());
    }
    changed.retain(|path| !authorized.contains(path));
    Ok(changed)
}

impl AcceptanceSnapshot {
    pub fn digest(&self) -> String {
        pwr_domain::hash_bytes(
            serde_json::to_vec(self).expect("snapshot contains only JSON values"),
        )
    }
    pub fn changed_files(&self, current: &Self) -> Vec<String> {
        self.artifacts
            .keys()
            .chain(current.artifacts.keys())
            .filter(|path| self.artifacts.get(*path) != current.artifacts.get(*path))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Explicit paths/globs select evidence; absent declarations infer test files
/// conservatively. Unreadable, escaped or excessively large evidence fails
/// closed instead of certifying only the subset that could be read.
pub fn snapshot(root: &Path) -> Result<Option<AcceptanceSnapshot>, String> {
    let acceptance_checks = super::declared_acceptance_checks(root)?;
    if acceptance_checks.is_empty() {
        return Ok(None);
    }
    #[derive(Deserialize, Default)]
    struct Artifacts {
        #[serde(default)]
        artifacts: Vec<String>,
    }
    #[derive(Deserialize)]
    struct Declaration {
        #[serde(default)]
        acceptance: Artifacts,
    }
    let bytes = std::fs::read(root.join(".pwr/checks.json")).map_err(|e| e.to_string())?;
    let declaration: Declaration = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let patterns = declaration.acceptance.artifacts;
    for pattern in &patterns {
        if pattern.len() > 1024
            || pattern.split('/').count() > 64
            || pattern.is_empty()
            || Path::new(pattern)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || pattern.contains(['[', ']', '\\'])
        {
            return Err(format!(
                "invalid acceptance artifact `{pattern}`: use workspace-relative paths or *, ** and ? globs"
            ));
        }
    }
    // These directories are deliberately outside the bounded inventory. Never
    // silently accept an explicit declaration whose evidence cannot be hashed.
    for pattern in &patterns {
        if pattern.split('/').any(|component| {
            matches!(
                component,
                ".git"
                    | ".pwr"
                    | "target"
                    | "node_modules"
                    | "vendor"
                    | ".venv"
                    | "venv"
                    | "__pycache__"
                    | "dist"
                    | "build"
            )
        }) {
            return Err(format!(
                "acceptance artifact `{pattern}` selects an excluded directory; place acceptance evidence in a source directory"
            ));
        }
    }
    let inferred = patterns.is_empty();
    let cargo_tests = acceptance_checks.iter().any(|(executable, args)| {
        executable == "cargo" && args.first().is_some_and(|arg| arg == "test")
    });
    let direct_artifacts: Vec<_> = acceptance_checks
        .iter()
        .flat_map(|(_, args)| args)
        .filter(|arg| !arg.starts_with('-') && root.join(arg).is_file())
        .collect();
    let mut files = Vec::new();
    let mut visited = 0;
    walk(root, root, 0, &mut visited, &mut files)?;
    let mut artifacts = BTreeMap::new();
    for path in files {
        let selected = if inferred {
            inferred_artifact(&path) || direct_artifacts.iter().any(|arg| arg.as_str() == path)
        } else {
            patterns.iter().any(|pattern| {
                glob(pattern, &path)
                    || (!pattern.contains(['*', '?']) && path.starts_with(&format!("{pattern}/")))
            })
        };
        let inline_tests = inferred && cargo_tests && path.ends_with(".rs");
        if !selected && !inline_tests {
            continue;
        }
        let physical = root.join(&path);
        let canonical = physical
            .canonicalize()
            .map_err(|e| format!("{path}: {e}"))?;
        if !canonical.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
            return Err(format!("acceptance artifact escapes workspace: {path}"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(physical)
            .map_err(|e| format!("{path}: {e}"))?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{path}: {e}"))?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(format!("acceptance artifact exceeds 64 MiB: {path}"));
        }
        if !selected
            && !bytes
                .windows(b"#[test]".len())
                .any(|part| part == b"#[test]")
            && !bytes
                .windows(b"#[cfg(test)]".len())
                .any(|part| part == b"#[cfg(test)]")
            && !bytes
                .windows(b"#[tokio::test]".len())
                .any(|part| part == b"#[tokio::test]")
        {
            continue;
        }
        artifacts.insert(path, Some(pwr_domain::hash_bytes(bytes)));
    }
    // Exact absent paths remain part of the evidence: creating them changes it.
    for pattern in patterns {
        if !pattern.contains(['*', '?']) {
            artifacts.entry(pattern).or_insert(None);
        }
    }
    Ok(Some(AcceptanceSnapshot {
        contract_hash: pwr_domain::hash_bytes(bytes),
        artifacts,
        inferred,
    }))
}

fn walk(
    root: &Path,
    directory: &Path,
    depth: usize,
    visited: &mut usize,
    files: &mut Vec<String>,
) -> Result<(), String> {
    if depth > 64 {
        return Err("acceptance inventory exceeds 64 directory levels".into());
    }
    for entry in
        std::fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        *visited += 1;
        if *visited > 100_000 {
            return Err("acceptance inventory exceeds 100000 entries; narrow the workspace".into());
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if kind.is_dir()
            || (kind.is_symlink() && entry.metadata().is_ok_and(|metadata| metadata.is_dir()))
        {
            if [
                ".git",
                ".pwr",
                "target",
                "node_modules",
                "vendor",
                ".venv",
                "venv",
                "__pycache__",
                "dist",
                "build",
            ]
            .iter()
            .any(|skip| name == *skip)
            {
                continue;
            }
            if !entry
                .path()
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(root.canonicalize().map_err(|e| e.to_string())?)
            {
                return Err(format!(
                    "acceptance inventory directory escapes workspace: {}",
                    entry.path().display()
                ));
            }
            walk(root, &entry.path(), depth + 1, visited, files)?;
        } else if kind.is_file() || kind.is_symlink() {
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

fn inferred_artifact(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/').any(|part| {
        matches!(
            part,
            "tests" | "test" | "__tests__" | "fixtures" | "e2e" | "scripts"
        )
    }) || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || name.ends_with("_test.go")
        || matches!(
            name,
            "Cargo.toml"
                | "Cargo.lock"
                | "package.json"
                | "package-lock.json"
                | "pyproject.toml"
                | "pytest.ini"
        )
}

fn glob(pattern: &str, path: &str) -> bool {
    fn segments(
        p: &[&str],
        s: &[&str],
        i: usize,
        j: usize,
        cache: &mut BTreeMap<(usize, usize), bool>,
    ) -> bool {
        if let Some(result) = cache.get(&(i, j)) {
            return *result;
        }
        let result = if i == p.len() {
            j == s.len()
        } else if p[i] == "**" {
            segments(p, s, i + 1, j, cache) || (j < s.len() && segments(p, s, i, j + 1, cache))
        } else {
            j < s.len() && wildcard(p[i], s[j]) && segments(p, s, i + 1, j + 1, cache)
        };
        cache.insert((i, j), result);
        result
    }
    segments(
        &pattern.split('/').collect::<Vec<_>>(),
        &path.split('/').collect::<Vec<_>>(),
        0,
        0,
        &mut BTreeMap::new(),
    )
}
fn wildcard(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    let (mut p, mut v, mut star, mut retry) = (0, 0, None, 0);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            retry = v;
        } else if let Some(at) = star {
            retry += 1;
            v = retry;
            p = at + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace(declaration: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".pwr")).unwrap();
        std::fs::create_dir(root.path().join("tests")).unwrap();
        std::fs::write(root.path().join(".pwr/checks.json"), declaration).unwrap();
        root
    }
    #[test]
    fn weakening_or_adding_acceptance_tests_changes_the_snapshot() {
        let root = workspace(
            r#"{"checks":[{"executable":"cargo","args":["test"],"kind":"acceptance"}],"acceptance":{"artifacts":["tests/**/*.rs"]}}"#,
        );
        let path = root.path().join("tests/acceptance.rs");
        std::fs::write(&path, "assert!(false)").unwrap();
        let before = snapshot(root.path()).unwrap().unwrap();
        std::fs::write(&path, "assert!(true)").unwrap();
        let after = snapshot(root.path()).unwrap().unwrap();
        assert_ne!(before.digest(), after.digest());
        assert_eq!(before.changed_files(&after), ["tests/acceptance.rs"]);
        std::fs::write(root.path().join("tests/new.rs"), "test").unwrap();
        assert_eq!(
            before
                .changed_files(&snapshot(root.path()).unwrap().unwrap())
                .len(),
            2
        );
    }
    #[test]
    fn inference_captures_cargo_and_npm_tests_and_configuration() {
        let root = workspace(r#"{"checks":[{"executable":"npm","kind":"acceptance"}]}"#);
        std::fs::write(root.path().join("tests/acceptance.rs"), "cargo").unwrap();
        std::fs::write(root.path().join("view.spec.ts"), "npm").unwrap();
        std::fs::write(root.path().join("package.json"), "{}").unwrap();
        let snapshot = snapshot(root.path()).unwrap().unwrap();
        assert!(snapshot.inferred);
        assert_eq!(snapshot.artifacts.len(), 3);
    }
    #[test]
    fn cargo_inline_tests_are_frozen_as_inferred_evidence() {
        let root =
            workspace(r#"{"checks":[{"executable":"cargo","args":["test"],"kind":"acceptance"}]}"#);
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("src/lib.rs"),
            "#[cfg(test)] mod tests { #[test] fn works() {} }",
        )
        .unwrap();
        assert!(
            snapshot(root.path())
                .unwrap()
                .unwrap()
                .artifacts
                .contains_key("src/lib.rs")
        );
    }
    #[test]
    fn exact_directory_declarations_cover_their_descendants() {
        let root = workspace(
            r#"{"checks":[{"executable":"cargo","kind":"acceptance"}],"acceptance":{"artifacts":["tests"]}}"#,
        );
        std::fs::write(root.path().join("tests/acceptance.rs"), "evidence").unwrap();
        assert!(
            snapshot(root.path())
                .unwrap()
                .unwrap()
                .artifacts
                .contains_key("tests/acceptance.rs")
        );
    }

    #[test]
    fn explicit_evidence_in_excluded_directories_is_rejected() {
        for path in ["target/test.rs", "vendor/tests", ".pwr/evidence.json"] {
            let root = workspace(
                &serde_json::json!({
                    "checks": [{"executable":"cargo", "kind":"acceptance"}],
                    "acceptance": {"artifacts":[path]}
                })
                .to_string(),
            );
            assert!(
                snapshot(root.path())
                    .unwrap_err()
                    .contains("excluded directory")
            );
        }
    }

    #[test]
    fn escaped_evidence_fails_closed() {
        let root = workspace(
            r#"{"checks":[{"executable":"cargo","kind":"acceptance"}],"acceptance":{"artifacts":["../outside"]}}"#,
        );
        assert!(snapshot(root.path()).is_err());
    }
    #[test]
    fn glob_is_path_aware() {
        assert!(glob("tests/**/*.rs", "tests/a.rs"));
        assert!(glob("tests/**/*.rs", "tests/nested/a.rs"));
        assert!(!glob("tests/*.rs", "tests/nested/a.rs"));
        assert!(!glob("tests/**/*.rs", "src/a.rs"));
    }
}
