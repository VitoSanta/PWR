//! A session-local test command proved before the model acts. Never a completion proof.
use pwr_domain::ChatMessage;
use pwr_tools::ToolPolicy;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, time::Duration};

pub const LIMIT: Duration = Duration::from_secs(90);
pub const MARKER: &str = "[PWR test recipe]\n";
const END: &str = "\n[/PWR test recipe]";
const MAX_ENTRIES: usize = 8192;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    pub executable: String,
    pub interpreter: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub selection: String,
    pub origin: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attempt {
    pub command: Candidate,
    pub result: Option<pwr_tools::ToolResult>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub aid_enabled: bool,
    pub started_ms: i64,
    pub finished_ms: i64,
    pub duration_ms: u128,
    pub attempts: Vec<Attempt>,
    pub selected_recipe: Option<Candidate>,
    pub executed_tests: Option<usize>,
    pub passed: Option<bool>,
    pub delivered: Option<String>,
    pub absent_reason: Option<String>,
    pub dependencies: BTreeMap<String, String>,
}

impl Report {
    pub fn absent(reason: &str) -> Self {
        Self {
            aid_enabled: true,
            absent_reason: Some(reason.into()),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Standing {
    pub dependencies: BTreeMap<String, String>,
    pub valid: bool,
}

fn metadata(path: &Path) -> String {
    match std::fs::symlink_metadata(path) {
        Ok(m) => format!(
            "{}:{:?}:{:?}",
            m.len(),
            m.modified(),
            std::fs::read_link(path).ok()
        ),
        Err(_) => "absent".into(),
    }
}

fn test_files(root: &Path) -> Result<Vec<String>, String> {
    fn walk(
        root: &Path,
        dir: &Path,
        depth: usize,
        seen: &mut usize,
        files: &mut Vec<String>,
    ) -> Result<(), String> {
        if !dir.exists() {
            return Ok(());
        }
        let mut entries = std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .take(MAX_ENTRIES + 1)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries {
            *seen += 1;
            if *seen > MAX_ENTRIES {
                return Err("test tree exceeds discovery bound".into());
            }
            let kind = e.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("test tree contains a symlink".into());
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "__pycache__" {
                continue;
            }
            if kind.is_dir() {
                if depth >= 16 {
                    return Err("test tree exceeds depth bound".into());
                }
                walk(root, &e.path(), depth + 1, seen, files)?;
            } else if kind.is_file() {
                files.push(
                    e.path()
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    let mut seen = 0;
    for folder in ["tests", "test"] {
        walk(root, &root.join(folder), 0, &mut seen, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn interpreters(root: &Path) -> Vec<String> {
    let mut bins: Vec<_> = std::fs::read_dir(root.join(".toolchains"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path().join("bin"))
        .collect();
    bins.sort();
    bins.extend([root.join(".venv/bin"), root.join("venv/bin")]);
    bins.into_iter()
        .flat_map(|bin| [bin.join("python"), bin.join("python3")])
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Configuration and test topology; no application source enters this fingerprint.
pub fn snapshot(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let files = test_files(root)?;
    let mut values = BTreeMap::new();
    for name in [
        "pyproject.toml",
        "setup.cfg",
        "setup.py",
        "pytest.ini",
        "tox.ini",
        "requirements.txt",
        ".pwr/chat-config.json",
        "runtests.py",
        "run_tests.py",
        "conftest.py",
    ] {
        let path = root.join(name);
        let stamp = if path.is_file() {
            if path.metadata().map_err(|e| e.to_string())?.len() > 1024 * 1024 {
                return Err("configuration too large".into());
            }
            pwr_domain::hash_bytes(&std::fs::read(&path).map_err(|e| e.to_string())?)
        } else {
            "absent".into()
        };
        values.insert(name.into(), stamp);
    }
    for name in files {
        let path = root.join(&name);
        // Cache output is deliberately absent; changing test files or their topology expires the recipe.
        values.insert(name, metadata(&path));
    }
    for executable in interpreters(root) {
        let path = Path::new(&executable);
        values.insert(executable.clone(), metadata(path));
        if let Ok(target) = path.canonicalize() {
            values.insert(format!("interpreter:{executable}"), metadata(&target));
        }
        let env = path
            .parent()
            .and_then(Path::parent)
            .ok_or("interpreter has no environment")?;
        for name in ["pyvenv.cfg", "bin", "lib"] {
            values.insert(
                env.join(name).to_string_lossy().into_owned(),
                metadata(&env.join(name)),
            );
        }
        fn packages(
            path: &Path,
            depth: usize,
            seen: &mut usize,
            values: &mut BTreeMap<String, String>,
        ) -> Result<(), String> {
            if depth > 16 {
                return Err("environment exceeds depth bound".into());
            }
            for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_name() == "__pycache__" {
                    continue;
                }
                *seen += 1;
                if *seen > MAX_ENTRIES {
                    return Err("environment exceeds discovery bound".into());
                }
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                let key = entry.path().to_string_lossy().into_owned();
                if kind.is_dir() {
                    // Ignore directory mtimes changed only by generated bytecode.
                    values.insert(key, "package directory".into());
                    packages(&entry.path(), depth + 1, seen, values)?;
                } else {
                    values.insert(key, metadata(&entry.path()));
                }
            }
            Ok(())
        }
        let mut seen = 0;
        if let Ok(versions) = std::fs::read_dir(env.join("lib")) {
            for version in versions.take(MAX_ENTRIES + 1) {
                let directory = version
                    .map_err(|e| e.to_string())?
                    .path()
                    .join("site-packages");
                if directory.is_dir() {
                    packages(&directory, 0, &mut seen, &mut values)?;
                }
            }
        }
    }
    Ok(values)
}

fn selection(source: &str) -> Option<(Option<String>, String)> {
    let mut class = None;
    for line in source.lines() {
        if let Some(after) = line.strip_prefix("class ") {
            class = Some(after.split(['(', ':']).next()?.trim().to_owned());
        } else if !line.starts_with(char::is_whitespace)
            && !line.starts_with('@')
            && !line.is_empty()
            && !line.starts_with('#')
        {
            class = None;
        }
        let trimmed = line.trim_start();
        if let Some(after) = trimmed.strip_prefix("def test_") {
            let name = format!("test_{}", after.split('(').next()?);
            if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && trimmed.contains('(')
            {
                return Some((class, name));
            }
        }
    }
    None
}

pub fn candidates(root: &Path) -> Result<Vec<Candidate>, String> {
    let Some(python) = interpreters(root).into_iter().next() else {
        return Ok(Vec::new());
    };
    let name = Path::new(&python)
        .file_name()
        .ok_or("interpreter has no name")?
        .to_string_lossy()
        .into_owned();
    let resolved = pwr_tools::toolchain_paths(root)
        .into_iter()
        .map(|bin| bin.join(&name))
        .find(|p| p.is_file());
    let executable = if resolved.as_ref() == Some(&std::path::PathBuf::from(&python)) {
        name
    } else {
        python.clone()
    };
    let mut tests = Vec::new();
    for name in test_files(root)? {
        let path = root.join(&name);
        if Path::new(&name).components().count() > 5 {
            continue;
        }
        if !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("test_"))
            || path.extension().is_none_or(|e| e != "py")
        {
            continue;
        }
        let size = path.metadata().map_err(|e| e.to_string())?.len();
        if size > 16 * 1024 {
            continue;
        }
        let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if let Some((class, test)) = selection(&source) {
            tests.push((size, name, class, test));
        }
    }
    tests.sort();
    let runner = [
        "runtests.py",
        "run_tests.py",
        "tests/runtests.py",
        "tests/run_tests.py",
        "test/runtests.py",
        "test/run_tests.py",
    ]
    .into_iter()
    .find(|p| root.join(p).is_file());
    let pytest = root.join("pytest.ini").is_file()
        || root.join("conftest.py").is_file()
        || ["pyproject.toml", "setup.cfg"].iter().any(|p| {
            std::fs::read_to_string(root.join(p))
                .is_ok_and(|s| s.contains("[tool.pytest") || s.contains("[tool:pytest]"))
        });
    let mut found = Vec::new();
    for (_, file, class, test) in tests.into_iter().take(2) {
        if let Some(runner) = runner {
            let parent = Path::new(runner).parent().unwrap_or(Path::new(""));
            if let Ok(relative) = Path::new(&file).strip_prefix(parent) {
                let mut label = relative
                    .with_extension("")
                    .to_string_lossy()
                    .replace('/', ".");
                if let Some(class) = &class {
                    label.push('.');
                    label.push_str(class);
                }
                label.push('.');
                label.push_str(&test);
                found.push(Candidate {
                    executable: executable.clone(),
                    interpreter: python.clone(),
                    argv: vec![runner.into(), label.clone()],
                    cwd: ".".into(),
                    selection: label,
                    origin: runner.into(),
                });
            }
        }
        if pytest {
            let target = format!(
                "{}{}::{test}",
                file,
                class.as_ref().map(|c| format!("::{c}")).unwrap_or_default()
            );
            found.push(Candidate {
                executable: executable.clone(),
                interpreter: python.clone(),
                argv: vec!["-m".into(), "pytest".into(), target.clone()],
                cwd: ".".into(),
                selection: target,
                origin: "pytest configuration".into(),
            });
        }
    }
    found.truncate(3);
    Ok(found)
}

pub async fn prepare(policy: &ToolPolicy) -> Report {
    let start = tokio::time::Instant::now();
    let started_ms = chrono::Utc::now().timestamp_millis();
    let mut report = match tokio::time::timeout(LIMIT, prepare_inner(policy)).await {
        Ok(report) => report,
        Err(_) => Report::absent("preparation deadline reached"),
    };
    report.started_ms = started_ms;
    finish(report, start)
}

async fn prepare_inner(policy: &ToolPolicy) -> Report {
    let start = tokio::time::Instant::now();
    let mut report = Report::absent("no bounded candidate with a workspace Python environment");
    report.started_ms = chrono::Utc::now().timestamp_millis();
    let root = policy.root.clone();
    let discovered = tokio::task::spawn_blocking(move || {
        Ok::<_, String>((snapshot(&root)?, candidates(&root)?))
    })
    .await;
    let (before, commands) = match discovered {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            report.absent_reason = Some(e);
            return finish(report, start);
        }
        Err(e) => {
            report.absent_reason = Some(e.to_string());
            return finish(report, start);
        }
    };
    for command in commands {
        // Four seconds are reserved for the tool's bounded pipe cleanup after timeout.
        let Some(left) = LIMIT.checked_sub(start.elapsed() + Duration::from_secs(4)) else {
            break;
        };
        if left.is_zero() {
            break;
        }
        let mut bounded = policy.clone();
        bounded.timeout = bounded.timeout.min(left);
        let outcome = pwr_tools::run_command_in(
            &bounded,
            &command.executable,
            &command.argv,
            None,
            Some(&command.cwd),
        )
        .await;
        let (result, error) = match outcome {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(e.to_string())),
        };
        if let Some(result) = &result {
            let output = format!("{}\n{}", result.stdout, result.stderr);
            if result.exit_code.is_some()
                && !result.stdout_truncated
                && !result.stderr_truncated
                && let Some(count) = pwr_verify::evidence::executed_tests(&output)
            {
                if snapshot(&policy.root).ok().as_ref() == Some(&before) {
                    let passed = result.exit_code == Some(0);
                    let quoted: Vec<_> = std::iter::once(command.executable.as_str())
                        .chain(command.argv.iter().map(String::as_str))
                        .map(shell_words::quote)
                        .collect();
                    report.delivered = Some(format!(
                        "{MARKER}Before your work, PWR executed `{}` from `{}` using interpreter `{}`: {count} tests executed, {}. This proves only this exact selection: `{}`. Use this runner's selection syntax for tests around your changes; other selections have not been proved. This initial result does not confirm your edits or completion. The recipe expires when test configuration, runner, environment or test structure changes.{END}",
                        quoted.join(" "),
                        command.cwd,
                        command.interpreter,
                        if passed { "passed" } else { "failed" },
                        command.selection
                    ));
                    report.selected_recipe = Some(command.clone());
                    report.executed_tests = Some(count);
                    report.passed = Some(passed);
                    report.absent_reason = None;
                    report.dependencies = before.clone();
                } else {
                    report.absent_reason = Some("dependencies changed during preparation".into());
                }
            }
        }
        report.attempts.push(Attempt {
            command,
            result,
            error,
        });
        if report.selected_recipe.is_some() {
            break;
        }
    }
    finish(report, start)
}

fn finish(mut report: Report, start: tokio::time::Instant) -> Report {
    report.duration_ms = start.elapsed().as_millis();
    report.finished_ms = chrono::Utc::now().timestamp_millis();
    report
}

/// Before a generation, remove the valid note if any dependency changed. No new probe.
pub fn expire(root: &Path, standing: &mut Standing, messages: &mut [ChatMessage]) -> bool {
    if !standing.valid || snapshot(root).ok().as_ref() == Some(&standing.dependencies) {
        return false;
    }
    standing.valid = false;
    for message in messages {
        if let Some(begin) = message.content.find(MARKER)
            && let Some(end) = message.content[begin..].find(END)
        {
            message.content.replace_range(begin..begin + end + END.len(), "[PWR test recipe expired: its configuration, test structure or environment changed. The earlier command and result are no longer a valid recipe.]");
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace(output: &str) -> (tempfile::TempDir, ToolPolicy) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("tests/unit")).unwrap();
        fs::create_dir_all(root.join(".toolchains/fixture/bin")).unwrap();
        fs::write(root.join("tests/runtests.py"), "# runner\n").unwrap();
        fs::write(
            root.join("tests/unit/test_value.py"),
            "class ValueTests:\n    def test_value(self):\n        pass\n",
        )
        .unwrap();
        let python = root.join(".toolchains/fixture/bin/python");
        fs::write(&python, format!("#!/bin/sh\n{output}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let policy = ToolPolicy {
            root: root.to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![python.to_string_lossy().into_owned(), "python".into()],
            output_limit: 4096,
            timeout: Duration::from_secs(1),
            sandbox: pwr_tools::SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        (dir, policy)
    }

    #[tokio::test]
    async fn proves_exact_selection_and_keeps_failure_as_failure() {
        let (_dir, policy) =
            workspace("printf 'Ran 1 test in 0.01s\\nFAILED (failures=1)\\n'; exit 1");
        let r = prepare(&policy).await;
        assert_eq!(r.executed_tests, Some(1), "{r:?}");
        assert_eq!(r.passed, Some(false));
        assert_eq!(
            r.selected_recipe.as_ref().unwrap().selection,
            "unit.test_value.ValueTests.test_value"
        );
        assert!(r.delivered.unwrap().contains("does not confirm your edits"));
        assert_eq!(r.attempts.len(), 1);
        assert!(r.finished_ms >= r.started_ms);
    }

    #[tokio::test]
    async fn zero_tests_and_collection_errors_never_make_a_recipe() {
        for output in [
            "printf 'Ran 0 tests in 0.01s\\nOK\\n'",
            "printf 'Ran 1 test in 0.01s\\nImportError: Failed to import test module\\n'",
        ] {
            let (_dir, policy) = workspace(output);
            let r = prepare(&policy).await;
            assert!(r.selected_recipe.is_none());
            assert!(r.delivered.is_none());
            assert!(!r.attempts.is_empty());
        }
    }

    #[tokio::test]
    async fn a_proved_recipe_expires_on_config_test_structure_or_environment() {
        let (dir, policy) = workspace("printf 'Ran 1 test in 0.01s\\nOK\\n'");
        let r = prepare(&policy).await;
        for path in [
            "pytest.ini",
            "tests/unit/new_file.py",
            ".toolchains/fixture/pyvenv.cfg",
        ] {
            let mut standing = Standing {
                valid: true,
                dependencies: r.dependencies.clone(),
            };
            let mut messages = vec![ChatMessage {
                role: "user".into(),
                content: r.delivered.clone().unwrap(),
                ..Default::default()
            }];
            fs::write(dir.path().join(path), "changed\n").unwrap();
            assert!(expire(dir.path(), &mut standing, &mut messages));
            assert!(!standing.valid);
            assert!(!messages[0].content.contains(MARKER));
            fs::remove_file(dir.path().join(path)).unwrap();
        }
    }

    #[tokio::test]
    async fn application_source_edits_do_not_turn_initial_tests_into_confirmation() {
        let (dir, policy) = workspace("printf 'Ran 1 test in 0.01s\\nOK\\n'");
        let r = prepare(&policy).await;
        fs::write(dir.path().join("application.py"), "changed = True\n").unwrap();
        let mut standing = Standing {
            valid: true,
            dependencies: r.dependencies,
        };
        assert!(!expire(dir.path(), &mut standing, &mut []));
        assert!(
            crate::confirmation::Confirmation::default()
                .standing()
                .is_none()
        );
    }

    #[tokio::test]
    async fn missing_workspace_environment_does_not_fall_back_to_host_python() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tests")).unwrap();
        fs::write(dir.path().join("tests/test_x.py"), "def test_x(): pass\n").unwrap();
        assert!(candidates(dir.path()).unwrap().is_empty());
    }

    #[tokio::test]
    async fn unlaunchable_command_and_timeout_are_absent_not_passed() {
        let (_dir, mut policy) = workspace("sleep 5; printf 'Ran 1 test in 0.01s\\nOK\\n'");
        policy.timeout = Duration::from_millis(50);
        let r = prepare(&policy).await;
        assert!(r.delivered.is_none());
        assert!(r.duration_ms < 5000);
        assert!(r.attempts[0].error.is_some());
        fs::write(
            policy.root.join(".toolchains/fixture/bin/python"),
            "not an executable image",
        )
        .unwrap();
        let r = prepare(&policy).await;
        assert!(r.delivered.is_none());
        assert!(
            r.attempts[0].error.is_some()
                || r.attempts[0]
                    .result
                    .as_ref()
                    .is_some_and(|r| r.exit_code != Some(0))
        );
    }

    #[test]
    fn helpers_are_not_selected_and_pytest_has_an_exact_node() {
        assert_eq!(
            selection("def helper(): pass\nclass X:\n    def test_one(self): pass\n"),
            Some((Some("X".into()), "test_one".into()))
        );
        let (dir, _) = workspace("exit 0");
        fs::remove_file(dir.path().join("tests/runtests.py")).unwrap();
        fs::write(dir.path().join("pytest.ini"), "[pytest]\n").unwrap();
        let found = candidates(dir.path()).unwrap();
        assert_eq!(
            found[0].selection,
            "tests/unit/test_value.py::ValueTests::test_value"
        );
    }
    #[tokio::test]
    async fn denied_commands_do_not_widen_the_session_policy() {
        let (_dir, mut policy) = workspace("printf 'Ran 1 test in 0.1s\\nOK\\n'");
        policy.allow_commands.clear();
        let r = prepare(&policy).await;
        assert!(r.delivered.is_none());
        assert!(r.attempts.iter().all(|a| {
            a.error
                .as_ref()
                .is_some_and(|e| e.contains("policy denied"))
        }));
        assert!(policy.allow_commands.is_empty());
    }

    #[tokio::test]
    async fn timeout_closes_child_process_before_returning() {
        let (dir, policy) = workspace("(sleep 1.3; echo leaked > leaked) & wait");
        let r = prepare(&policy).await;
        assert!(r.delivered.is_none());
        assert!(r.duration_ms < 90_000);
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(
            !dir.path().join("leaked").exists(),
            "a timed-out child survived"
        );
    }

    #[tokio::test]
    #[ignore = "explicit clean development checkout only; executes one bounded selection"]
    async fn probe_clean_checkout() {
        let root = std::path::PathBuf::from(
            std::env::var("PWR_RECIPE_PROBE_ROOT").expect("supply a clean, unsealed checkout"),
        );
        let policy = ToolPolicy {
            allow_commands: pwr_verify::required_executables(&root),
            extra_readable: pwr_tools::dependency_roots(&root),
            protected: Vec::new(),
            root,
            output_limit: 64 * 1024,
            timeout: Duration::from_secs(30),
            sandbox: pwr_tools::SandboxPolicy::FullAccess,
            approvals: Vec::new(),
        };
        let report = prepare(&policy).await;
        println!("RECIPE_REPORT {}", serde_json::to_string(&report).unwrap());
        assert!(report.duration_ms <= 90_000);
    }
}
