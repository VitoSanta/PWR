//! The surface was read, create and replace.
//!
//! A task that reorganises files could not be expressed at all: the agent
//! could write a file into a new directory but never make an empty one, move
//! anything, or remove what it had superseded. And it could not see its own
//! accumulated change -- it had to remember every file it had touched, and a
//! hash is not a diff.

use pwr_tools::*;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn policy(root: &Path) -> ToolPolicy {
    ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec![],
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(20),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

#[test]
fn a_directory_can_be_made_and_a_path_moved_into_it() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("old.rs"), "body").unwrap();
    let policy = policy(root.path());

    make_directory(&policy, Path::new("src/inner")).unwrap();
    assert!(root.path().join("src/inner").is_dir());

    let moved = move_path(&policy, Path::new("old.rs"), Path::new("src/inner/new.rs")).unwrap();
    assert_eq!(moved.from.as_deref(), Some("old.rs"));
    assert!(!root.path().join("old.rs").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("src/inner/new.rs")).unwrap(),
        "body"
    );
}

/// The same rule `write_file` follows, for the same reason: a blind overwrite
/// should never be one missing argument away.
#[test]
fn a_move_onto_an_existing_path_is_refused() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.rs"), "a").unwrap();
    fs::write(root.path().join("b.rs"), "b").unwrap();
    let error = move_path(&policy(root.path()), Path::new("a.rs"), Path::new("b.rs")).unwrap_err();
    assert!(error.to_string().contains("already exists"));
    // Neither file moved.
    assert_eq!(fs::read_to_string(root.path().join("b.rs")).unwrap(), "b");
}

/// A delete is the least reversible edit there is, so "the file I read" and
/// "the file on disk" being different matters more here than anywhere else.
#[test]
fn deleting_a_file_needs_its_current_hash() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("code.rs");
    fs::write(&path, "one").unwrap();
    let policy = policy(root.path());

    assert!(delete_path(&policy, Path::new("code.rs"), None, false).is_err());
    assert!(delete_path(&policy, Path::new("code.rs"), Some("stale"), false).is_err());
    assert!(path.exists(), "a refused delete removed the file anyway");

    let hash = read_file(&policy, Path::new("code.rs"))
        .unwrap()
        .artifact_hash;
    let removed = delete_path(&policy, Path::new("code.rs"), Some(&hash), false).unwrap();
    assert_eq!(removed.entries, 1);
    assert!(!path.exists());
}

/// A directory has no single hash, so removing one is deliberate rather than
/// guarded -- and the count of what went is returned so the audit says how
/// much rather than only that something did.
#[test]
fn a_directory_is_only_removed_when_that_was_asked_for() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("build/nested")).unwrap();
    fs::write(root.path().join("build/one.o"), "x").unwrap();
    fs::write(root.path().join("build/nested/two.o"), "x").unwrap();
    let policy = policy(root.path());

    assert!(delete_path(&policy, Path::new("build"), None, false).is_err());
    assert!(root.path().join("build").exists());

    let removed = delete_path(&policy, Path::new("build"), None, true).unwrap();
    assert!(removed.entries >= 2, "{removed:?}");
    assert!(!root.path().join("build").exists());
}

/// Both ends are resolved against the root, so neither reaches outside it.
#[test]
fn neither_end_of_a_move_may_leave_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a.rs"), "a").unwrap();
    let policy = policy(root.path());
    assert!(move_path(&policy, Path::new("a.rs"), Path::new("../escaped.rs")).is_err());
    assert!(move_path(&policy, Path::new("../../etc/hosts"), Path::new("here.rs")).is_err());
    assert!(delete_path(&policy, Path::new("../outside.txt"), Some("x"), false).is_err());
    assert!(make_directory(&policy, Path::new("../outside")).is_err());
}

/// A symlink is refused rather than followed: what it points at may live
/// outside the workspace, and deleting through one is deleting there.
#[cfg(unix)]
#[test]
fn a_symlink_is_not_deleted_through() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("precious.txt");
    fs::write(&target, "keep me").unwrap();
    std::os::unix::fs::symlink(&target, root.path().join("link.txt")).unwrap();

    assert!(
        delete_path(
            &policy(root.path()),
            Path::new("link.txt"),
            Some("x"),
            false
        )
        .is_err()
    );
    assert!(target.exists(), "the target was removed through the link");
}

// ------------------------------------------------------------------- vcs

fn git(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(status.status.success(), "git {args:?} failed");
}

#[test]
fn status_and_diff_report_what_actually_changed() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    git(root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("code.rs"), "fn one() {}\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "first"]);

    fs::write(root.join("code.rs"), "fn two() {}\n").unwrap();
    fs::write(root.join("added.rs"), "new\n").unwrap();

    let policy = policy(root);
    let status = block_on(vcs_status(&policy)).unwrap();
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status.head.is_some(), "HEAD was not read");
    let changed: Vec<&str> = status
        .changed
        .iter()
        .map(|(_, path)| path.as_str())
        .collect();
    assert!(changed.contains(&"code.rs"), "{changed:?}");
    assert!(changed.contains(&"added.rs"), "{changed:?}");

    let diff = block_on(vcs_diff(&policy, &[])).unwrap();
    assert_eq!(diff.exit_code, Some(0));
    assert!(diff.stdout.contains("-fn one() {}"), "{}", diff.stdout);
    assert!(diff.stdout.contains("+fn two() {}"), "{}", diff.stdout);
}

/// A workspace that is not a checkout reports no branch rather than inventing
/// one, which is the rule `session show` already follows.
#[test]
fn a_workspace_outside_version_control_reports_no_branch() {
    let root = tempfile::tempdir().unwrap();
    let status = block_on(vcs_status(&policy(root.path())));
    assert!(
        status.is_err() || status.unwrap().branch.is_none(),
        "a non-checkout claimed a branch"
    );
}

// ----------------------------------------------------------------- patches

/// A change touching three places in a file was three whole-file rewrites,
/// each carrying the entire file and each invalidating the hash the next one
/// was written against -- so the second and third arrived stale.
#[test]
fn several_hunks_land_under_one_hash_guard() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("code.rs"),
        "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n",
    )
    .unwrap();
    let policy = policy(root.path());
    let hash = read_file(&policy, Path::new("code.rs"))
        .unwrap()
        .artifact_hash;

    let applied = apply_patch(
        &policy,
        Path::new("code.rs"),
        &hash,
        &[
            Hunk {
                find: "fn alpha() {}".into(),
                replace: "fn alpha() { one() }".into(),
            },
            Hunk {
                find: "fn gamma() {}".into(),
                replace: "fn gamma() { three() }".into(),
            },
        ],
    )
    .unwrap();
    let after = fs::read_to_string(root.path().join("code.rs")).unwrap();
    assert!(after.contains("fn alpha() { one() }"), "{after}");
    assert!(after.contains("fn beta() {}"), "{after}");
    assert!(after.contains("fn gamma() { three() }"), "{after}");
    // The hash of the result, under the name the next call must pass it as.
    assert_eq!(applied.new_hash, applied.expected_hash);
}

/// A patch that half-lands leaves a file in a state nobody described, which is
/// worse than one that does not land at all.
#[test]
fn no_hunk_is_applied_if_any_of_them_would_fail() {
    let root = tempfile::tempdir().unwrap();
    let original = "fn alpha() {}\nfn beta() {}\n";
    fs::write(root.path().join("code.rs"), original).unwrap();
    let policy = policy(root.path());
    let hash = read_file(&policy, Path::new("code.rs"))
        .unwrap()
        .artifact_hash;

    let error = apply_patch(
        &policy,
        Path::new("code.rs"),
        &hash,
        &[
            Hunk {
                find: "fn alpha() {}".into(),
                replace: "fn alpha() { one() }".into(),
            },
            Hunk {
                find: "fn absent() {}".into(),
                replace: "never".into(),
            },
        ],
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("hunk 2"), "{error}");
    assert_eq!(
        fs::read_to_string(root.path().join("code.rs")).unwrap(),
        original,
        "the first hunk landed even though the patch failed"
    );
}

#[test]
fn an_ambiguous_or_stale_patch_is_refused() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("code.rs"), "x = 1\nx = 1\n").unwrap();
    let policy = policy(root.path());
    let hash = read_file(&policy, Path::new("code.rs"))
        .unwrap()
        .artifact_hash;

    // Two occurrences: choosing between them is the silent wrong edit the
    // hash guard exists to prevent.
    let ambiguous = apply_patch(
        &policy,
        Path::new("code.rs"),
        &hash,
        &[Hunk {
            find: "x = 1".into(),
            replace: "x = 2".into(),
        }],
    )
    .unwrap_err()
    .to_string();
    assert!(ambiguous.contains("matches 2 times"), "{ambiguous}");

    let stale = apply_patch(
        &policy,
        Path::new("code.rs"),
        "not-the-hash",
        &[Hunk {
            find: "x = 1\nx = 1".into(),
            replace: "x = 2".into(),
        }],
    )
    .unwrap_err()
    .to_string();
    // The refusal names the hash the file now has, as every other stale
    // refusal in this project does.
    assert!(stale.contains("now hashes to"), "{stale}");
}

/// "Not found" is true and sends the caller round the loop again on work that
/// is done.
#[test]
fn a_hunk_already_applied_says_so() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("code.rs"), "fn alpha() { one() }\n").unwrap();
    let policy = policy(root.path());
    let hash = read_file(&policy, Path::new("code.rs"))
        .unwrap()
        .artifact_hash;
    let error = apply_patch(
        &policy,
        Path::new("code.rs"),
        &hash,
        &[Hunk {
            find: "fn alpha() {}".into(),
            replace: "fn alpha() { one() }".into(),
        }],
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("already applied"), "{error}");
}

/// The harness's own scratch directory is not part of the repository, and a
/// listing that shows it is spending the deployment's budget on our noise.
///
/// Measured on an Angular build: `list_tree` with a budget of 100 entries
/// returned 97 paths inside `.pwr-scratch` — npm's debug logs and node's
/// compile cache, put there because a provisioned command runs with HOME and
/// TMPDIR redirected into it — and never reached `src/`. The deployment then
/// made nine `search` calls hunting for files by name, because the listing had
/// told it nothing about the project it was asked to work on.
#[test]
fn a_listing_never_shows_the_harness_its_own_scratch_directory() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    std::fs::create_dir_all(root.join(pwr_tools::SCRATCH_DIRECTORY).join(".npm/_logs")).unwrap();
    std::fs::write(
        root.join(pwr_tools::SCRATCH_DIRECTORY)
            .join(".npm/_logs/debug.log"),
        "noise",
    )
    .unwrap();
    std::fs::create_dir_all(root.join(pwr_tools::STATE_DIRECTORY)).unwrap();
    std::fs::write(
        root.join(pwr_tools::STATE_DIRECTORY).join("state.sqlite"),
        "x",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src/app")).unwrap();
    std::fs::write(root.join("src/app/main.ts"), "export const x = 1;").unwrap();

    let policy = pwr_tools::PolicyProfile::Safe.build(root.to_path_buf());
    let listed = pwr_tools::list_tree(&policy, 100).unwrap();
    let paths: Vec<&str> = listed.iter().map(|entry| entry.path.as_str()).collect();

    assert!(
        !paths
            .iter()
            .any(|path| path.starts_with(pwr_tools::SCRATCH_DIRECTORY)),
        "the listing shows our own scratch directory: {paths:?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.starts_with(pwr_tools::STATE_DIRECTORY)),
        "the listing shows our own state directory: {paths:?}"
    );
    // And the project it was actually asked about is there.
    assert!(
        paths.contains(&"src/app/main.ts"),
        "the listing did not reach the source: {paths:?}"
    );
}

/// A repository the search tests share: three languages, one name declared in
/// each, and the same name used elsewhere so a declaration search has something
/// wrong to find.
fn multilingual(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(
        root.join("src/money.rs"),
        "pub fn to_cents(v: f64) -> i64 {\n    (v * 100.0) as i64\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/convert.py"),
        "def to_cents(value):\n    return int(value * 100)\n",
    )
    .unwrap();
    fs::write(
        root.join("web/total.ts"),
        "import { to_cents } from './money';\nconst total = to_cents(12.5);\n",
    )
    .unwrap();
}

#[test]
fn a_regex_finds_what_the_same_query_as_literal_text_cannot() {
    let root = tempfile::tempdir().unwrap();
    multilingual(root.path());
    let policy = policy(root.path());

    let literal = search(&policy, "fn|def", 20).unwrap();
    assert!(
        literal.files.is_empty(),
        "the literal search matched something that is only a pattern: {:?}",
        literal.files
    );

    let pattern = search_query(
        &policy,
        &SearchQuery {
            pattern: "(fn|def) to_cents".into(),
            regex: true,
            path_glob: None,
            max_matches: 20,
        },
    )
    .unwrap();
    let paths: Vec<&str> = pattern
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, vec!["src/convert.py", "src/money.rs"]);
}

#[test]
fn a_path_glob_narrows_the_walk_without_hiding_depth() {
    let root = tempfile::tempdir().unwrap();
    multilingual(root.path());
    let policy = policy(root.path());

    let rust_only = search_query(
        &policy,
        &SearchQuery {
            pattern: "to_cents".into(),
            regex: false,
            path_glob: Some("**/*.rs".into()),
            max_matches: 20,
        },
    )
    .unwrap();
    let paths: Vec<&str> = rust_only
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(
        paths,
        vec!["src/money.rs"],
        "the glob did not narrow the walk, or it stopped at the first directory \
         that is not a .rs file"
    );

    // And the inverted form excludes rather than includes.
    let not_rust = search_query(
        &policy,
        &SearchQuery {
            pattern: "to_cents".into(),
            regex: false,
            path_glob: Some("!**/*.rs".into()),
            max_matches: 20,
        },
    )
    .unwrap();
    assert!(
        !not_rust.files.iter().any(|file| file.path.ends_with(".rs")),
        "`!` did not exclude: {:?}",
        not_rust.files
    );
}

#[test]
fn find_definition_finds_the_declaration_and_not_the_call() {
    let root = tempfile::tempdir().unwrap();
    multilingual(root.path());
    let policy = policy(root.path());

    let found = find_definition(&policy, "to_cents", None, 20).unwrap();
    let paths: Vec<&str> = found.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["src/convert.py", "src/money.rs"],
        "a declaration search must reach every language and stop at the call site"
    );
    assert!(
        !paths.contains(&"web/total.ts"),
        "the import and the call were reported as declarations"
    );
}

#[test]
fn find_definition_refuses_what_is_not_a_name() {
    let root = tempfile::tempdir().unwrap();
    multilingual(root.path());
    let policy = policy(root.path());

    // The mistake this catches: pasting the search query, keyword and all.
    let refused = find_definition(&policy, "def to_cents", None, 20).unwrap_err();
    assert!(
        format!("{refused}").contains("single identifier"),
        "the refusal does not say what a name is: {refused}"
    );
    assert!(find_definition(&policy, "  ", None, 20).is_err());
}

#[test]
fn a_file_is_found_by_name_when_its_content_never_says_it() {
    // The tomli run of 2026-09-18: `hex-escape` searched for thirteen times
    // across the test data, nothing each time, while the file was named so.
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("tests/data/valid/spec-1.1.0");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("hex-escape.toml"), "answer = \"\\x64\"\n").unwrap();
    fs::write(data.join("other.toml"), "x = 1\n").unwrap();
    let found = search(&policy(root.path()), "hex-escape", 20).unwrap();
    assert!(found.files.is_empty(), "no content mentions it");
    assert_eq!(
        found.files_named,
        vec!["tests/data/valid/spec-1.1.0/hex-escape.toml".to_string()]
    );
}

#[test]
fn a_directory_given_as_path_glob_is_searched_inside_and_the_result_says_so() {
    let root = tempfile::tempdir().unwrap();
    let valid = root.path().join("tests/data/valid");
    fs::create_dir_all(&valid).unwrap();
    fs::write(valid.join("case.toml"), "answer = \"x64\"\n").unwrap();
    let query = SearchQuery {
        pattern: "x64".into(),
        regex: false,
        path_glob: Some("tests/data/valid".into()),
        max_matches: 20,
    };
    let found = search_query(&policy(root.path()), &query).unwrap();
    assert_eq!(found.matches_returned, 1);
    assert!(found.note.unwrap().contains("tests/data/valid/**"));
}

#[test]
fn a_real_glob_is_left_as_written_and_carries_no_note() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/lib.rs"), "fn needle() {}\n").unwrap();
    let query = SearchQuery {
        pattern: "needle".into(),
        regex: false,
        path_glob: Some("src/**".into()),
        max_matches: 20,
    };
    let found = search_query(&policy(root.path()), &query).unwrap();
    assert_eq!(found.matches_returned, 1);
    assert!(found.note.is_none());
    assert!(found.files_named.is_empty());
}

#[test]
fn a_hash_copied_with_one_slip_still_names_the_file_and_a_stale_one_does_not() {
    let current = "236e7d9d0db41fd2ff372b3f592beff5aae31fdd5684405bfe9e0917332c87d5";
    // The doubled letter Qwen3.6 wrote on 2026-09-18.
    assert!(hash_matches(
        current,
        "236e7d9d0db41fd2ff372b3f592beff5aae31fdd5684405bbfe9e0917332c87d5"
    ));
    // A dropped and a swapped character.
    assert!(hash_matches(
        current,
        "236e7d9d0db41fd2ff372b3f592beff5aae31fdd5684405fe9e0917332c87d5"
    ));
    assert!(hash_matches(
        current,
        "236e7d9d0db41fd2ff372b3f592beff5aae31fdd5684405bfe9e0917332c8d75"
    ));
    // A prefix long enough to be a claim.
    assert!(hash_matches(current, "236e7d9d0db4"));
    assert!(!hash_matches(current, "236e7d9d"));
    // The right head and another hash's tail, as Qwen3.6 wrote on 2026-09-23.
    assert!(hash_matches(
        "c635b6af191dbe933847fde1e6edfe9ec9fe2d6fe2908f161762df09657d642b",
        "c635b6af191dbe933847f09c5029d79720e236288d9b6a3997d17019c73d68ff7e28b077"
    ));
    // Fifteen right characters are not enough to ignore the rest.
    assert!(!hash_matches(
        current,
        "236e7d9d0db41fdXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
    ));
    // The hash of the file before it changed: unrelated, and refused.
    let stale = "3b41af144e14c2190d10653563034a2d17afaf9859a943d9b5684b93dc3fc042";
    assert!(!hash_matches(current, stale));
    assert!(!hash_matches(current, ""));
}

#[test]
fn an_edit_with_a_slipped_hash_is_applied_and_one_against_a_changed_file_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("probe.rs");
    fs::write(&path, "pub fn value() -> i32 {\n    1\n}\n").unwrap();
    let policy = policy(root.path());
    let real = pwr_domain::hash_bytes(fs::read(&path).unwrap());
    let mut slipped = real.clone();
    slipped.insert(40, slipped.as_bytes()[40] as char);
    let done = apply_replace(
        &policy,
        Path::new("probe.rs"),
        &slipped,
        "pub fn value() -> i32 {\n    2\n}\n",
    );
    assert!(done.is_ok(), "{done:?}");
    // The file has changed, so the hash read before the change is stale.
    let refused = apply_replace(
        &policy,
        Path::new("probe.rs"),
        &real,
        "pub fn value() -> i32 {\n    3\n}\n",
    );
    assert!(refused.is_err());
}

#[test]
fn a_fragment_sent_as_the_whole_file_is_refused_with_the_numbers() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("results.py");
    let original: String = (0..900).map(|i| format!("line_{i} = {i}\n")).collect();
    fs::write(&path, &original).unwrap();
    let policy = policy(root.path());
    let hash = pwr_domain::hash_bytes(original.as_bytes());
    let method = "    def __iadd__(self, other):\n        if not other:\n            return self\n";
    let refused = apply_replace(&policy, Path::new("results.py"), &hash, method)
        .expect_err("a three-line fragment replaced a 900-line file");
    let text = refused.to_string();
    assert!(
        text.contains("900 lines") && text.contains("replace_text"),
        "{text}"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original,
        "the file was touched"
    );

    // A real rewrite that keeps most of the file still lands.
    let edited = original.replacen("line_0 = 0", "line_0 = 1", 1);
    assert!(apply_replace(&policy, Path::new("results.py"), &hash, &edited).is_ok());
    // As on a 25-line test file, where a two-line fragment wiped it.
    let test_file: String = (0..25).map(|i| format!("case_{i} = {i}\n")).collect();
    fs::write(root.path().join("test_x.py"), &test_file).unwrap();
    let short_hash = pwr_domain::hash_bytes(test_file.as_bytes());
    assert!(
        apply_replace(
            &policy,
            Path::new("test_x.py"),
            &short_hash,
            "# Cases the parser does not support yet\nXFAIL = {\"negative\"}\n",
        )
        .is_err()
    );
    // And a short file may be rewritten to any length.
    fs::write(root.path().join("small.py"), "a = 1\nb = 2\nc = 3\n").unwrap();
    let small = pwr_domain::hash_bytes(b"a = 1\nb = 2\nc = 3\n");
    assert!(apply_replace(&policy, Path::new("small.py"), &small, "a = 1\n").is_ok());
}

#[test]
fn a_partial_read_says_the_hash_is_the_whole_files() {
    let root = tempfile::tempdir().unwrap();
    let text: String = (1..=100).map(|i| format!("{i}\n")).collect();
    fs::write(root.path().join("f.py"), &text).unwrap();
    let policy = policy(root.path());
    let window = read_file_window(&policy, Path::new("f.py"), Some(21), Some(10)).unwrap();
    let note = window.edit_note.expect("a window without a note");
    assert!(
        note.contains("lines 21-30 of 100") && note.contains("whole file"),
        "{note}"
    );
    assert!(
        read_file(&policy, Path::new("f.py"))
            .unwrap()
            .edit_note
            .is_none()
    );
}

#[test]
fn a_line_is_placed_in_the_definitions_that_hold_it() {
    let source = "import x\n\nclass Tests(Base):\n    def setUp(self):\n        pass\n\n    def test_add(self):\n        a = 1\n\n        assert a\n\ndef helper():\n    return 2\n";
    let lines: Vec<&str> = source.lines().collect();
    assert_eq!(
        enclosing_definitions(&lines, 9).as_deref(),
        Some("line 3: class Tests(Base) > line 7: def test_add(self)")
    );
    assert_eq!(
        enclosing_definitions(&lines, 12).as_deref(),
        Some("line 12: def helper()")
    );
    assert_eq!(enclosing_definitions(&lines, 0), None);
    let rust = "impl Engine {\n    pub fn run(&self) {\n        go();\n    }\n}\n";
    let lines: Vec<&str> = rust.lines().collect();
    assert_eq!(
        enclosing_definitions(&lines, 2).as_deref(),
        Some("line 1: impl Engine > line 2: pub fn run(&self)")
    );
}

#[test]
fn search_and_read_say_where_a_line_sits() {
    let root = tempfile::tempdir().unwrap();
    let mut source = String::from("class Test02(TestCase):\n");
    for i in 0..50 {
        source.push_str(&format!("    def test_{i}(self):\n        value = {i}\n"));
    }
    fs::write(root.path().join("t.py"), &source).unwrap();
    let policy = policy(root.path());
    let found = search(&policy, "value = 30", 5).unwrap();
    let hit = &found.files[0].lines[0];
    assert_eq!(
        hit.within.as_deref(),
        Some("line 1: class Test02(TestCase) > line 62: def test_30(self)")
    );
    let window = read_file_window(&policy, Path::new("t.py"), Some(63), Some(3)).unwrap();
    assert_eq!(window.within, hit.within);
}

/// Seen 2026-09-18: inside the sandbox every vcs_status failed with
/// `couldn't create cache file`, because macOS's `/usr/bin/git` is an xcrun
/// shim writing a cache outside the sandbox's writable paths. xcrun writes
/// that cache only when it is missing or stale, so this passes with the shim
/// too on a machine whose cache is fresh: it guards that version control works
/// sandboxed, not the cache condition itself.
#[test]
fn version_control_answers_inside_the_sandbox() {
    let root = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    fs::write(root.path().join("a.txt"), "one\n").unwrap();
    git(&["add", "."]);
    git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "init",
    ]);
    fs::write(root.path().join("a.txt"), "two\n").unwrap();
    let mut policy = policy(root.path());
    policy.sandbox = SandboxPolicy::Preferred;
    let status = block_on(vcs_status(&policy));
    assert!(status.is_ok(), "{status:?}");
    assert!(format!("{:?}", status.unwrap()).contains("a.txt"));
}

#[test]
fn a_file_is_written_back_to_its_original_bytes() {
    let root = tempfile::tempdir().unwrap();
    let original = b"def f():\n    return 1\n";
    fs::write(root.path().join("m.py"), "broken").unwrap();
    let policy = policy(root.path());
    let restored = restore_file(&policy, Path::new("m.py"), original).unwrap();
    assert_eq!(fs::read(root.path().join("m.py")).unwrap(), original);
    assert_eq!(restored.new_hash, pwr_domain::hash_bytes(original));
    assert!(
        restore_file(&policy, Path::new("m.py"), original).is_err(),
        "a no-op restore"
    );
}

/// Seen 2026-09-19 (suite A3, `a3-crlf-line-endings`): a model cannot type a
/// carriage return through a tool call, and a CRLF file edited with LF text
/// ended up mixed, then rebuilt by a script.
#[test]
fn an_edit_keeps_a_crlf_files_line_endings() {
    let root = tempfile::tempdir().unwrap();
    let original = "A = {\r\n    \"km\": 1000,\r\n}\r\n\r\ndef f(u):\r\n    return A[u]\r\n";
    let policy = policy(root.path());
    let path = root.path().join("units.py");
    let all_crlf = |bytes: &[u8]| {
        bytes.iter().filter(|b| **b == b'\n').count()
            == bytes.windows(2).filter(|w| w == b"\r\n").count()
    };

    fs::write(&path, original).unwrap();
    let hash = pwr_domain::hash_bytes(original.as_bytes());
    let replaced = replace_text(
        &policy,
        Path::new("units.py"),
        &hash,
        "    \"km\": 1000,\n}",
        "    \"km\": 1000,\n    \"mm\": 0.001,\n}",
    )
    .unwrap();
    let after = fs::read(&path).unwrap();
    assert!(all_crlf(&after), "{:?}", String::from_utf8_lossy(&after));
    assert!(String::from_utf8_lossy(&after).contains("\"mm\": 0.001,\r\n"));
    assert!(replaced.normalized.is_some());

    let hash = replaced.new_hash;
    let whole = "A = {\n    \"km\": 1000,\n    \"mm\": 0.001,\n    \"m\": 1,\n}\n\ndef f(u):\n    return A[u]\n";
    apply_replace(&policy, Path::new("units.py"), &hash, whole).unwrap();
    assert!(all_crlf(&fs::read(&path).unwrap()));

    let hash = pwr_domain::hash_bytes(fs::read(&path).unwrap());
    apply_patch(
        &policy,
        Path::new("units.py"),
        &hash,
        &[Hunk {
            find: "def f(u):\n    return A[u]".into(),
            replace: "def f(u):\n    if u not in A:\n        raise ValueError(u)\n    return A[u]"
                .into(),
        }],
    )
    .unwrap();
    assert!(all_crlf(&fs::read(&path).unwrap()));

    // An LF file is left as it is.
    fs::write(root.path().join("lf.py"), "a = 1\nb = 2\n").unwrap();
    let hash = pwr_domain::hash_bytes(b"a = 1\nb = 2\n");
    let lf = replace_text(&policy, Path::new("lf.py"), &hash, "b = 2", "b = 3\nc = 4").unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("lf.py")).unwrap(),
        "a = 1\nb = 3\nc = 4\n"
    );
    assert!(lf.normalized.is_none());
}

/// Seen 2026-09-19 (catalogue, Nemotron 3.5): `/slugify.py` meant the
/// workspace's `slugify.py`. Still refused, but the refusal says so.
#[test]
fn a_root_anchored_path_is_read_as_the_workspace_file_it_names() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("slugify.py"), "x = 1\n").unwrap();
    let policy = policy(root.path());
    // Read as the workspace file it names, since that exists.
    assert_eq!(
        read_file(&policy, Path::new("/slugify.py"))
            .unwrap()
            .content,
        "x = 1"
    );
    assert_eq!(
        read_file(&policy, Path::new("/workspace/slugify.py"))
            .unwrap()
            .content,
        "x = 1"
    );
    // One that names nothing here stays refused, and names the rule.
    let elsewhere = read_file(&policy, Path::new("/etc/hosts"))
        .unwrap_err()
        .to_string();
    assert!(elsewhere.contains("outside the workspace"), "{elsewhere}");
    assert!(read_file(&policy, Path::new("/../slugify.py")).is_err());
}

/// Root-anchored paths are read as the workspace file they name, so the
/// protected-file guard must see them the same way.
#[test]
fn a_protected_file_cannot_be_reached_by_anchoring_or_dot_segments() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("spec.md"), "the task\n").unwrap();
    let mut policy = policy(root.path());
    policy.protected = vec![std::path::PathBuf::from("spec.md")];
    let hash = pwr_domain::hash_bytes(b"the task\n");
    for path in ["spec.md", "/spec.md", "/workspace/spec.md", "./spec.md"] {
        let refused = apply_replace(&policy, Path::new(path), &hash, "rewritten\n");
        assert!(refused.is_err(), "{path} reached a protected file");
    }
    assert_eq!(
        fs::read_to_string(root.path().join("spec.md")).unwrap(),
        "the task\n"
    );
}

/// `list_tree` under one directory lists only what is inside it, with paths
/// still relative to the workspace root; a path that is not a directory is
/// refused with the way to see what is there (backlog C.24).
#[test]
fn list_tree_can_be_limited_to_one_directory() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::write(root.path().join("src/app/app.ts"), "x").unwrap();
    std::fs::write(root.path().join("README.md"), "x").unwrap();
    let policy = pwr_tools::ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: Vec::new(),
        output_limit: 64 * 1024,
        timeout: std::time::Duration::from_secs(5),
        sandbox: pwr_tools::SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    let listed: Vec<String> = pwr_tools::list_tree_under(&policy, 100, Some("src"))
        .unwrap()
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    assert!(listed.contains(&"src/app/app.ts".to_owned()), "{listed:?}");
    assert!(!listed.contains(&"README.md".to_owned()), "{listed:?}");
    assert!(!listed.contains(&"src".to_owned()), "{listed:?}");
    assert!(pwr_tools::list_tree_under(&policy, 100, Some("README.md")).is_err());
    assert!(pwr_tools::list_tree_under(&policy, 100, Some("../elsewhere")).is_err());
}

/// A reference folder is listed where it is, with paths that read_file takes
/// as they are: chat mode's folders are only these (2026-09-23).
#[test]
fn a_reference_folder_is_listed_with_readable_paths() {
    let workspace = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    std::fs::create_dir(reference.path().join("docs")).unwrap();
    std::fs::write(reference.path().join("docs/guide.md"), "the guide\n").unwrap();
    let folder = reference.path().canonicalize().unwrap();
    let policy = pwr_tools::ToolPolicy {
        extra_readable: vec![folder.clone()],
        ..pwr_tools::PolicyProfile::Safe.build(workspace.path().to_path_buf())
    };
    let named = folder.display().to_string();
    let listed = pwr_tools::list_tree_under(&policy, 50, Some(&named)).unwrap();
    let file = listed
        .iter()
        .find(|entry| entry.path.ends_with("docs/guide.md"))
        .expect("the document is listed");
    let read = pwr_tools::read_file(&policy, std::path::Path::new(&file.path)).unwrap();
    assert!(read.content.contains("the guide"));
    // Somewhere not attached stays refused.
    assert!(pwr_tools::list_tree_under(&policy, 50, Some("/etc")).is_err());
}
