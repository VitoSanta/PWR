//! What to say when a call for the project's tests executes none of them.
//!
//! Measured 2026-10-09 (33 runs, Qwen 3.5 9B, three SWE-bench instances): in
//! eight of them the model never got the project's tests to run -- the one
//! such run with H0 on took thirty minutes, the other eight a mean of nine.
//! Of 183 calls that executed nothing, a third named a label or module the
//! runner does not know (`forms` for `forms_tests`, `utils.http` for
//! `utils_tests.test_http`), a fifth a file not there from the folder the
//! command ran in, and one in twelve ran zero tests and answered `OK`.
//! Calling again a command that had already worked cost little: one to four
//! percent of the time.
//!
//! So the result of such a call says what it was -- no test executed, and
//! which of those it is -- and gives what the next call needs: the test
//! paths that exist and resemble the name asked for, where a path is, and
//! the command that did run the project's tests earlier in the session, if
//! one did. Only then: a call that runs tests is told nothing.
//!
//! General rules, no project in them. Where the reason is not recognised,
//! nothing is said.

use std::collections::BTreeSet;
use std::path::Path;

/// A command that executed tests of the project in this session.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Worked {
    command: String,
    folder: Option<String>,
    tests: usize,
}

/// What this session knows about running the project's tests: the last
/// command that did, and the names it has already been given paths for.
#[derive(Debug, Default)]
pub struct TestCalls {
    worked: Option<Worked>,
    suggested_for: BTreeSet<String>,
}

/// Folders a search for test paths does not enter.
const NOT_SEARCHED: &[&str] = &[
    "node_modules",
    "target",
    "build",
    "dist",
    "venv",
    "__pycache__",
];
const SEARCH_DEPTH: usize = 4;
const SEARCH_ENTRIES: usize = 6_000;
const SUGGESTIONS: usize = 3;

impl TestCalls {
    /// A call executed tests of the project.
    pub fn worked(&mut self, command: &str, folder: Option<&str>, tests: usize) {
        self.worked = Some(Worked {
            command: command.to_owned(),
            folder: folder
                .filter(|folder| !folder.is_empty() && *folder != ".")
                .map(str::to_owned),
            tests,
        });
    }

    /// A call for tests ran to its end and executed none. What to say with
    /// its result, or `None` where nothing is known that would help the next
    /// call.
    ///
    /// `own` is a call that names a file the session wrote. A test file of
    /// the session's that the runner does not pick up is the same trouble as
    /// a label it does not know, and is told the same; but a script of the
    /// session's that prints what it likes is not a test run that failed to
    /// start, so with no reason recognised nothing is said of it.
    pub fn ran_nothing(
        &mut self,
        root: &Path,
        arguments: &serde_json::Value,
        output: &str,
        own: bool,
    ) -> Option<String> {
        // The folder the command ran in, from the root; the root itself is none.
        let folder = arguments["cwd"]
            .as_str()
            .map(|folder| {
                Path::new(folder)
                    .strip_prefix(root)
                    .map_or(folder.to_owned(), |inside| {
                        inside.to_string_lossy().into_owned()
                    })
            })
            .map(|folder| folder.trim_end_matches('/').to_owned())
            .filter(|folder| !folder.is_empty() && folder != ".");
        let folder = folder.as_deref();
        let words = words(arguments);
        let mut said: Vec<String> = Vec::new();
        let listed = |found: Vec<String>| {
            found
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let unknown = unknown_selector(output, &words);
        if matched_no_test(output) {
            said.push("The selection matched no test.".to_owned());
            // A name that is a module but holds no test: `django.forms`.
            if unknown.is_none()
                && let Some((name, found)) = selectors(root, folder, arguments)
                    .into_iter()
                    .filter(|name| !self.suggested_for.contains(*name))
                    .find_map(|name| {
                        let found = test_paths_like(root, name);
                        (!found.is_empty()).then_some((name, found))
                    })
            {
                self.suggested_for.insert(name.to_owned());
                said.push(format!(
                    "Test paths that exist here and resemble `{name}`: {}.",
                    listed(found)
                ));
            }
        }
        if let Some((name, written)) = unknown {
            said.push(format!(
                "The runner does not know `{name}`, the name this command gave it."
            ));
            if self.suggested_for.insert(name.clone()) {
                let found = test_paths_like(root, &written);
                if !found.is_empty() {
                    said.push(format!(
                        "Test paths that exist here and resemble it: {}.",
                        listed(found)
                    ));
                }
            }
        }
        if let Some(path) = file_not_found(output)
            && let Some(place) = where_it_is(root, folder, &path)
        {
            said.push(place);
        }
        if own && said.is_empty() {
            return None;
        }
        let Some(worked) = &self.worked else {
            return (!said.is_empty()).then(|| format!("{NOTHING_RAN} {}", said.join(" ")));
        };
        // With no reason recognised, only what the output shows is said.
        let opening = if said.is_empty() {
            NOTHING_SHOWN
        } else {
            NOTHING_RAN
        };
        said.push(format!(
            "Earlier in this session `{}`{} executed {} of the project's tests.",
            worked.command,
            worked
                .folder
                .as_ref()
                .map(|folder| format!(", run in `{folder}`,"))
                .unwrap_or_default(),
            worked.tests
        ));
        Some(format!("{opening} {}", said.join(" ")))
    }
}

const NOTHING_RAN: &str = "No test was executed by this call.";
const NOTHING_SHOWN: &str = "This call's output shows no test having run.";

/// Programs that are test runners by name.
const RUNNERS: &[&str] = &[
    "pytest", "py.test", "tox", "nox", "jest", "vitest", "mocha", "rspec", "ctest", "phpunit",
];
/// Programs that run what they are given: interpreters, package managers,
/// build tools. One of these is a call for tests when a word of it says so.
const LAUNCHERS: &[&str] = &[
    "python", "pypy", "node", "deno", "bun", "ruby", "php", "perl", "bash", "sh", "zsh", "npm",
    "npx", "yarn", "pnpm", "cargo", "go", "make", "gradle", "gradlew", "mvn", "mvnw", "dotnet",
    "swift", "uv", "poetry", "pipenv", "hatch", "pdm", "bundle", "rake", "composer", "mix",
];

/// The command's words after its program, under any of the names a model
/// gives the list, and without the program said again at their front.
fn words(arguments: &serde_json::Value) -> Vec<&str> {
    let executable = arguments["executable"].as_str().unwrap_or_default();
    let words: Vec<&str> = ["args", "argv", "arg_list", "arguments"]
        .iter()
        .find_map(|name| arguments[*name].as_array())
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect();
    match words.first() {
        Some(first) if !executable.is_empty() && *first == executable => words[1..].to_vec(),
        _ => words,
    }
}

/// The command as it was asked, on one line.
pub fn command_line(arguments: &serde_json::Value) -> String {
    let mut line = arguments["executable"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    for word in words(arguments) {
        line.push(' ');
        line.push_str(word);
    }
    line
}

/// The words of a command that select tests: not its options nor what
/// follows one, and not a path that exists (the runner's own script, a
/// folder of tests).
fn selectors<'a>(
    root: &Path,
    folder: Option<&str>,
    arguments: &'a serde_json::Value,
) -> Vec<&'a str> {
    let words = words(arguments);
    let exists = |word: &str| {
        root.join(word).exists()
            || folder.is_some_and(|folder| root.join(folder).join(word).exists())
    };
    words
        .iter()
        .enumerate()
        .filter(|(at, word)| {
            !word.starts_with('-')
                && !(*at > 0 && words[at - 1].starts_with('-') && !words[at - 1].contains('='))
                && !word.contains(char::is_whitespace)
                && !exists(word.split("::").next().unwrap_or(word))
        })
        .map(|(_, word)| *word)
        .collect()
}

/// Whether a command is a call for tests, by its own words: a test runner,
/// or a program that runs what it is given with a word that names tests.
/// Installing something is not one, whatever it installs.
pub fn is_test_call(arguments: &serde_json::Value) -> bool {
    let executable = arguments["executable"].as_str().unwrap_or_default();
    let program = executable.rsplit('/').next().unwrap_or(executable);
    let words = words(arguments);
    // Nor is asking a runner how it is used.
    if words.iter().any(|word| {
        matches!(
            *word,
            "install" | "add" | "uninstall" | "-h" | "--help" | "--version"
        )
    }) {
        return false;
    }
    if RUNNERS.contains(&program) {
        return true;
    }
    // `python3.11` is `python`.
    let family = program.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == '.');
    LAUNCHERS.contains(&family)
        && words.iter().any(|word| {
            // A shell is given its command in one word.
            word.split_whitespace().any(|part| {
                let name = part.rsplit('/').next().unwrap_or(part).to_lowercase();
                name.contains("test")
                    || name.starts_with("spec")
                    || RUNNERS.contains(&name.as_str())
            })
        })
}

/// Whether a runner that ran to its end says its selection held no test.
fn matched_no_test(output: &str) -> bool {
    output.lines().map(str::trim).any(|line| {
        line.starts_with("Ran 0 tests in ")
            || line.contains("no tests ran in ")
            || line.starts_with("test result: ok. 0 passed; 0 failed; 0 ignored")
    })
}

/// The label, module or file a test runner says it cannot find, with the
/// word of the command it comes from -- and only where the command gave it
/// that name: an import that fails inside the project is the project's, and
/// nothing is said of it.
fn unknown_selector(output: &str, words: &[&str]) -> Option<(String, String)> {
    let given = |name: &str| {
        words.iter().find_map(|word| {
            word.strip_prefix(name)
                .filter(|rest| rest.is_empty() || rest.starts_with(['.', ':', '/']))
                .map(|_| (*word).to_owned())
        })
    };
    for line in output.lines().map(str::trim) {
        let named = if let Some(rest) = line
            .strip_prefix("ModuleNotFoundError: No module named ")
            .or_else(|| line.strip_prefix("ImportError: No module named "))
            // unittest and Django's runner: a label that is not a module.
            .or_else(|| line.strip_prefix("ImportError: Failed to import test module: "))
        {
            clean(rest)
        } else if let Some(rest) = line
            // pytest: a path that is not there.
            .strip_prefix("ERROR: file or directory not found: ")
            .or_else(|| line.strip_prefix("ERROR: not found: "))
        {
            clean(rest.split("::").next().unwrap_or(rest))
        } else {
            None
        };
        if let Some(name) = named
            && let Some(written) = given(&name)
        {
            return Some((name, written));
        }
    }
    None
}

fn clean(name: &str) -> Option<String> {
    let name = name
        .trim()
        .trim_matches(['\'', '"', '`'])
        .split_whitespace()
        .next()?
        .to_owned();
    (!name.is_empty() && name.len() <= 160).then_some(name)
}

/// The file a program could not open, as the program wrote it.
fn file_not_found(output: &str) -> Option<String> {
    for line in output.lines().map(str::trim) {
        if let Some(at) = line.find("can't open file ") {
            let rest = &line[at + "can't open file ".len()..];
            let quote = rest.chars().next().filter(|ch| matches!(ch, '\'' | '"'))?;
            return rest[1..].split(quote).next().map(str::to_owned);
        }
    }
    None
}

/// Where a file that was not found is: at the root when the command ran in
/// a folder, in the folder when it ran at the root, or under another name's
/// folder nearby. `None` where it is nowhere to be seen.
fn where_it_is(root: &Path, folder: Option<&str>, asked: &str) -> Option<String> {
    let asked_path = Path::new(asked);
    let relative = asked_path.strip_prefix(root).unwrap_or(asked_path);
    if relative.is_absolute() {
        return None;
    }
    let shown = relative.to_string_lossy();
    if let Some(folder) = folder {
        // Written from the root, run in a folder.
        if root.join(relative).is_file() && !root.join(folder).join(relative).is_file() {
            return Some(format!(
                "`{shown}` exists from the workspace root; this command ran in `{folder}`, where \
                 that path does not exist."
            ));
        }
    }
    let name = relative.file_name()?.to_string_lossy().into_owned();
    let found = paths_named(root, |candidate| candidate == name)
        .into_iter()
        .find(|path| path != shown.as_ref())?;
    Some(format!(
        "`{shown}` is not there from {}; a file of that name is at `{found}`.",
        folder.map_or_else(
            || "the workspace root".to_owned(),
            |folder| format!("`{folder}`")
        )
    ))
}

/// What a part of a name is about, without what marks it as a test:
/// `forms_tests` and `test_forms` are about `forms`.
fn stem_of(part: &str) -> String {
    let part = part.to_lowercase();
    part.trim_start_matches("test_")
        .trim_end_matches("_tests")
        .trim_end_matches("_test")
        .to_owned()
}

/// Test paths in the workspace that resemble a name written for the runner:
/// folders of tests and test files whose own name holds a part of it.
///
/// Nearest first. A path is nearer the more parts of the name its folders
/// and file are about -- for `utils.http`, `tests/utils_tests/test_http.py`
/// -- then the earlier the part, then a name that is the part itself marked
/// as tests (`forms` finds `tests/forms_tests/` before `tests/model_forms/`),
/// then folders before files. A file counts by its own name only:
/// `tests/i18n/forms.py` is something tests use, not tests. The project's
/// own top folder in a name (`django.forms`) says nothing of which tests,
/// and what lies inside a folder already listed is not listed again.
fn test_paths_like(root: &Path, written: &str) -> Vec<String> {
    let parts: Vec<&str> = written
        .split(['.', '/', ':'])
        .filter(|part| !part.is_empty())
        .collect();
    let mut stems: Vec<String> = Vec::new();
    for part in &parts {
        let stem = stem_of(part);
        if stem.len() >= 3
            && !matches!(stem.as_str(), "test" | "tests")
            && !(parts.len() > 1 && root.join(part).is_dir())
            && !stems.contains(&stem)
        {
            stems.push(stem);
        }
    }
    if stems.is_empty() {
        return Vec::new();
    }
    let bare_of = |name: &str| name.split('.').next().unwrap_or(name).to_lowercase();
    let last_of = |path: &str| {
        bare_of(
            path.trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(path),
        )
    };
    // Whether a name is a part itself, marked as tests or not.
    let is_about = |bare: &str, stem: &str| {
        bare == stem
            || bare
                .strip_suffix(stem)
                .is_some_and(|mark| matches!(mark, "test_" | "tests_"))
            || bare
                .strip_prefix(stem)
                .is_some_and(|mark| matches!(mark, "_test" | "_tests"))
    };
    let mut found: Vec<String> = paths_named(root, |name| {
        let bare = bare_of(name);
        stems.iter().any(|stem| bare.contains(stem.as_str()))
    })
    .into_iter()
    .filter(|path| {
        if path.ends_with('/') {
            is_test_folder(path)
        } else {
            crate::confirmation::is_test_path(path.rsplit('/').next().unwrap_or(path))
        }
    })
    .filter(|path| path.trim_end_matches('/') != written.trim_end_matches('/'))
    .collect();
    found.sort_by_key(|path| {
        let last = last_of(path);
        let about = stems
            .iter()
            .filter(|stem| {
                path.split('/')
                    .any(|name| is_about(&bare_of(name), stem.as_str()))
            })
            .count();
        let part = stems
            .iter()
            .position(|stem| is_about(&last, stem))
            .or_else(|| stems.iter().position(|stem| last.contains(stem.as_str())))
            .unwrap_or(stems.len());
        let exact = stems.get(part).is_some_and(|stem| is_about(&last, stem));
        (
            std::cmp::Reverse(about),
            part,
            !exact,
            !path.ends_with('/'),
            last.len(),
            path.len(),
        )
    });
    let mut listed: Vec<String> = Vec::new();
    for path in found {
        if listed.len() == SUGGESTIONS {
            break;
        }
        if !listed
            .iter()
            .any(|folder| folder.ends_with('/') && path.starts_with(folder.as_str()))
        {
            listed.push(path);
        }
    }
    listed
}

fn is_test_folder(path: &str) -> bool {
    path.ends_with('/')
        && path
            .trim_end_matches('/')
            .split('/')
            .any(|part| matches!(part, "tests" | "test" | "__tests__" | "spec"))
}

/// Paths in the workspace, folders with a trailing `/`, whose last name
/// satisfies `wanted`: a bounded walk that stays out of hidden folders and of
/// what build tools make.
fn paths_named(root: &Path, wanted: impl Fn(&str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = 0usize;
    let mut level = vec![(root.to_path_buf(), String::new())];
    for _ in 0..SEARCH_DEPTH {
        let mut next = Vec::new();
        for (folder, prefix) in level {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                seen += 1;
                if seen > SEARCH_ENTRIES {
                    return found;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || NOT_SEARCHED.contains(&name.as_str()) {
                    continue;
                }
                let path = format!("{prefix}{name}");
                let is_folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
                if wanted(&name) {
                    found.push(if is_folder {
                        format!("{path}/")
                    } else {
                        path.clone()
                    });
                }
                if is_folder {
                    next.push((entry.path(), format!("{path}/")));
                }
            }
        }
        level = next;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A repository laid out as Django's: the source in one folder, its
    /// tests in another under names of their own.
    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for folder in [
            "django/forms",
            "django/utils",
            "tests/forms_tests/tests",
            "tests/forms_tests/field_tests",
            "tests/model_forms",
            "tests/utils_tests",
            "tests/admin_views",
            "tests/i18n",
            "node_modules/forms",
            ".toolchains/python/lib/forms",
        ] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        for file in [
            "django/forms/fields.py",
            "django/utils/http.py",
            "tests/runtests.py",
            "tests/i18n/forms.py",
            "tests/forms_tests/tests/test_forms.py",
            "tests/utils_tests/test_http.py",
        ] {
            std::fs::write(dir.path().join(file), "").unwrap();
        }
        dir
    }

    const UNKNOWN_LABEL: &str = "E\n======\nERROR: forms (unittest.loader._FailedTest)\n------\n\
        ImportError: Failed to import test module: forms\nTraceback (most recent call last):\n\
        ModuleNotFoundError: No module named 'forms'\n\n------\nRan 1 test in 0.000s\n\n\
        FAILED (errors=1)\n";

    fn forms() -> serde_json::Value {
        json!({"executable": "python", "args": ["tests/runtests.py", "forms"]})
    }

    #[test]
    fn a_label_the_runner_does_not_know_is_answered_with_the_test_paths_that_exist() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        let said = calls
            .ran_nothing(dir.path(), &forms(), UNKNOWN_LABEL, false)
            .unwrap();
        assert!(said.starts_with(NOTHING_RAN), "{said}");
        assert!(
            said.contains("The runner does not know `forms`, the name this command gave it."),
            "{said}"
        );
        // The tests' own names for it, the nearest first; not the source
        // folder, not a module the tests use, not what build tools hold.
        assert!(
            said.contains("resemble it: `tests/forms_tests/`, `tests/model_forms/`."),
            "{said}"
        );
        assert!(!said.contains("django/forms"), "{said}");
        assert!(!said.contains("i18n"), "{said}");
        assert!(
            !said.contains("node_modules") && !said.contains(".toolchains"),
            "{said}"
        );
        // Asked the same name again: told again what it was, not the list.
        let again = calls
            .ran_nothing(dir.path(), &forms(), UNKNOWN_LABEL, false)
            .unwrap();
        assert!(again.contains("does not know `forms`"), "{again}");
        assert!(!again.contains("resemble it"), "{again}");
    }

    #[test]
    fn a_dotted_label_is_matched_by_the_part_the_runner_names() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        let call = json!({"executable": "python",
            "args": ["runtests.py", "utils.http.HttpDateTests"], "cwd": "tests"});
        let output = "ModuleNotFoundError: No module named 'utils'\nRan 1 test in 0.000s\n\n\
                      FAILED (errors=1)\n";
        let said = calls.ran_nothing(dir.path(), &call, output, false).unwrap();
        assert!(said.contains("does not know `utils`"), "{said}");
        // The file about what was written after it, then the folder.
        assert!(
            said.contains("resemble it: `tests/utils_tests/test_http.py`, `tests/utils_tests/`."),
            "{said}"
        );
    }

    #[test]
    fn an_import_that_fails_inside_the_project_is_not_a_name_the_command_gave() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        // The label is right; the change under test broke an import. That is
        // the model's to read in the output, and nothing here speaks over it.
        let call = json!({"executable": "python", "args": ["tests/runtests.py", "forms_tests"]});
        let output = "ImportError: Failed to import test module: forms_tests.tests.test_forms\n\
                      ModuleNotFoundError: No module named 'django.utils.htpp'\n\
                      Ran 1 test in 0.000s\n\nFAILED (errors=1)\n";
        assert_eq!(calls.ran_nothing(dir.path(), &call, output, false), None);
    }

    #[test]
    fn a_file_not_there_from_the_folder_the_command_ran_in_is_placed() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        // Written from the root, run in `tests`.
        let call = json!({"executable": "python3", "args": ["tests/runtests.py"], "cwd": "tests"});
        let output =
            "python3: can't open file 'tests/runtests.py': [Errno 2] No such file or directory\n";
        let said = calls.ran_nothing(dir.path(), &call, output, false).unwrap();
        assert!(
            said.contains(
                "`tests/runtests.py` exists from the workspace root; this command ran in `tests`"
            ),
            "{said}"
        );
        // Written for `tests`, run at the root.
        let call = json!({"executable": "python", "args": ["runtests.py"]});
        let output = "python: can't open file 'runtests.py': [Errno 2] No such file or directory\n";
        let said = calls.ran_nothing(dir.path(), &call, output, false).unwrap();
        assert!(
            said.contains(
                "`runtests.py` is not there from the workspace root; a file of that name is at \
                 `tests/runtests.py`"
            ),
            "{said}"
        );
        // The interpreter writes the whole path: it is read from the root.
        let whole = format!(
            "python: can't open file '{}': [Errno 2] No such file or directory\n",
            dir.path().join("runtests.py").display()
        );
        let said = calls.ran_nothing(dir.path(), &call, &whole, false).unwrap();
        assert!(said.contains("at `tests/runtests.py`"), "{said}");
    }

    #[test]
    fn zero_tests_is_said_for_what_it_is() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        let all = json!({"executable": "python", "args": ["tests/runtests.py", "-k", "nothing"]});
        let said = calls
            .ran_nothing(dir.path(), &all, "Ran 0 tests in 0.000s\n\nOK\n", false)
            .unwrap();
        assert_eq!(
            said,
            "No test was executed by this call. The selection matched no test."
        );
        let said = calls
            .ran_nothing(dir.path(), &all, "==== no tests ran in 0.01s ====\n", false)
            .unwrap();
        assert!(said.ends_with("The selection matched no test."), "{said}");
        // A name that is a module of the source and holds no test: where the
        // tests about it are, once. The program said twice is one program.
        let source = json!({"executable": "python",
            "args": ["python", "tests/runtests.py", "django.forms", "--settings", "test_sqlite"]});
        let said = calls
            .ran_nothing(dir.path(), &source, "Ran 0 tests in 0.000s\n\nOK\n", false)
            .unwrap();
        assert!(
            said.ends_with(
                "The selection matched no test. Test paths that exist here and resemble \
                 `django.forms`: `tests/forms_tests/`, `tests/model_forms/`."
            ),
            "{said}"
        );
        let again = calls
            .ran_nothing(dir.path(), &source, "Ran 0 tests in 0.000s\n\nOK\n", false)
            .unwrap();
        assert!(again.ends_with("The selection matched no test."), "{again}");
        assert_eq!(
            command_line(&source),
            "python tests/runtests.py django.forms --settings test_sqlite"
        );
    }

    #[test]
    fn the_command_that_ran_the_tests_earlier_is_recalled_only_when_a_call_runs_none() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        calls.worked("python runtests.py forms_tests", Some("tests"), 646);
        let said = calls
            .ran_nothing(dir.path(), &forms(), UNKNOWN_LABEL, false)
            .unwrap();
        assert!(said.starts_with(NOTHING_RAN), "{said}");
        assert!(
            said.ends_with(
                "Earlier in this session `python runtests.py forms_tests`, run in `tests`, \
                 executed 646 of the project's tests."
            ),
            "{said}"
        );
        // A reason nobody here recognises, and a command that worked: that
        // command is still worth saying, and no more is claimed than the
        // output shows.
        let said = calls
            .ran_nothing(dir.path(), &forms(), "Segmentation fault\n", false)
            .unwrap();
        assert!(said.starts_with(NOTHING_SHOWN), "{said}");
        assert!(said.contains("Earlier in this session"), "{said}");
        // The last one that worked is the one recalled; run at the root, no folder.
        calls.worked("pytest tests/test_a.py", Some("."), 7);
        let said = calls
            .ran_nothing(dir.path(), &forms(), "Ran 0 tests in 0.000s\n", false)
            .unwrap();
        assert!(
            said.contains("`pytest tests/test_a.py` executed 7"),
            "{said}"
        );
    }

    #[test]
    fn a_reason_not_recognised_and_nothing_that_worked_is_answered_with_nothing() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        for output in [
            "runtests.py: error: argument -v/--verbosity: expected one argument\n",
            "django.core.exceptions.ImproperlyConfigured: The SECRET_KEY setting must not be \
             empty.\n",
            "SyntaxError: invalid syntax\n",
            "",
        ] {
            assert_eq!(
                calls.ran_nothing(dir.path(), &forms(), output, false),
                None,
                "{output}"
            );
        }
        // A name with nothing like it among the tests: said, with no list.
        let call = json!({"executable": "python", "args": ["tests/runtests.py", "zebra"]});
        let said = calls
            .ran_nothing(
                dir.path(),
                &call,
                "ModuleNotFoundError: No module named 'zebra'\n",
                false,
            )
            .unwrap();
        assert!(
            said.contains("does not know `zebra`") && !said.contains("resemble"),
            "{said}"
        );
    }

    #[test]
    fn a_test_file_of_the_session_s_that_runs_no_test_is_told_so_and_its_scripts_are_left_alone() {
        let dir = workspace();
        let mut calls = TestCalls::default();
        calls.worked("python tests/runtests.py forms_tests", None, 646);
        // A test file the session wrote, asked of the runner by a name it
        // does not pick up: zero tests and `OK`.
        let call = json!({"executable": "python", "args": ["tests/runtests.py", "test_mine"]});
        let said = calls
            .ran_nothing(dir.path(), &call, "Ran 0 tests in 0.000s\n\nOK\n", true)
            .unwrap();
        assert!(
            said.starts_with("No test was executed by this call. The selection matched no test."),
            "{said}"
        );
        // A script of the session's that fails its own way is not a test
        // run that did not start, whatever worked before.
        let script = json!({"executable": "python", "args": ["test_mine.py"]});
        assert_eq!(
            calls.ran_nothing(dir.path(), &script, "AssertionError\n", true),
            None
        );
    }

    #[test]
    fn a_call_for_tests_is_known_by_its_own_words() {
        for call in [
            json!({"executable": "python", "args": ["tests/runtests.py", "forms"]}),
            json!({"executable": ".toolchains/python/bin/python3.11", "args": ["-m", "pytest"]}),
            json!({"executable": "pytest", "args": ["-q"]}),
            json!({"executable": "python", "args": ["-m", "unittest", "discover"]}),
            json!({"executable": "tox", "args": ["-e", "py39"]}),
            json!({"executable": "cargo", "args": ["test"]}),
            json!({"executable": "npm", "args": ["test"]}),
            json!({"executable": "make", "args": ["test"]}),
            json!({"executable": "bash", "args": ["-lc", "cd tests && python runtests.py forms"]}),
        ] {
            assert!(is_test_call(&call), "{call}");
        }
        for call in [
            json!({"executable": "ls", "args": ["tests"]}),
            json!({"executable": "git", "args": ["diff", "tests/test_forms.py"]}),
            json!({"executable": "python", "args": ["manage.py", "migrate"]}),
            json!({"executable": "python", "args": ["-m", "pip", "install", "pytest"]}),
            json!({"executable": "pip", "args": ["install", "pytest"]}),
            json!({"executable": "python", "args": ["tests/runtests.py", "--help"]}),
        ] {
            assert!(!is_test_call(&call), "{call}");
        }
    }
}
