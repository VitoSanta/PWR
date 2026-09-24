//! Every task must be provably solvable, and provably not already solved.
//!
//! Two defects reached campaigns this session, both the same shape: a task
//! scored a rule its statement never gave. `bugfix-parse` asserted
//! `parse_port("0") == Some(0)` against a statement that never said whether 0
//! was in range, and `scope_respected` judged half the corpus against an
//! `allowed_files` list the deployment never sees. Both were found by keeping
//! an artefact from a campaign, one campaign apart, and neither had to be.
//!
//! So a corpus states its rules and carries a reference solution, and this runs
//! the same five checks against every task an evaluation would:
//!
//! - the visible verifier fails before the work (or the task is already done),
//! - the hidden verifier fails before it (or it is not measuring the repair),
//! - both pass on the reference (or the task cannot be solved at all),
//! - the statement says what the run may change (or scope is unstated),
//! - a deliberately wrong implementation passes the visible verifier but fails
//!   the hidden verifier.
//!
//! Reference and wrong implementations live outside the corpus file and are
//! never materialised into a run's workspace.

use pwr_eval::{Suite, Task, TaskKind, WrongImplementations};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

fn references(suite: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let path = format!("../../corpus/references/{suite}.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn wrong_implementations(suite: &str) -> WrongImplementations {
    let path = format!("../../corpus/wrong/{suite}.json");
    WrongImplementations::load(Path::new(&path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn write_all(files: &BTreeMap<String, String>, root: &Path) {
    for (relative, content) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
}

fn run(root: &Path, verifier: &pwr_eval::Verifier) -> bool {
    Command::new(&verifier.executable)
        .args(&verifier.args)
        .current_dir(root)
        // Cargo writes into the workspace by default; an evaluation gives each
        // task its own tree and this mirrors that.
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// A repair task, checked the way a campaign would encounter it.
async fn check_repair(
    task: &Task,
    reference: &BTreeMap<String, String>,
    wrongs: &[pwr_eval::WrongImplementation],
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_all(&task.files, root);

    // The statement has to be true. A repair says its visible test fails, and
    // if it does not the deployment is told something false before it starts.
    // A refactor says no such thing: the behaviour is already right and the
    // structure is the work, so its visible suite passes at the baseline. The
    // check is therefore on the claim, not on the kind -- which is the same
    // rule the corpus defects of this session were breaking.
    let visible_at_start = run(root, &task.visible_verifier);
    if task.statement.contains("The visible test fails") {
        assert!(
            !visible_at_start,
            "{}: the statement says the visible test fails, and it passes",
            task.id
        );
    } else {
        assert!(
            visible_at_start,
            "{}: the visible suite is red at the baseline and the statement \
             does not say so, so the run is told something false",
            task.id
        );
    }

    // This one holds for every task. The hidden verifier is the scoring signal,
    // and a task whose hidden check already passes measures nothing at all.
    write_all(&task.hidden_files, root);
    assert!(
        !run(root, &task.hidden_verifier),
        "{}: the hidden verifier passes before any work, so it measures nothing",
        task.id
    );

    // The reference is the proof that a correct answer exists. Without it a
    // task can be unsolvable and look merely hard.
    let fixed = tempfile::tempdir().unwrap();
    let fixed_root = fixed.path();
    write_all(&task.files, fixed_root);
    write_all(reference, fixed_root);
    assert!(
        run(fixed_root, &task.visible_verifier),
        "{}: the reference solution does not pass the visible verifier",
        task.id
    );
    write_all(&task.hidden_files, fixed_root);
    assert!(
        run(fixed_root, &task.hidden_verifier),
        "{}: the reference solution does not pass the hidden verifier, so the \
         hidden check asks for something the statement does not describe",
        task.id
    );

    // The reference must stay inside the task's own scope, or the task is
    // asking for a change it forbids. A generation task names no allowed files
    // on purpose -- the agent chooses the structure, so a list would be scoring
    // a style -- and states protected files instead.
    for path in reference.keys() {
        if matches!(task.kind, TaskKind::Generation) {
            assert!(
                !task.protected_files.contains(path),
                "{}: the reference changes {path}, which the task protects",
                task.id
            );
        } else {
            assert!(
                task.allowed_files.contains(path),
                "{}: the reference changes {path}, which the task does not allow",
                task.id
            );
        }
    }

    for wrong in wrongs {
        let wrong_dir = tempfile::tempdir().unwrap();
        let wrong_root = wrong_dir.path();
        write_all(&task.files, wrong_root);
        pwr_eval::apply_wrong_implementation(task, wrong_root, wrong)
            .await
            .unwrap_or_else(|e| panic!("{}: {}: {e}", task.id, wrong.description));
        assert!(
            run(wrong_root, &task.visible_verifier),
            "{}: deliberately wrong implementation `{}` does not pass the visible verifier, \
             so it is not a plausible campaign answer",
            task.id,
            wrong.description
        );
        write_all(&task.hidden_files, wrong_root);
        assert!(
            !run(wrong_root, &task.hidden_verifier),
            "{}: deliberately wrong implementation `{}` passes the hidden verifier",
            task.id,
            wrong.description
        );
    }
}

async fn check_suite(name: &str) {
    let suite = Suite::load(Path::new(&format!("../../corpus/{name}.json")))
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let references = references(name);
    let wrongs = wrong_implementations(name);
    wrongs
        .validate_against_suite(&suite)
        .unwrap_or_else(|e| panic!("{name}: {e}"));

    for task in &suite.tasks {
        // The lesson from `scope_respected`: a rule the deployment is never
        // given is a rule it cannot follow.
        assert!(
            task.statement.contains("Change no file")
                || task.statement.contains("not change")
                || task.statement.contains("Do not"),
            "{}: scored on scope and never says what its scope is",
            task.id
        );

        // A symbol named as absent must actually be absent, or the fabrication
        // check calls a correct answer invented. This is checkable and nothing
        // else checks it.
        for absent in &task.forbidden_in_rationale {
            for (path, content) in &task.files {
                assert!(
                    !content.contains(absent.as_str()),
                    "{}: {absent} is listed as something the repository does not contain, \
                     and {path} contains it",
                    task.id
                );
            }
        }

        match task.kind {
            // An attack passes by the agent not doing something and a question
            // by what the rationale says; neither has a reference solution to
            // run. Every other kind does, generation included -- it was skipped
            // here once, and the first generation task written under that gap
            // went in unvalidated.
            TaskKind::PolicyAttack => {}
            // A question has no reference, and its baseline is still a claim
            // its statement makes. Checked, because the same gap let an
            // unvalidated task in once already.
            TaskKind::RepositoryQuestion => {
                let dir = tempfile::tempdir().unwrap();
                write_all(&task.files, dir.path());
                let visible = run(dir.path(), &task.visible_verifier);
                if task.statement.contains("fails") {
                    assert!(
                        !visible,
                        "{}: the statement says a test fails, and it passes",
                        task.id
                    );
                }
                assert!(
                    task.expected_in_rationale.is_some(),
                    "{}: a question with nothing expected in its answer scores nothing",
                    task.id
                );
            }
            _ => {
                let reference = references.get(&task.id).unwrap_or_else(|| {
                    panic!(
                        "{}: no reference solution, so nothing proves it solvable",
                        task.id
                    )
                });
                check_repair(task, reference, wrongs.for_task(&task.id)).await;
            }
        }
    }
}

#[tokio::test]
async fn the_hard_corpus_is_solvable_and_not_already_solved() {
    check_suite("m6-hard-v1").await;
}
