//! The tests related to a source file, run by the core before and after the
//! file changes (goal aid `core_check`, off unless asked for).
//!
//! Measured 2026-10-13 on sixty recorded runs (Qwen 3.5 9B, three SWE-bench
//! instances, four settings of the aids): 499 of 899 minutes came after the
//! source had reached its final state, 69% of the time in the forty resolved
//! runs, where the source was final at a median of 2.3 minutes. What the
//! model did with that time was look for the project's tests, write their
//! selection in a form the runner does not take, and read failures that were
//! there before it changed anything.
//!
//! Those three things are known here without a model: which test paths
//! resemble the file, how this project's runner names them, and which
//! failures are new. So the core runs them -- once before the first change to
//! a file, again after each change -- and says, with the edit's result, only
//! what the change did to them. Tests that all pass on the source as it
//! stands are the confirmation `accept_stop` already knows how to take.
//!
//! It says what the related tests say and no more: the tests that exist
//! passing is not the request being met, and the note says so. Where no test
//! resembles the file, or the runner's words are not known, nothing is said.
//!
//! Replayed without a model on the 43 distinct patches of those runs: no new
//! failure on any of the 40 resolved runs' patches, new failures on 4 of the
//! 20 unresolved ones, 3.7 seconds a check at the median.

use pwr_tools::ToolPolicy;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

/// How long one run of the related tests may take.
pub const LIMIT: Duration = Duration::from_secs(45);
/// How much of a run's output is kept to read its report from.
const OUTPUT: usize = 256 * 1024;
const MAX_ENTRIES: usize = 50_000;
const MAX_FILES: usize = 8;
const MAX_FOLDERS: usize = 3;
const NAMED: usize = 8;

/// A source file whose related tests this module knows how to look for.
pub fn is_source(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e == "py")
        && !is_test_file(path)
        && !crate::confirmation::is_test_path(path)
}

fn is_test_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".py")
        && (name.starts_with("test_") || name.ends_with("_test.py") || name == "tests.py")
}

fn stem(word: &str) -> String {
    let word = word.to_lowercase();
    match word.strip_suffix('s') {
        Some(short) if word.len() > 3 => short.to_owned(),
        _ => word,
    }
}

fn tokens(path: &str) -> BTreeSet<String> {
    path.split(['/', '_', '.', '-'])
        .filter(|part| !part.is_empty())
        .map(stem)
        .collect()
}

fn test_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, depth: usize, seen: &mut usize, files: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            *seen += 1;
            if *seen > MAX_ENTRIES {
                return;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_symlink() || name.starts_with('.') || name == "__pycache__" {
                continue;
            }
            if kind.is_dir() {
                if depth < 16 {
                    walk(root, &entry.path(), depth + 1, seen, files);
                }
            } else if kind.is_file()
                && let Ok(relative) = entry.path().strip_prefix(root)
            {
                let relative = relative.to_string_lossy().replace('\\', "/");
                if is_test_file(&relative) {
                    files.push(relative);
                }
            }
        }
    }
    let mut files = Vec::new();
    let mut seen = 0;
    for top in ["tests", "test"] {
        walk(root, &root.join(top), 0, &mut seen, &mut files);
    }
    files.sort();
    files
}

/// Test paths related to a source file, and why these: files, or folders
/// ending in `/`. Three tiers, the first that finds something decides.
///
/// - the source's package and its file name, the name being in the test
///   file's own name or in a folder's (`django/forms/fields.py` and
///   `tests/forms_tests/field_tests/`);
/// - the file name alone, where no test carries the package: a flat test
///   tree (`sphinx/builders/gettext.py` and `tests/test_build_gettext.py`);
/// - the package alone, where it names one test folder.
///
/// A name shared by tests of several packages (`test_fields.py` under the
/// model, admin and GIS tests) is not taken for a file of another package:
/// the first version of this rule did, and chose the model fields' tests
/// for the forms' fields.
pub fn related_tests(root: &Path, source: &str) -> (Vec<String>, String) {
    let without = source.strip_suffix(".py").unwrap_or(source);
    let mut parts: Vec<&str> = without.split('/').filter(|part| !part.is_empty()).collect();
    // The project's own top folder says nothing of which tests.
    if parts.len() > 1 && root.join(parts[0]).is_dir() {
        parts.remove(0);
    }
    let Some(last) = parts.last() else {
        return (Vec::new(), "the path names no file".into());
    };
    let file_stem = stem(last);
    if file_stem.len() < 3 || matches!(file_stem.as_str(), "test" | "init" | "__init__" | "main") {
        return (
            Vec::new(),
            "the file's name says nothing of which tests".into(),
        );
    }
    let package: BTreeSet<String> = parts[..parts.len() - 1].iter().map(|p| stem(p)).collect();
    let mut tiers: [Vec<(usize, String)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for test in test_files(root) {
        let (folder, name) = test.rsplit_once('/').unwrap_or(("", &test));
        let in_name = tokens(name).contains(&file_stem);
        let in_folder = tokens(folder).contains(&file_stem);
        let all = tokens(&test);
        let packaged = package.iter().filter(|p| all.contains(*p)).count();
        if (in_name || in_folder) && packaged > 0 {
            let score = packaged + if in_name { 2 } else { 0 } + if in_folder { 2 } else { 0 };
            tiers[0].push((score, test));
        } else if in_name || in_folder {
            tiers[1].push((1, test));
        } else if packaged > 0 {
            tiers[2].push((packaged, test));
        }
    }
    for (index, scored) in tiers.iter().enumerate() {
        let Some(best) = scored.iter().map(|(score, _)| *score).max() else {
            continue;
        };
        let chosen: Vec<&String> = scored
            .iter()
            .filter(|(score, _)| *score == best)
            .map(|(_, test)| test)
            .collect();
        let folders: BTreeSet<&str> = chosen
            .iter()
            .map(|test| test.rsplit_once('/').map_or("", |(folder, _)| folder))
            .collect();
        let tier = ["A", "B", "C"][index];
        if index == 2 {
            let tops: BTreeSet<String> = folders
                .iter()
                .map(|folder| folder.split('/').take(2).collect::<Vec<_>>().join("/"))
                .collect();
            return match tops.iter().next() {
                Some(top) if tops.len() == 1 => (
                    vec![format!("{top}/")],
                    format!(
                        "tier C, the folder named after the package ({} files)",
                        chosen.len()
                    ),
                ),
                _ => (
                    Vec::new(),
                    format!("{} folders carry the package's name: too wide", tops.len()),
                ),
            };
        }
        if chosen.len() <= MAX_FILES {
            return (
                chosen.into_iter().cloned().collect(),
                format!(
                    "tier {tier}, {} file(s)",
                    scored.iter().filter(|(s, _)| *s == best).count()
                ),
            );
        }
        if folders.len() <= MAX_FOLDERS {
            return (
                folders.iter().map(|folder| format!("{folder}/")).collect(),
                format!(
                    "tier {tier}, {} files in {} folder(s)",
                    chosen.len(),
                    folders.len()
                ),
            );
        }
        return (
            Vec::new(),
            format!(
                "{} files in {} folders: too scattered",
                chosen.len(),
                folders.len()
            ),
        );
    }
    (Vec::new(), "no test path resembles the source".into())
}

/// A run of related tests, as the project's own runner takes the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub executable: String,
    pub argv: Vec<String>,
    pub cwd: String,
}

impl Command {
    /// The command as a person would type it.
    pub fn shown(&self) -> String {
        std::iter::once(self.executable.as_str())
            .chain(self.argv.iter().map(String::as_str))
            .map(shell_words::quote)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The selection written for the runner this workspace has: a dotted label
/// below the runner's folder for a `runtests.py` (`tests/forms_tests/` is
/// `forms_tests` to Django's -- the path form loads the same modules under
/// another name and fails six of them), the path itself for pytest. `None`
/// where the workspace has no Python of its own or no runner known here.
pub fn command(root: &Path, selection: &[String]) -> Option<Command> {
    if selection.is_empty() {
        return None;
    }
    let (executable, _) = crate::test_recipe::python(root)?;
    let argv = if let Some(runner) = crate::test_recipe::runner(root) {
        let parent = Path::new(runner).parent().unwrap_or(Path::new(""));
        let mut argv = vec![runner.to_owned()];
        for path in selection {
            let relative = Path::new(path)
                .strip_prefix(parent)
                .unwrap_or(Path::new(path));
            let label = relative
                .with_extension("")
                .to_string_lossy()
                .replace('/', ".");
            argv.push(label.trim_end_matches('.').to_owned());
        }
        argv
    } else if crate::test_recipe::uses_pytest(root) {
        let mut argv: Vec<String> = ["-m", "pytest", "-q", "-rfE", "-p", "no:cacheprovider"]
            .map(String::from)
            .into();
        argv.extend(
            selection
                .iter()
                .map(|path| path.trim_end_matches('/').to_owned()),
        );
        argv
    } else {
        return None;
    };
    Some(Command {
        executable,
        argv,
        cwd: ".".into(),
    })
}

/// What a run of tests reported: how many executed and which failed, by the
/// names [`pwr_verify::failure`] gives them. `None` where the output is not
/// a report this module can read -- no test executed, a module that could
/// not be loaded, a runner whose words are not known.
pub fn read(stdout: &str, stderr: &str) -> Option<(usize, BTreeSet<String>)> {
    let executed = pwr_verify::evidence::executed_tests(&format!("{stdout}\n{stderr}"))?;
    Some((
        executed,
        pwr_verify::failure::fingerprint(stdout, stderr).identifiers,
    ))
}

/// The related tests as they stood before a file's first change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Baseline {
    /// No test to run, and why: said to nobody.
    Nothing(String),
    /// The command ran and its report was read.
    Ran {
        command: Command,
        executed: usize,
        failing: BTreeSet<String>,
    },
    /// The command was run and no report could be read from it.
    Unreadable { command: Command, why: String },
}

impl Baseline {
    pub fn record(&self, source: &str) -> serde_json::Value {
        match self {
            Self::Nothing(why) => {
                serde_json::json!({"source": source, "selected": false, "why": why})
            }
            Self::Ran {
                command,
                executed,
                failing,
            } => serde_json::json!({
                "source": source, "selected": true, "command": command.shown(),
                "executed": executed, "failing": failing,
            }),
            Self::Unreadable { command, why } => serde_json::json!({
                "source": source, "selected": true, "command": command.shown(), "unreadable": why,
            }),
        }
    }
}

/// What a change did to the related tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// What is said to the model with the edit's result.
    pub note: String,
    pub command: String,
    /// The tests that ran and all passed, when they did: a confirmation of
    /// the source as it stands.
    pub confirmed: Option<usize>,
    pub executed: Option<usize>,
    pub newly_failing: Vec<String>,
    pub now_passing: Vec<String>,
    pub still_failing: usize,
}

impl Outcome {
    pub fn record(&self, source: &str) -> serde_json::Value {
        serde_json::json!({
            "source": source, "command": self.command, "executed": self.executed,
            "confirmed": self.confirmed, "newly_failing": self.newly_failing,
            "now_passing": self.now_passing, "still_failing": self.still_failing,
        })
    }
}

fn named(names: &[String]) -> String {
    let shown: Vec<String> = names
        .iter()
        .take(NAMED)
        .map(|name| format!("`{name}`"))
        .collect();
    match names.len().saturating_sub(NAMED) {
        0 => shown.join(", "),
        more => format!("{} and {more} more", shown.join(", ")),
    }
}

/// The baseline and the report after a change, compared. Pure.
pub fn compare(
    source: &str,
    command: &Command,
    before: (usize, &BTreeSet<String>),
    after: Option<(usize, &BTreeSet<String>, bool)>,
    excerpt: &str,
) -> Outcome {
    let shown = command.shown();
    let Some((executed, failing, exit_ok)) = after else {
        return Outcome {
            note: format!(
                "PWR ran the project's tests related to `{source}` after this change (`{shown}`) \
                 and could not read a report from them: {excerpt} Nothing is confirmed by that \
                 run; before the change it executed {} tests.",
                before.0
            ),
            command: shown,
            confirmed: None,
            executed: None,
            newly_failing: Vec::new(),
            now_passing: Vec::new(),
            still_failing: 0,
        };
    };
    let newly_failing: Vec<String> = failing.difference(before.1).cloned().collect();
    let now_passing: Vec<String> = before.1.difference(failing).cloned().collect();
    let still_failing = failing.intersection(before.1).count();
    let counts = format!(
        "Before the change: {} ran, {} failing. After: {executed} ran, {} failing.",
        before.0,
        before.1.len(),
        failing.len()
    );
    let fixed = if now_passing.is_empty() {
        String::new()
    } else {
        format!(" Passing now and failing before: {}.", named(&now_passing))
    };
    let old = if still_failing == 0 {
        String::new()
    } else {
        format!(
            " {still_failing} of the failing tests failed before the change too: they are not \
             its doing."
        )
    };
    let confirmed = (exit_ok && failing.is_empty() && executed > 0).then_some(executed);
    let note = if newly_failing.is_empty() {
        format!(
            "PWR ran the project's tests related to `{source}` before and after this change: \
             `{shown}`. {counts} No test newly fails.{fixed}{old} This says the change broke none \
             of these tests; it does not by itself say the request is met."
        )
    } else {
        format!(
            "PWR ran the project's tests related to `{source}` before and after this change: \
             `{shown}`. {counts} Newly failing, passing before the change: {}.{fixed}{old} The end \
             of the report:\n{excerpt}",
            named(&newly_failing)
        )
    };
    Outcome {
        note,
        command: shown,
        confirmed,
        executed: Some(executed),
        newly_failing,
        now_passing,
        still_failing,
    }
}

/// The baselines a conversation holds, by source path.
#[derive(Debug, Default)]
pub struct CoreCheck {
    baselines: BTreeMap<String, Baseline>,
}

impl CoreCheck {
    pub fn knows(&self, source: &str) -> bool {
        self.baselines.contains_key(source)
    }

    pub fn baseline(&self, source: &str) -> Option<Baseline> {
        self.baselines.get(source).cloned()
    }

    pub fn set(&mut self, source: &str, baseline: Baseline) {
        self.baselines.insert(source.to_owned(), baseline);
    }
}

struct Ran {
    stdout: String,
    stderr: String,
    exit_ok: bool,
    truncated: bool,
}

async fn run(policy: &ToolPolicy, command: &Command) -> Result<Ran, String> {
    let mut bounded = policy.clone();
    bounded.timeout = bounded.timeout.min(LIMIT);
    bounded.output_limit = bounded.output_limit.max(OUTPUT);
    let result = pwr_tools::run_command_in(
        &bounded,
        &command.executable,
        &command.argv,
        None,
        Some(&command.cwd),
    )
    .await
    .map_err(|problem| problem.to_string())?;
    Ok(Ran {
        exit_ok: result.exit_code == Some(0),
        truncated: result.stdout_truncated || result.stderr_truncated,
        stdout: result.stdout,
        stderr: result.stderr,
    })
}

fn end_of(text: &str, limit: usize) -> String {
    let text = text.trim_end();
    let mut start = text.len().saturating_sub(limit);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

/// The related tests before the first change to `source`.
pub async fn before(policy: &ToolPolicy, source: &str) -> Baseline {
    let root = policy.root.clone();
    let asked = source.to_owned();
    let found = tokio::task::spawn_blocking(move || {
        let (selection, why) = related_tests(&root, &asked);
        (command(&root, &selection), selection.is_empty(), why)
    })
    .await;
    let command = match found {
        Ok((Some(command), _, _)) => command,
        Ok((None, true, why)) => return Baseline::Nothing(why),
        Ok((None, false, _)) => {
            return Baseline::Nothing(
                "no Python of the workspace's own, or no runner known".into(),
            );
        }
        Err(problem) => return Baseline::Nothing(problem.to_string()),
    };
    match run(policy, &command).await {
        Ok(ran) => match read(&ran.stdout, &ran.stderr) {
            Some((executed, failing)) if !ran.truncated || ran.exit_ok => Baseline::Ran {
                command,
                executed,
                failing,
            },
            Some(_) => Baseline::Unreadable {
                command,
                why: "the report was longer than what is kept".into(),
            },
            None => Baseline::Unreadable {
                command,
                why: "no test report in the output".into(),
            },
        },
        Err(why) => Baseline::Unreadable { command, why },
    }
}

/// The related tests after a change to `source`, against their baseline.
/// `None` where there was nothing to compare with.
pub async fn after(policy: &ToolPolicy, source: &str, baseline: &Baseline) -> Option<Outcome> {
    let Baseline::Ran {
        command,
        executed,
        failing,
    } = baseline
    else {
        return None;
    };
    let before = (*executed, failing);
    Some(match run(policy, command).await {
        Ok(ran) => {
            let excerpt = end_of(&format!("{}\n{}", ran.stdout, ran.stderr), 1500);
            match read(&ran.stdout, &ran.stderr) {
                Some((executed, failing)) if !ran.truncated || ran.exit_ok => compare(
                    source,
                    command,
                    before,
                    Some((executed, &failing, ran.exit_ok)),
                    &excerpt,
                ),
                _ => compare(source, command, before, None, &end_of(&excerpt, 400)),
            }
        }
        Err(why) => compare(source, command, before, None, &why),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "\n").unwrap();
        }
        dir
    }

    fn django() -> tempfile::TempDir {
        let mut files: Vec<String> = [
            "django/forms/fields.py",
            "django/utils/http.py",
            "django/db/models/deletion.py",
            "tests/runtests.py",
            "tests/utils_tests/test_http.py",
            "tests/utils_tests/test_text.py",
            "tests/forms_tests/tests/test_forms.py",
            "tests/model_forms/test_uuid.py",
            "tests/model_fields/test_field_flags.py",
            "tests/gis_tests/test_fields.py",
            "tests/contenttypes_tests/test_fields.py",
            "tests/invalid_models_tests/test_custom_fields.py",
            "tests/delete/tests.py",
            "tests/model_inheritance/tests.py",
            "tests/aggregation/tests.py",
        ]
        .map(String::from)
        .into();
        files
            .extend((0..12).map(|n| format!("tests/forms_tests/field_tests/test_kind{n}field.py")));
        tree(&files.iter().map(String::as_str).collect::<Vec<_>>())
    }

    // Each selection below is what `passo0/regola.py` chose on the real
    // repositories in the replay of 2026-10-13.
    #[test]
    fn a_file_is_matched_by_its_package_and_a_folder_named_after_it() {
        let dir = django();
        let (chosen, why) = related_tests(dir.path(), "django/forms/fields.py");
        assert_eq!(chosen, ["tests/forms_tests/field_tests/"], "{why}");
        assert!(why.starts_with("tier A"), "{why}");
    }

    #[test]
    fn a_name_shared_by_other_packages_tests_is_not_taken_for_this_file() {
        let dir = django();
        let (chosen, _) = related_tests(dir.path(), "django/forms/fields.py");
        assert!(
            chosen.iter().all(|path| !path.contains("model_fields")),
            "{chosen:?}"
        );
        assert!(
            chosen.iter().all(|path| !path.contains("gis_tests")),
            "{chosen:?}"
        );
    }

    #[test]
    fn a_file_is_matched_by_a_test_file_of_its_name_in_its_package() {
        let dir = django();
        let (chosen, why) = related_tests(dir.path(), "django/utils/http.py");
        assert_eq!(chosen, ["tests/utils_tests/test_http.py"], "{why}");
    }

    #[test]
    fn a_flat_test_tree_is_matched_by_the_file_name_alone() {
        let dir = tree(&[
            "sphinx/builders/gettext.py",
            "tests/test_build_gettext.py",
            "tests/test_builder.py",
            "tests/test_intl.py",
            "tests/roots/test-gettext-template/conf.py",
        ]);
        let (chosen, why) = related_tests(dir.path(), "sphinx/builders/gettext.py");
        assert_eq!(chosen, ["tests/test_build_gettext.py"], "{why}");
        assert!(why.starts_with("tier B"), "{why}");
    }

    #[test]
    fn a_package_that_names_many_test_folders_selects_nothing() {
        let dir = django();
        let (chosen, why) = related_tests(dir.path(), "django/db/models/deletion.py");
        assert!(chosen.is_empty(), "{chosen:?}");
        assert!(why.contains("too wide"), "{why}");
    }

    #[test]
    fn nothing_resembling_the_file_selects_nothing() {
        let dir = tree(&["app/billing.py", "tests/test_users.py"]);
        assert!(related_tests(dir.path(), "app/billing.py").0.is_empty());
        assert!(related_tests(dir.path(), "app/__init__.py").0.is_empty());
    }

    fn with_python(dir: &tempfile::TempDir, script: &str) -> ToolPolicy {
        let root = dir.path();
        let python = root.join(".toolchains/fixture/bin/python");
        fs::create_dir_all(python.parent().unwrap()).unwrap();
        fs::write(&python, format!("#!/bin/sh\n{script}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
        }
        ToolPolicy {
            root: root.to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![python.to_string_lossy().into_owned(), "python".into()],
            output_limit: 4096,
            timeout: Duration::from_secs(5),
            sandbox: pwr_tools::SandboxPolicy::Disabled,
            approvals: Vec::new(),
        }
    }

    #[test]
    fn the_selection_is_written_as_this_runner_takes_it() {
        let dir = django();
        with_python(&dir, "true");
        let folder = command(dir.path(), &["tests/forms_tests/field_tests/".into()]).unwrap();
        assert_eq!(
            folder.argv,
            ["tests/runtests.py", "forms_tests.field_tests"]
        );
        let file = command(dir.path(), &["tests/utils_tests/test_http.py".into()]).unwrap();
        assert_eq!(file.argv, ["tests/runtests.py", "utils_tests.test_http"]);
        assert!(command(dir.path(), &[]).is_none());
    }

    #[test]
    fn pytest_takes_the_path_and_no_python_of_the_workspace_takes_nothing() {
        let dir = tree(&[
            "pkg/gettext.py",
            "tests/test_build_gettext.py",
            "pytest.ini",
        ]);
        assert!(command(dir.path(), &["tests/test_build_gettext.py".into()]).is_none());
        with_python(&dir, "true");
        let made = command(dir.path(), &["tests/test_build_gettext.py".into()]).unwrap();
        assert_eq!(made.argv.last().unwrap(), "tests/test_build_gettext.py");
        assert!(made.argv.starts_with(&["-m".into(), "pytest".into()]));
    }

    const TWO_BROKEN: &str = "======\nFAIL: test_parsing_rfc850 (utils_tests.test_http.HttpDateProcessingTests)\n------\nAssertionError\n\n======\nFAIL: test_parsing_year_less_than_70 (utils_tests.test_http.HttpDateProcessingTests)\n------\nAssertionError\n\n------\nRan 45 tests in 0.004s\n\nFAILED (failures=2)\n";

    #[test]
    fn a_report_is_read_for_how_many_ran_and_which_failed() {
        assert_eq!(
            read("", "Ran 45 tests in 0.004s\n\nOK\n"),
            Some((45, BTreeSet::new()))
        );
        let (executed, failing) = read("", TWO_BROKEN).unwrap();
        assert_eq!((executed, failing.len()), (45, 2), "{failing:?}");
        assert_eq!(read("", "Ran 0 tests in 0.000s\n\nOK\n"), None);
        assert_eq!(read("nothing of the kind", ""), None);
    }

    fn sample() -> Command {
        Command {
            executable: "python".into(),
            argv: vec!["tests/runtests.py".into(), "utils_tests.test_http".into()],
            cwd: ".".into(),
        }
    }

    #[test]
    fn tests_all_passing_before_and_after_confirm_the_source_and_claim_no_more() {
        let none = BTreeSet::new();
        let said = compare(
            "django/utils/http.py",
            &sample(),
            (45, &none),
            Some((45, &none, true)),
            "",
        );
        assert_eq!(said.confirmed, Some(45));
        assert!(said.note.contains("No test newly fails"), "{}", said.note);
        assert!(
            said.note
                .contains("does not by itself say the request is met"),
            "{}",
            said.note
        );
        assert!(
            said.note
                .contains("python tests/runtests.py utils_tests.test_http"),
            "{}",
            said.note
        );
    }

    #[test]
    fn a_test_the_change_broke_is_named_and_nothing_is_confirmed() {
        let none = BTreeSet::new();
        let (executed, failing) = read("", TWO_BROKEN).unwrap();
        let said = compare(
            "django/utils/http.py",
            &sample(),
            (45, &none),
            Some((executed, &failing, false)),
            "AssertionError",
        );
        assert_eq!(said.confirmed, None);
        assert_eq!(said.newly_failing.len(), 2);
        assert!(said.note.contains("Newly failing"), "{}", said.note);
        assert!(said.note.contains("test_parsing_rfc850"), "{}", said.note);
    }

    #[test]
    fn a_test_failing_before_the_change_is_not_laid_at_its_door() {
        let old: BTreeSet<String> = ["pkg.Case.test_old".to_owned()].into();
        let said = compare(
            "pkg/a.py",
            &sample(),
            (10, &old),
            Some((10, &old, false)),
            "",
        );
        assert!(said.newly_failing.is_empty());
        assert_eq!(said.still_failing, 1);
        assert!(
            said.note.contains("failed before the change too"),
            "{}",
            said.note
        );
        // Tests that do not all pass are not a confirmation, whoever broke them.
        assert_eq!(said.confirmed, None);
    }

    #[test]
    fn a_run_with_no_report_after_the_change_confirms_nothing() {
        let none = BTreeSet::new();
        let said = compare("pkg/a.py", &sample(), (10, &none), None, "timed out");
        assert_eq!(said.confirmed, None);
        assert!(
            said.note.contains("could not read a report"),
            "{}",
            said.note
        );
    }

    #[tokio::test]
    async fn the_core_runs_the_related_tests_before_and_after_a_change() {
        let dir = django();
        // A runner that passes until the source says otherwise.
        let policy = with_python(
            &dir,
            "if grep -q broken django/utils/http.py; then printf 'FAIL: test_x (utils_tests.test_http.T)\\n\\nRan 45 tests in 0.01s\\n\\nFAILED (failures=1)\\n' >&2; exit 1; fi\nprintf 'Ran 45 tests in 0.01s\\n\\nOK\\n' >&2",
        );
        let baseline = before(&policy, "django/utils/http.py").await;
        let Baseline::Ran {
            executed, failing, ..
        } = &baseline
        else {
            panic!("{baseline:?}");
        };
        assert_eq!((*executed, failing.len()), (45, 0));
        let clean = after(&policy, "django/utils/http.py", &baseline)
            .await
            .unwrap();
        assert_eq!(clean.confirmed, Some(45));
        fs::write(dir.path().join("django/utils/http.py"), "broken\n").unwrap();
        let broken = after(&policy, "django/utils/http.py", &baseline)
            .await
            .unwrap();
        assert_eq!(broken.confirmed, None);
        assert_eq!(broken.newly_failing.len(), 1, "{}", broken.note);
    }

    #[tokio::test]
    async fn a_file_with_no_related_tests_has_no_baseline_and_no_outcome() {
        let dir = django();
        let policy = with_python(&dir, "exit 0");
        let baseline = before(&policy, "django/db/models/deletion.py").await;
        assert!(matches!(baseline, Baseline::Nothing(_)), "{baseline:?}");
        assert!(
            after(&policy, "django/db/models/deletion.py", &baseline)
                .await
                .is_none()
        );
    }

    /// The rule and the runner's form on the real checkouts of the replay,
    /// and the tests really run there: by hand, with
    /// `PWR_CORE_CHECK_CHECKOUTS` naming the folder that holds them.
    #[tokio::test]
    #[ignore = "needs the prepared checkouts of the replay"]
    async fn the_rule_agrees_with_the_replay_on_real_checkouts() {
        let checkouts =
            std::path::PathBuf::from(std::env::var("PWR_CORE_CHECK_CHECKOUTS").unwrap());
        for (checkout, source, label, tests) in [
            (
                "django__django-11848",
                "django/utils/http.py",
                "utils_tests.test_http",
                45,
            ),
            (
                "django__django-11880",
                "django/forms/fields.py",
                "forms_tests.field_tests",
                188,
            ),
            (
                "sphinx-doc__sphinx-10466",
                "sphinx/builders/gettext.py",
                "tests/test_build_gettext.py",
                7,
            ),
        ] {
            let root = checkouts.join(checkout);
            let (selection, why) = related_tests(&root, source);
            let made = command(&root, &selection).unwrap_or_else(|| panic!("{checkout}: {why}"));
            assert_eq!(made.argv.last().unwrap(), label, "{checkout}: {why}");
            let policy = ToolPolicy {
                root: root.clone(),
                extra_readable: Vec::new(),
                protected: Vec::new(),
                allow_commands: vec![made.executable.clone()],
                output_limit: 4096,
                timeout: Duration::from_secs(120),
                sandbox: pwr_tools::SandboxPolicy::Disabled,
                approvals: Vec::new(),
            };
            let baseline = before(&policy, source).await;
            let Baseline::Ran {
                executed, failing, ..
            } = &baseline
            else {
                panic!("{checkout}: {baseline:?}");
            };
            assert_eq!((*executed, failing.len()), (tests, 0), "{checkout}");
            let same = after(&policy, source, &baseline).await.unwrap();
            assert_eq!(same.confirmed, Some(tests), "{checkout}: {}", same.note);
        }
    }

    #[test]
    fn only_source_files_are_checked() {
        assert!(is_source("django/forms/fields.py"));
        assert!(!is_source("tests/forms_tests/tests/test_forms.py"));
        assert!(!is_source("docs/index.rst"));
    }
}
