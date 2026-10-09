//! What the project's own tests have confirmed about the source as it stands.
//!
//! Measured 2026-10-09 (fifteen runs, Qwen 3.5 9B, three SWE-bench
//! instances): in ten of them the model read a run of the project's tests,
//! all passing, after its last change to the source -- and went on for a
//! median of 8.7 minutes and 12.5 further checks (19 minutes on one
//! instance), most of them trial scripts of its own. The output said `OK`;
//! nothing said what that `OK` was worth.
//!
//! So it is said: when a run of the project's tests executes tests and all
//! pass, the result carries which command, how many tests, and that they
//! are about the source as it is now; and the first time something is run
//! or a trial file is written after that with the source unchanged, that
//! result says the confirmation still stands.
//!
//! It is information. It does not end a goal, refuse an action or say the
//! request is met: the tests that exist passing is not the request being
//! satisfied, and the note says so.

/// A run of the project's tests that executed tests and passed them all.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Passed {
    command: String,
    tests: usize,
    /// How many actions have been observed since.
    actions_since: usize,
}

/// The confirmation the source has, if any, and whether the model has been
/// told once that it still stands.
#[derive(Debug, Default)]
pub struct Confirmation {
    standing: Option<Passed>,
    reminded: bool,
}

impl Confirmation {
    /// The source changed: what the tests said was about other code.
    pub fn source_changed(&mut self) {
        self.standing = None;
        self.reminded = false;
    }

    /// The project's tests ran and all passed. What to say with the result.
    pub fn passed(&mut self, command: &str, tests: usize, tests_changed: &[String]) -> String {
        self.standing = Some(Passed {
            command: command.to_owned(),
            tests,
            actions_since: 0,
        });
        self.reminded = false;
        let changed = if tests_changed.is_empty() {
            String::new()
        } else {
            format!(
                " This session changed test files ({}), so these are not the project's tests as \
                 they were.",
                tests_changed.join(", ")
            )
        };
        format!(
            "`{command}` ran {tests} of the project's tests and all passed, on the source as it \
             stands now.{changed} That is what the project's own tests can say about your \
             change; it does not by itself mean the request is met. Running them again, or \
             writing trial scripts for the same behaviour, adds nothing to it unless the source \
             changes."
        )
    }

    /// Something else was run, or a trial file written, with the source as it
    /// was when the tests passed. Said once for each confirmation.
    pub fn checking_again(&mut self) -> Option<String> {
        let passed = self.standing.as_mut()?;
        passed.actions_since += 1;
        if self.reminded {
            return None;
        }
        self.reminded = true;
        Some(format!(
            "The source has not changed since `{}` ran {} of the project's tests, all passing. \
             This adds to that only if it checks something those tests do not.",
            passed.command, passed.tests
        ))
    }

    /// Any other action, counted so the state knows how long ago the tests ran.
    pub fn other_action(&mut self) {
        if let Some(passed) = self.standing.as_mut() {
            passed.actions_since += 1;
        }
    }
}

/// Whether a path is a test file by where it is or what it is called: under
/// a `tests` folder anywhere, under a `test` or `spec` folder at the root, or
/// named as one. Not everything under a folder called `test`: Django's own
/// test framework is source, in `django/test/`.
pub fn is_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name);
    let folders: Vec<&str> = path.split('/').rev().skip(1).collect();
    folders
        .iter()
        .any(|part| matches!(*part, "tests" | "__tests__"))
        || folders
            .last()
            .is_some_and(|first| matches!(*first, "test" | "spec"))
        || stem.starts_with("test_")
        || stem.ends_with("_test")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name == "conftest.py"
}

/// Whether a file is a trial of the session's own rather than the project's
/// source: one this session created, at the workspace root or named as a
/// test. A file the project already had is its source wherever it is, and
/// so is a new file placed among its folders.
pub fn is_trial_file(path: &str, created_by_this_session: bool) -> bool {
    created_by_this_session && (!path.contains('/') || is_test_path(path))
}

/// Whether a command is a run of the project's tests and not of something
/// the session wrote: it names none of the files the session created, and
/// its code is not given inline or on standard input.
pub fn runs_the_projects_tests(arguments: &serde_json::Value, created: &[String]) -> bool {
    let words: Vec<&str> = arguments["args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect();
    if arguments["stdin"]
        .as_str()
        .is_some_and(|text| !text.trim().is_empty())
        || words
            .iter()
            .any(|word| matches!(*word, "-c" | "-e" | "--eval"))
    {
        return false;
    }
    !created.iter().any(|path| {
        let name = path.rsplit('/').next().unwrap_or(path);
        let module = name.split('.').next().unwrap_or(name);
        words.iter().any(|word| {
            word.contains(path.as_str())
                || word.rsplit('/').next() == Some(name)
                || word.split(['.', ':']).any(|part| part == module)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tests_that_pass_are_said_once_with_what_they_are_worth() {
        let mut confirmation = Confirmation::default();
        let said = confirmation.passed("python tests/runtests.py forms_tests", 646, &[]);
        assert!(
            said.contains("ran 646 of the project's tests and all passed"),
            "{said}"
        );
        assert!(said.contains("on the source as it stands now"), "{said}");
        // Never the request itself, and never an order to stop.
        assert!(
            said.contains("does not by itself mean the request is met"),
            "{said}"
        );
        assert!(!said.to_lowercase().contains("complete"), "{said}");
        assert!(!said.to_lowercase().contains("must"), "{said}");
    }

    #[test]
    fn checking_again_with_the_source_unchanged_is_said_once() {
        let mut confirmation = Confirmation::default();
        // Nothing confirmed: nothing to say about checking again.
        assert_eq!(confirmation.checking_again(), None);
        confirmation.passed("pytest tests/test_a.py", 7, &[]);
        let said = confirmation.checking_again().unwrap();
        assert!(
            said.contains("has not changed since `pytest tests/test_a.py` ran 7"),
            "{said}"
        );
        assert!(
            said.contains("only if it checks something those tests do not"),
            "{said}"
        );
        // Not at every action after it.
        assert_eq!(confirmation.checking_again(), None);
        confirmation.other_action();
        assert_eq!(confirmation.checking_again(), None);
    }

    #[test]
    fn a_change_to_the_source_voids_what_the_tests_said() {
        let mut confirmation = Confirmation::default();
        confirmation.passed("pytest", 7, &[]);
        confirmation.source_changed();
        assert_eq!(confirmation.checking_again(), None);
        // Passing again on the new source is a new confirmation, said in full
        // and reminded of once.
        assert!(confirmation.passed("pytest", 7, &[]).contains("ran 7"));
        assert!(confirmation.checking_again().is_some());
        // And tests passing again without a change in between start it over.
        confirmation.passed("pytest", 8, &[]);
        assert!(confirmation.checking_again().unwrap().contains("ran 8"));
    }

    #[test]
    fn tests_this_session_changed_are_not_the_projects_as_they_were() {
        let mut confirmation = Confirmation::default();
        let said = confirmation.passed("pytest", 7, &["tests/test_a.py".into()]);
        assert!(
            said.contains("This session changed test files (tests/test_a.py)"),
            "{said}"
        );
    }

    #[test]
    fn a_trial_file_is_one_the_session_made_at_the_root_or_as_a_test() {
        assert!(is_trial_file("test_deepcopy_fix.py", true));
        assert!(is_trial_file("try_it.py", true));
        assert!(is_trial_file("tests/test_new.py", true));
        // A new module among the project's folders is its source.
        assert!(!is_trial_file("django/utils/dates.py", true));
        // What the project already had is its source, wherever it is.
        assert!(!is_trial_file("setup.py", false));
        assert!(!is_trial_file("django/forms/fields.py", false));
        assert!(!is_trial_file("tests/forms_tests/test_base.py", false));
        assert!(is_test_path("tests/forms_tests/test_base.py"));
        assert!(is_test_path("src/app/app.component.spec.ts"));
        assert!(is_test_path("test/unit/a.rb"));
        // Source, in a folder called `test`.
        assert!(!is_test_path("django/test/utils.py"));
    }

    #[test]
    fn the_projects_tests_are_not_a_file_or_a_script_of_the_sessions_own() {
        let created = vec!["test_deepcopy_fix.py".to_owned()];
        let project = json!({"executable": "python", "args": ["tests/runtests.py", "forms_tests"]});
        assert!(runs_the_projects_tests(&project, &created));
        assert!(runs_the_projects_tests(
            &json!({"executable": "pytest", "args": ["tests/test_a.py"]}),
            &created
        ));
        // The session's own file, by path, by name or as a module.
        for own in [
            json!({"executable": "python", "args": ["test_deepcopy_fix.py"]}),
            json!({"executable": "pytest", "args": ["./test_deepcopy_fix.py", "-q"]}),
            json!({"executable": "python", "args": ["-m", "unittest", "test_deepcopy_fix"]}),
            json!({"executable": "python", "args": ["-m", "pytest", "test_deepcopy_fix.py::test_one"]}),
        ] {
            assert!(!runs_the_projects_tests(&own, &created), "{own}");
        }
        // Code given inline or on standard input is the session's own.
        assert!(!runs_the_projects_tests(
            &json!({"executable": "python", "args": ["-c", "import unittest"]}),
            &[]
        ));
        assert!(!runs_the_projects_tests(
            &json!({"executable": "python", "stdin": "import unittest\nunittest.main()"}),
            &[]
        ));
    }
}
