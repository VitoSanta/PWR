//! Process isolation and the approval gates for effects that leave the workspace.

mod common;

use pwr_tools::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn policy(root: &Path) -> ToolPolicy {
    ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["sh".into(), "git".into(), "cargo".into(), "echo".into()],
        output_limit: 8192,
        timeout: Duration::from_secs(20),
        sandbox: SandboxPolicy::Preferred,
        approvals: Vec::new(),
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

// -------------------------------------------------------------- approvals

#[test]
fn editing_a_dependency_manifest_requires_approval() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Cargo.toml"), "[package]").unwrap();
    let mut policy = policy(root.path());
    let hash = pwr_domain::hash_bytes("[package]");
    let denied = apply_replace(
        &policy,
        Path::new("Cargo.toml"),
        &hash,
        "[package]\nevil = \"1\"",
    );
    assert!(denied.is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("Cargo.toml")).unwrap(),
        "[package]"
    );

    policy.approvals = vec![Approval::DependencyChange];
    assert!(
        apply_replace(
            &policy,
            Path::new("Cargo.toml"),
            &hash,
            "[package]\nok = \"1\""
        )
        .is_ok()
    );
}

#[test]
fn every_known_manifest_and_lockfile_is_gated() {
    for manifest in [
        "Cargo.toml",
        "Cargo.lock",
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "requirements.txt",
        "pyproject.toml",
        "poetry.lock",
        "go.mod",
        "go.sum",
        "Gemfile",
    ] {
        assert_eq!(
            edit_approval(Path::new(manifest)),
            Some(Approval::DependencyChange),
            "not gated: {manifest}"
        );
        // Nested copies are gated too; the gate is the file, not its depth.
        assert_eq!(
            edit_approval(&PathBuf::from("vendor/sub").join(manifest)),
            Some(Approval::DependencyChange)
        );
    }
    assert_eq!(edit_approval(Path::new("src/main.rs")), None);
}

#[test]
fn history_rewriting_requires_approval() {
    for args in [
        vec!["rebase".to_string(), "-i".into(), "HEAD~3".into()],
        vec!["commit".to_string(), "--amend".into()],
        vec!["reset".to_string(), "--hard".into(), "HEAD~1".into()],
        vec!["filter-branch".to_string()],
    ] {
        assert_eq!(
            command_approval("git", &args),
            Some(Approval::HistoryRewrite),
            "not gated: git {args:?}"
        );
    }
    assert_eq!(command_approval("git", &["status".to_string()]), None);
}

#[test]
fn publishing_and_pushing_require_approval() {
    assert_eq!(
        command_approval("cargo", &["publish".to_string()]),
        Some(Approval::Publish)
    );
    assert_eq!(
        command_approval("npm", &["publish".to_string()]),
        Some(Approval::Publish)
    );
    assert_eq!(
        command_approval("git", &["push".to_string(), "origin".into(), "main".into()]),
        Some(Approval::Publish)
    );
    // A forced push is gated whichever rule catches it first.
    assert!(command_approval("git", &["push".to_string(), "--force".into()]).is_some());
}

#[test]
fn an_ungranted_approval_stops_the_command_before_it_runs() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    let denied = block_on(run_command(
        &policy,
        "git",
        &["push".to_string(), "origin".into(), "main".into()],
    ));
    assert!(denied.is_err());
}

#[test]
fn granting_one_approval_does_not_grant_another() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.approvals = vec![Approval::DependencyChange];
    assert!(policy.require(Approval::DependencyChange).is_ok());
    assert!(policy.require(Approval::Publish).is_err());
    assert!(policy.require(Approval::HistoryRewrite).is_err());
}

#[test]
fn the_default_profile_grants_nothing() {
    for profile in [PolicyProfile::Safe, PolicyProfile::Development] {
        let policy = profile.build(PathBuf::from("/tmp"));
        assert!(policy.approvals.is_empty());
        assert!(policy.require(Approval::Publish).is_err());
    }
}

// --------------------------------------------------------------- sandbox

#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_command_cannot_write_outside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().canonicalize().unwrap().join("escaped.txt");
    let policy = policy(root.path());
    let result = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            format!("echo pwned > {}", target.display()),
        ],
    ))
    .unwrap();
    assert!(result.sandboxed, "the fixture did not exercise a sandbox");
    assert!(!target.exists(), "sandbox did not prevent the write");
    assert_ne!(result.exit_code, Some(0));
}

/// Measured 2026-10-01: a command killed the dev server of PWR's own window
/// (`pkill -f 'ng serve'`). The profile denies signalling any process the
/// sandboxed command did not start; its own children stay stoppable.
#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_command_cannot_signal_a_process_it_did_not_start() {
    let root = tempfile::tempdir().unwrap();
    let quiet = root.path().join("quiet.txt");
    fs::write(&quiet, "").unwrap();
    // A process of the person's own, started outside any sandbox.
    let mut victim = std::process::Command::new("tail")
        .args(["-f", quiet.to_str().unwrap()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("pkill".into());
    let by_pid = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            format!("kill -TERM {}; echo status=$?", victim.id()),
        ],
    ))
    .unwrap();
    assert!(by_pid.sandboxed, "the fixture did not exercise a sandbox");
    assert!(by_pid.stdout.contains("status=1"), "{by_pid:?}");
    let by_name = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            "pkill -f 'tail -f' ; echo status=$?".to_string(),
        ],
    ))
    .unwrap();
    assert!(by_name.stdout.contains("status=1"), "{by_name:?}");
    assert!(
        victim.try_wait().unwrap().is_none(),
        "a sandboxed command killed a process it did not start"
    );
    // What it started itself it can stop.
    let own = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            "tail -f /dev/null & kill $!; echo status=$?".to_string(),
        ],
    ))
    .unwrap();
    assert!(own.stdout.contains("status=0"), "{own:?}");
    let _ = victim.kill();
    let _ = victim.wait();
}

/// A refusal says it was the sandbox, and what to do about it -- rather
/// than reading as a project that cannot write its own files.
#[cfg(target_os = "macos")]
#[test]
fn a_sandbox_refusal_says_it_was_the_sandbox() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().canonicalize().unwrap().join("escaped.txt");
    let result = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), format!("echo x > {}", target.display())],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert!(
        result.stderr.contains("Operation not permitted"),
        "{result:?}"
    );
    assert!(
        result
            .stderr
            .contains("[PWR] This command ran in the sandbox"),
        "{result:?}"
    );
    assert!(result.stderr.contains("outside_sandbox"), "{result:?}");
}

/// Full access: the person's own environment, no boundary -- and the record
/// says the command was not sandboxed.
#[cfg(target_os = "macos")]
#[test]
fn full_access_runs_unconfined_in_the_persons_environment() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().canonicalize().unwrap().join("written.txt");
    let mut policy = policy(root.path());
    policy.sandbox = SandboxPolicy::FullAccess;
    let result = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            format!("echo ok > {} && echo \"$HOME\"", target.display()),
        ],
    ))
    .unwrap();
    assert!(!result.sandboxed);
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(target.exists());
    let home = std::env::var("HOME").unwrap();
    assert_eq!(result.stdout.trim(), home);
}

#[test]
fn running_outside_the_sandbox_asks_for_exactly_that() {
    let action = ActionProposal::RunCommand {
        executable: "dotnet".into(),
        args: vec!["build".into()],
        stdin: None,
        cwd: None,
        outside_sandbox: true,
    };
    let (approval, question) = required_approval(&action).unwrap();
    assert_eq!(approval, Approval::OutsideSandbox);
    assert!(question.contains("dotnet build"), "{question}");
    assert!(question.contains("outside the sandbox"), "{question}");
    // Granting something else does not lift the sandbox.
    let mut policy = policy(Path::new("/tmp"));
    policy.approvals = vec![Approval::NetworkAccess];
    assert!(policy.require(Approval::OutsideSandbox).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_command_can_still_write_inside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    let result = block_on(run_command(
        &policy,
        "sh",
        &["-c".to_string(), "echo ok > inside.txt".to_string()],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(
        fs::read_to_string(root.path().join("inside.txt"))
            .unwrap()
            .trim(),
        "ok"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_sandbox_denies_network_when_policy_does() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("curl".into());
    policy.timeout = Duration::from_secs(20);
    // The URL is assembled inside the shell so no argument contains a scheme
    // for the allowlist check to catch. What refuses here is the sandbox.
    let result = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            r#"s=htt; curl -s --max-time 8 -o /dev/null "${s}ps://example.com""#.to_string(),
        ],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert_ne!(
        result.exit_code,
        Some(0),
        "network reached from inside the sandbox"
    );
}

/// Build tooling needs a scratch area. It is given one inside the workspace
/// rather than by widening the sandbox to all of $TMPDIR, which would let one
/// task's workspace write into another's.
#[test]
fn a_child_process_gets_its_scratch_directory_inside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    let result = block_on(run_command(
        &policy,
        "sh",
        &["-c".to_string(), "printf %s \"$TMPDIR\"".to_string()],
    ))
    .unwrap();
    assert_eq!(result.exit_code, Some(0));
    let reported = std::path::PathBuf::from(result.stdout.trim());
    assert!(
        reported.starts_with(root.path().canonicalize().unwrap()),
        "scratch directory {reported:?} is outside the workspace"
    );
    assert!(reported.is_dir());
}

/// The system temp directory stays outside the boundary.
#[cfg(target_os = "macos")]
#[test]
fn the_system_temp_directory_stays_unwritable_inside_the_sandbox() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    let target = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join("pwr-should-not-exist.txt");
    let _ = fs::remove_file(&target);
    let result = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            format!("echo pwned > {}", target.display()),
        ],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert!(!target.exists(), "system temp directory was writable");
}

/// Package managers keep caches and config under HOME, so the child's HOME
/// points into its workspace. That keeps downloads inside the boundary and
/// makes a run hermetic; the real home is a different directory and stays
/// unwritable.
#[test]
fn a_child_process_gets_its_home_inside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    let result = block_on(run_command(
        &policy,
        "sh",
        &["-c".to_string(), "printf %s \"$HOME\"".to_string()],
    ))
    .unwrap();
    let reported = PathBuf::from(result.stdout.trim());
    assert!(reported.starts_with(root.path().canonicalize().unwrap()));
    assert_ne!(reported, PathBuf::from(std::env::var("HOME").unwrap()));
}

#[cfg(target_os = "macos")]
#[test]
fn the_real_home_directory_stays_unwritable_inside_the_sandbox() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    // Resolved here rather than read from $HOME inside the child, which now
    // points into the workspace.
    let target = PathBuf::from(std::env::var("HOME").unwrap()).join("pwr-should-not-exist.txt");
    let _ = fs::remove_file(&target);
    let result = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            format!("echo pwned > {}", target.display()),
        ],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert_ne!(result.exit_code, Some(0));
    assert!(!target.exists(), "the real home directory was writable");
}

#[test]
fn a_result_always_records_whether_it_was_sandboxed() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.sandbox = SandboxPolicy::Disabled;
    let result = block_on(run_command(&policy, "echo", &["hi".to_string()])).unwrap();
    // An unsandboxed run must be visibly unsandboxed, never silently so.
    assert!(!result.sandboxed);
}

#[test]
fn a_required_sandbox_fails_closed_when_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.sandbox = SandboxPolicy::Required;
    // Non-canonicalisable root: no profile can be built for it.
    policy.root = root.path().join("missing");
    let denied = block_on(run_command(&policy, "echo", &["hi".to_string()]));
    assert!(denied.is_err());
}

// ------------------------------------------------------- network access

/// The project's own policy gates network activation on approval rather than
/// forbidding it. Dependency resolution needs the network; so does
/// exfiltration, and an unattended agent reading an untrusted repository is
/// the case the grant exists to make deliberate.
#[test]
fn the_network_is_closed_until_it_is_granted() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    assert!(!policy.network_allowed());
    policy.approvals = vec![Approval::NetworkAccess];
    assert!(policy.network_allowed());
    // A different grant does not open it.
    policy.approvals = vec![Approval::DependencyChange, Approval::Publish];
    assert!(!policy.network_allowed());
}

#[test]
fn an_ungranted_run_cannot_name_a_url_in_a_command() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    assert!(
        block_on(run_command(
            &policy,
            "echo",
            &["https://example.com".to_string()]
        ))
        .is_err()
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_sandbox_opens_egress_only_with_the_grant() {
    let root = tempfile::tempdir().unwrap();
    let fetch = |policy: &ToolPolicy| {
        block_on(run_command(
            policy,
            "sh",
            &[
                "-c".to_string(),
                r#"s=htt; curl -s --max-time 10 -o /dev/null "${s}ps://example.com""#.to_string(),
            ],
        ))
        .unwrap()
    };
    let denied = fetch(&policy(root.path()));
    assert!(denied.sandboxed);
    assert_ne!(denied.exit_code, Some(0), "egress without a grant");

    let mut granted_policy = policy(root.path());
    granted_policy.approvals = vec![Approval::NetworkAccess];
    let granted = fetch(&granted_policy);
    assert!(granted.sandboxed);
    assert_eq!(
        granted.exit_code,
        Some(0),
        "granted egress was still blocked: {}",
        granted.stderr
    );
}

/// A network grant must not become a filesystem grant.
#[cfg(target_os = "macos")]
#[test]
fn a_network_grant_does_not_widen_the_filesystem_boundary() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.approvals = vec![Approval::NetworkAccess];
    let target = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join("pwr-network-grant-probe.txt");
    let _ = fs::remove_file(&target);
    let result = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".to_string(),
            format!("echo pwned > {}", target.display()),
        ],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert!(!target.exists());
}

// ------------------------------------------------------------------ reads

/// The sandbox confined writes and left reads open. With `--provision`
/// granting an arbitrary executable and a network together, that is the shape
/// of an exfiltration, and the nine denied credential paths narrowed it rather
/// than closing it.
///
/// This fixture first proves the file is readable *without* the sandbox. Three
/// fixtures in this project have passed for a reason unrelated to what they
/// tested -- one aimed at a `~/.ssh` that did not exist and reported "no such
/// file" as a denial while a mutant removing the rule survived.
#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_command_cannot_read_a_file_outside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().canonicalize().unwrap().join("private.txt");
    fs::write(&secret, "the contents nobody outside should see").unwrap();

    // Without the sandbox this succeeds, which is what makes the denial below
    // evidence of the sandbox rather than of a missing file.
    let unsandboxed = ToolPolicy {
        sandbox: SandboxPolicy::Disabled,
        ..policy(root.path())
    };
    let open = block_on(run_command(
        &unsandboxed,
        "sh",
        &["-c".to_string(), format!("cat {}", secret.display())],
    ))
    .unwrap();
    assert_eq!(open.exit_code, Some(0), "the file was not readable at all");
    assert!(open.stdout.contains("nobody outside"));

    let result = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), format!("cat {}", secret.display())],
    ))
    .unwrap();
    assert!(result.sandboxed, "the fixture did not exercise a sandbox");
    assert_ne!(result.exit_code, Some(0), "the sandbox read it: {result:?}");
    assert!(
        !result.stdout.contains("nobody outside"),
        "contents leaked: {}",
        result.stdout
    );
}

/// A directory outside the workspace cannot be listed either. Names are
/// evidence on their own -- a repository list, a client list, a filename that
/// says what a person is working on.
#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_command_cannot_list_a_directory_outside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside = outside.path().canonicalize().unwrap();
    fs::write(outside.join("acquisition-notes.md"), "x").unwrap();

    let result = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), format!("ls {}", outside.display())],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert!(
        !result.stdout.contains("acquisition-notes"),
        "directory names leaked: {}",
        result.stdout
    );
}

/// The failure mode of a strict profile is that nothing runs at all: denying
/// every read denies the linker its cache and `/usr/bin/true` aborts before
/// `main`. The boundary is only worth having if the toolchain still works, so
/// that is asserted with a real command rather than by reading the profile.
#[cfg(target_os = "macos")]
#[test]
fn the_toolchain_still_runs_under_the_read_boundary() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("in.txt"), "workspace contents").unwrap();
    let result = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), "cat in.txt && git --version".to_string()],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert_eq!(
        result.exit_code,
        Some(0),
        "the read boundary broke the toolchain: {result:?}"
    );
    // The workspace itself stays readable, which is the whole point of the
    // agent being there.
    assert!(result.stdout.contains("workspace contents"));
    assert!(result.stdout.contains("git version"));
}

/// Found by watching a real run spend a fifth of its budget on `ls .pwr`
/// and `cat` of an index artifact before it had installed anything.
///
/// `list_tree` and `search` exclude the harness's state through the shared
/// walker, but a command does not go through that walker. The agent's
/// workspace is the project, not the records the harness keeps about it.
#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_command_cannot_read_the_harness_state() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join(".pwr");
    fs::create_dir_all(state.join("indexes")).unwrap();
    fs::write(state.join("indexes/idx.json"), "harness bookkeeping").unwrap();
    fs::write(root.path().join("real.rs"), "fn real() {}").unwrap();

    let result = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), "cat .pwr/indexes/idx.json".to_string()],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert!(
        !result.stdout.contains("harness bookkeeping"),
        "the agent read the harness's own records: {}",
        result.stdout
    );

    // The project itself is untouched by the denial.
    let project = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), "cat real.rs".to_string()],
    ))
    .unwrap();
    assert_eq!(project.exit_code, Some(0));
    assert!(project.stdout.contains("fn real"));
}

/// The read denial left the state writable: the workspace write allowance
/// covers `.pwr` and nothing followed it. Measured before the fix: `echo x >
/// .pwr/probe` and `rm .pwr/indexes/idx.json` both succeeded under the
/// sandbox. Edit capabilities are refused on protected paths; a command must
/// not reach the event store by another route.
#[cfg(target_os = "macos")]
#[test]
fn a_sandboxed_command_cannot_write_the_harness_state() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join(".pwr");
    fs::create_dir_all(state.join("indexes")).unwrap();
    fs::write(state.join("indexes/idx.json"), "harness bookkeeping").unwrap();

    for command in [
        "echo x > .pwr/probe",
        "rm .pwr/indexes/idx.json",
        "rm -rf .pwr/indexes",
    ] {
        let result = block_on(run_command(
            &policy(root.path()),
            "sh",
            &["-c".to_string(), command.to_string()],
        ))
        .unwrap();
        assert!(result.sandboxed);
        assert_ne!(
            result.exit_code,
            Some(0),
            "`{command}` succeeded: {result:?}"
        );
    }
    assert!(!state.join("probe").exists());
    assert_eq!(
        fs::read_to_string(state.join("indexes/idx.json")).unwrap(),
        "harness bookkeeping"
    );

    // The workspace and the child's own scratch directory, whose name shares
    // the prefix, stay writable.
    let project = block_on(run_command(
        &policy(root.path()),
        "sh",
        &[
            "-c".to_string(),
            "echo written > real.rs && echo scratch > .pwr-scratch/probe".to_string(),
        ],
    ))
    .unwrap();
    assert_eq!(project.exit_code, Some(0), "{project:?}");
    assert_eq!(
        fs::read_to_string(root.path().join("real.rs")).unwrap(),
        "written\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join(".pwr-scratch/probe")).unwrap(),
        "scratch\n"
    );
}

/// A repository's own checks walk the whole workspace, and a directory they
/// cannot list stops them. Observed on 2026-09-14 in the R2 pilot: `npx --no
/// ava` and `npm test` on filenamify and slugify crashed with `EPERM: operation
/// not permitted, scandir '.pwr'` in every arm, so their visible verifier
/// could never pass. Listing is allowed; reading a record still is not.
#[cfg(target_os = "macos")]
#[test]
fn a_command_that_walks_the_workspace_is_not_stopped_by_the_harness_state() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join(".pwr");
    fs::create_dir_all(state.join("indexes")).unwrap();
    fs::write(state.join("indexes/idx.json"), "harness bookkeeping").unwrap();
    fs::write(root.path().join("real.rs"), "fn real() {}").unwrap();

    let walked = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), "find . -type f".to_string()],
    ))
    .unwrap();
    assert!(walked.sandboxed);
    assert_eq!(walked.exit_code, Some(0), "{walked:?}");
    assert!(walked.stdout.contains("real.rs"), "{walked:?}");

    let read = block_on(run_command(
        &policy(root.path()),
        "sh",
        &["-c".to_string(), "cat .pwr/indexes/idx.json".to_string()],
    ))
    .unwrap();
    assert_ne!(read.exit_code, Some(0), "{read:?}");
    assert!(!read.stdout.contains("harness bookkeeping"), "{read:?}");
}

/// The scratch directory is deliberately readable: it is the child's own HOME
/// and TMPDIR, and denying it would break the tools that were pointed at it.
#[cfg(target_os = "macos")]
#[test]
fn the_childs_own_scratch_directory_stays_readable() {
    let root = tempfile::tempdir().unwrap();
    let result = block_on(run_command(
        &policy(root.path()),
        "sh",
        &[
            "-c".to_string(),
            "echo written > \"$TMPDIR/probe.txt\" && cat \"$TMPDIR/probe.txt\"".to_string(),
        ],
    ))
    .unwrap();
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(result.stdout.contains("written"));
}

/// `args` read as the whole argv, with the program repeated at the front.
///
/// Measured on an Angular build: `npm` with args `["npm", "run", "build"]`
/// executed `npm npm run build` and came back as npm's own
/// `Unknown command: "npm"`. Refusing it by name did not stop it: 33 of B1's
/// actions in the R2 pilot traces were that refusal. The repeat is dropped and
/// the command runs as meant; a repeat with nothing after it is still refused.
#[tokio::test]
async fn a_repeated_program_name_is_dropped_and_the_command_runs() {
    let workspace = tempfile::tempdir().unwrap();
    let mut policy = pwr_tools::PolicyProfile::Development.build(workspace.path().to_path_buf());
    policy.allow_commands.push("echo".into());

    let ran = pwr_tools::run_command(&policy, "echo", &["echo".to_string(), "hello".to_string()])
        .await
        .expect("a repeated program name runs the command it meant");
    assert_eq!(ran.stdout.trim(), "hello", "{ran:?}");

    let bare = pwr_tools::run_command(&policy, "echo", &["echo".to_string()])
        .await
        .expect_err("the program repeated alone is not a command");
    assert!(
        bare.to_string().contains("must not repeat the program"),
        "{bare}"
    );
}

/// A program whose own name is a legitimate first argument must still run: the
/// guard is about a repeated argv[0], not about the word appearing anywhere.
#[tokio::test]
async fn an_argument_that_merely_mentions_the_program_is_not_a_repeat() {
    let workspace = tempfile::tempdir().unwrap();
    let mut policy = pwr_tools::PolicyProfile::Development.build(workspace.path().to_path_buf());
    policy.allow_commands.push("echo".into());
    let outcome = pwr_tools::run_command(&policy, "echo", &["hello echo".to_string()]).await;
    assert!(
        !matches!(&outcome, Err(error) if error.to_string().contains("repeat the program")),
        "an argument mentioning the program was read as a repeat"
    );
}

// ------------------------------------------------ shell syntax without a shell

/// `cd` run as a process changes nothing and exits 0. Measured on 2026-09-21:
/// seventy `cd site && npm run build` calls, each reported as a success, and
/// no build ever ran.
#[test]
fn a_shell_builtin_is_refused_and_names_cwd() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("cd".into());
    let refused = block_on(run_command(
        &policy,
        "cd",
        &["exec".into(), "cd".into(), "site && npm run build".into()],
    ));
    let Err(ToolError::Denied(why)) = refused else {
        panic!("cd ran: {refused:?}");
    };
    assert!(why.contains("cwd: \"site\""), "{why}");
}

#[test]
fn a_lone_shell_operator_is_refused_rather_than_passed_as_an_argument() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    let refused = block_on(run_command(
        &policy,
        "echo",
        &["built".into(), "&&".into(), "echo".into(), "tested".into()],
    ));
    let Err(ToolError::Denied(why)) = refused else {
        panic!("the operator was passed through: {refused:?}");
    };
    assert!(why.contains("`&&` is shell syntax"), "{why}");
    // Inside a shell's own script it is the script's business.
    let fine = block_on(run_command(
        &policy,
        "sh",
        &["-c".into(), "echo built && echo tested".into()],
    ))
    .unwrap();
    assert_eq!(fine.exit_code, Some(0));
}

#[test]
fn find_exec_keeps_its_terminating_semicolon() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("find".into());
    let result = block_on(run_command(
        &policy,
        "find",
        &[
            ".".into(),
            "-maxdepth".into(),
            "0".into(),
            "-exec".into(),
            "echo".into(),
            "{}".into(),
            ";".into(),
        ],
    ))
    .unwrap();
    assert_eq!(result.exit_code, Some(0), "{}", result.stderr);
}

/// `ls` with args `["exec", "ls", "-la", "dir"]`: the launcher and the program
/// repeated in front of the real arguments, learned from one `npm exec` that
/// worked and then applied to everything.
#[test]
fn an_exec_prefix_repeating_the_program_is_dropped() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("site")).unwrap();
    fs::write(root.path().join("site/index.html"), "hi").unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("ls".into());
    let result = block_on(run_command(
        &policy,
        "ls",
        &["exec".into(), "ls".into(), "-a".into(), "site".into()],
    ))
    .unwrap();
    assert_eq!(result.exit_code, Some(0), "{}", result.stderr);
    assert!(result.stdout.contains("index.html"), "{}", result.stdout);
}

#[test]
fn cwd_runs_the_program_in_a_workspace_subdirectory() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("site")).unwrap();
    fs::write(root.path().join("site/marker"), "here").unwrap();
    let policy = policy(root.path());
    let result = block_on(run_command_in(
        &policy,
        "sh",
        &["-c".into(), "cat marker".into()],
        None,
        Some("site"),
    ))
    .unwrap();
    assert_eq!(result.exit_code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout.trim(), "here");
}

#[test]
fn npx_run_reports_the_script_invocation_error_before_execution() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("npx".into());
    let result = block_on(run_command_in(
        &policy,
        "npx",
        &["run".into(), "build".into()],
        None,
        None,
    ));
    assert!(
        matches!(result, Err(ToolError::Denied(message)) if message.contains("executable `npm`")),
        "expected a concrete npm command"
    );
}

#[test]
fn npm_run_names_the_nested_project_when_cwd_is_missing() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("site")).unwrap();
    fs::write(root.path().join("site/package.json"), "{}").unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("npm".into());
    let result = block_on(run_command_in(
        &policy,
        "npm",
        &["run".into(), "build".into()],
        None,
        None,
    ));
    assert!(
        matches!(result, Err(ToolError::Denied(message)) if message.contains("cwd `site`")),
        "expected a concrete cwd hint"
    );
}

#[test]
fn cwd_cannot_leave_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    for outside in ["..", "/tmp", "missing"] {
        let refused = block_on(run_command_in(
            &policy,
            "echo",
            &["hi".into()],
            None,
            Some(outside),
        ));
        assert!(
            matches!(refused, Err(ToolError::Denied(_))),
            "{outside}: {refused:?}"
        );
    }
}

// ------------------------------------------------------ reference folders

/// A workspace nested in a project reads the project's documents when, and
/// only when, the project is declared as a reference folder -- and never
/// writes there, nor reads its harness state or secrets.
#[test]
fn a_declared_reference_folder_is_readable_and_nothing_more() {
    let project = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join("docs")).unwrap();
    fs::create_dir_all(project.path().join(".pwr")).unwrap();
    fs::create_dir_all(project.path().join("site")).unwrap();
    fs::write(project.path().join("docs/guide.md"), "the guide").unwrap();
    fs::write(project.path().join(".pwr/state"), "private").unwrap();
    fs::write(project.path().join(".env"), "SECRET=1").unwrap();
    let mut policy = policy(&project.path().join("site"));

    assert!(read_file(&policy, Path::new("../docs/guide.md")).is_err());

    policy.extra_readable = vec![project.path().to_path_buf()];
    let read = read_file(&policy, Path::new("../docs/guide.md")).unwrap();
    assert!(read.content.contains("the guide"));
    for private in ["../.pwr/state", "../.env"] {
        assert!(read_file(&policy, Path::new(private)).is_err(), "{private}");
    }
    assert!(write_file(&policy, Path::new("../docs/new.md"), "x").is_err());

    // Out through the parent and back in: the workspace's own file.
    fs::write(project.path().join("site/notes.md"), "own notes").unwrap();
    let own = read_file(&policy, Path::new("../site/notes.md")).unwrap();
    assert!(own.content.contains("own notes"));
    assert!(!project.path().join("docs/new.md").exists());

    // A command aimed at the reference is refused with the way in: read_file.
    let refused = block_on(run_command_in(&policy, "ls", &[], None, Some("../docs")));
    let Err(ToolError::Denied(why)) = refused else {
        panic!("ran in the reference folder: {refused:?}");
    };
    assert!(why.contains("read-only reference folder"), "{why}");
    assert!(why.contains("read_file"), "{why}");
}

// ------------------------------------------- servers and loose edit matching

/// A server run through `run_command` only waits for the timeout.
#[test]
fn a_command_that_serves_forever_is_sent_to_start_service() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy
        .allow_commands
        .extend(["python3".into(), "npm".into()]);
    for (program, args) in [
        ("python3", vec!["-m", "http.server", "8000"]),
        ("npm", vec!["run", "dev"]),
        ("npm", vec!["start"]),
    ] {
        let args: Vec<String> = args.into_iter().map(String::from).collect();
        let refused = block_on(run_command(&policy, program, &args));
        let Err(ToolError::Denied(why)) = refused else {
            panic!("{program} {args:?} ran: {refused:?}");
        };
        assert!(why.contains("start_service"), "{why}");
    }
    // A build is not a server.
    let build = block_on(run_command(&policy, "npm", &["run".into(), "build".into()]));
    assert!(!matches!(build, Err(ToolError::Denied(ref why)) if why.contains("start_service")));
}

/// A hunk that dropped the blank line inside the block it copied still
/// edits that block, when the block is unique.
#[test]
fn an_edit_matches_its_block_despite_a_dropped_blank_line() {
    let root = tempfile::tempdir().unwrap();
    let source =
        "fn a() {\n    const bottom = y + h - 1;\n\n    for (let i = 0; i < 3; i++) {}\n}\n";
    fs::write(root.path().join("a.js"), source).unwrap();
    let policy = policy(root.path());
    let hash = pwr_domain::hash_bytes(source);
    let result = apply_patch(
        &policy,
        Path::new("a.js"),
        &hash,
        &[Hunk {
            find: "const bottom = y + h - 1;\n    for (let i = 0; i < 3; i++) {}".into(),
            replace: "const bottom = y + h - 0.001;\n\n    for (let i = 0; i < 3; i++) {}".into(),
        }],
    )
    .unwrap();
    assert!(result.normalized.is_some());
    let edited = fs::read_to_string(root.path().join("a.js")).unwrap();
    assert_eq!(
        edited,
        "fn a() {\n    const bottom = y + h - 0.001;\n\n    for (let i = 0; i < 3; i++) {}\n}\n"
    );
    // Ambiguous blocks are still refused.
    let twice = "x = 1;\n\ny = 2;\nx = 1;\ny = 2;\n";
    fs::write(root.path().join("b.js"), twice).unwrap();
    let refused = replace_text(
        &policy,
        Path::new("b.js"),
        &pwr_domain::hash_bytes(twice),
        "x = 1;\ny = 2;\n\n",
        "z;",
    );
    assert!(refused.is_err());
}

/// `echo "ls -la"` prints and succeeds; `sh "ls -la"` looks for a script.
/// Neither runs the command it names, and both are refused with the way to.
#[test]
fn a_command_passed_to_echo_or_a_bare_shell_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("bash".into());
    for (program, arg) in [
        ("echo", "ls -la src"),
        ("sh", "ls -la"),
        ("bash", "cat package.json"),
    ] {
        let refused = block_on(run_command(&policy, program, &[arg.to_string()]));
        let Err(ToolError::Denied(why)) = refused else {
            panic!("{program} {arg:?} ran: {refused:?}");
        };
        assert!(why.contains("not run") || why.contains("-c"), "{why}");
    }
    // Ordinary echo and a real -c script still run.
    assert!(block_on(run_command(&policy, "echo", &["hello world".into()])).is_ok());
    assert!(block_on(run_command(&policy, "sh", &["-c".into(), "ls -la".into()])).is_ok());
}

// ------------------------------------------------------- long documents

/// A long Markdown document asked for whole comes back as its outline and its
/// opening; a window of it, and code of any length, come back as asked
/// (D.E2E-15).
#[test]
fn a_long_document_read_whole_returns_its_outline() {
    let root = tempfile::tempdir().unwrap();
    let mut doc = String::from("# Roadmap\n\nIntro.\n");
    for section in 0..40 {
        doc.push_str(&format!("\n## Section {section}\n\n"));
        doc.push_str("```sh\n# not a heading\n```\n");
        for _ in 0..12 {
            doc.push_str("A line of prose that fills the section with some text.\n");
        }
    }
    assert!(doc.len() as u64 > LONG_DOCUMENT_BYTES);
    fs::write(root.path().join("roadmap.md"), &doc).unwrap();
    fs::write(root.path().join("big.rs"), doc.replace('#', "//")).unwrap();
    let policy = policy(root.path());

    let whole = read_file_for_model(&policy, Path::new("roadmap.md"), None, None).unwrap();
    let outline = whole.outline.expect("an outline");
    assert!(outline.contains("## Section 39"), "{outline}");
    assert!(!outline.contains("not a heading"), "{outline}");
    assert!(whole.truncated);
    assert_eq!(whole.content.lines().count(), 80);

    let line = outline
        .lines()
        .find(|line| line.ends_with("## Section 39"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let section =
        read_file_for_model(&policy, Path::new("roadmap.md"), Some(line), Some(5)).unwrap();
    assert!(
        section.content.starts_with("## Section 39"),
        "{}",
        section.content
    );
    assert!(section.outline.is_none());

    let code = read_file_for_model(&policy, Path::new("big.rs"), None, None).unwrap();
    assert!(code.outline.is_none());
    // Up to the output limit, not cut to the document head.
    assert!(code.content.lines().count() > 80);
}

// ------------------------------------------------- installed dependencies

/// The packages a project declares are searchable where they are installed,
/// and only those: the version on disk is the one the project builds against
/// (backlog C.12, installed dependencies before the web).
#[test]
fn installed_dependencies_are_searchable_and_readable_but_never_writable() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        "{\n  \"name\": \"app\",\n  \"dependencies\": {\n    \"left-pad\": \"^1.0.0\"\n  }\n}\n",
    )
    .unwrap();
    for package in ["left-pad", "unused-by-the-project"] {
        fs::create_dir_all(root.path().join("node_modules").join(package)).unwrap();
        fs::write(
            root.path()
                .join("node_modules")
                .join(package)
                .join("index.js"),
            "export function pad(text) { return TARGET_TOKEN + text; }\n",
        )
        .unwrap();
    }
    fs::write(
        root.path().join("Cargo.lock"),
        "[[package]]\nname = \"widget\"\nversion = \"2.1.0\"\n",
    )
    .unwrap();
    let crate_dir = home
        .path()
        .join("registry/src/index.crates.io-1/widget-2.1.0/src");
    fs::create_dir_all(&crate_dir).unwrap();
    fs::write(
        crate_dir.join("lib.rs"),
        "pub fn widget() { TARGET_TOKEN }\n",
    )
    .unwrap();

    let mut reading = policy(root.path());
    reading.extra_readable = dependency_roots(root.path());

    let found = search_dependencies(&reading, &SearchQuery::literal("TARGET_TOKEN", 20)).unwrap();
    let paths: Vec<&str> = found.files.iter().map(|file| file.path.as_str()).collect();
    assert!(
        paths.iter().any(|path| path.contains("left-pad")),
        "{paths:?}"
    );
    // Declared, not merely present.
    assert!(
        !paths
            .iter()
            .any(|path| path.contains("unused-by-the-project")),
        "{paths:?}"
    );

    // A passage found this way can then be read, and never written.
    let hit = found.files[0].path.clone();
    let read = read_file(&reading, Path::new(&hit)).unwrap();
    assert!(read.content.contains("TARGET_TOKEN"));
    assert!(write_file(&reading, Path::new(&hit), "no").is_err());

    // The cargo half: the versions Cargo.lock pins, unpacked in the registry.
    let sources = dependency_sources_with_cargo_home(root.path(), Some(home.path()));
    let labels: Vec<&str> = sources.iter().map(|source| source.label.as_str()).collect();
    assert!(labels.contains(&"cargo/widget-2.1.0"), "{labels:?}");
    assert!(labels.contains(&"node_modules/left-pad"), "{labels:?}");

    // A workspace with nothing installed says so rather than returning nothing.
    let bare = tempfile::tempdir().unwrap();
    let refused = search_dependencies(&policy(bare.path()), &SearchQuery::literal("x", 5));
    let Err(ToolError::Denied(why)) = refused else {
        panic!("{refused:?}");
    };
    assert!(why.contains("no installed dependencies"), "{why}");
}

/// A run cannot make the checks pass by editing the library they are about.
///
/// Measured 2026-09-23 on the in-house-package task: a run changed one
/// character inside `node_modules/@acme/ledger-ids` so the expected string came
/// out, and the audit recorded the task as verified.
#[test]
fn an_installed_dependency_cannot_be_edited_without_the_approval() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("node_modules/@acme/ledger-ids");
    fs::create_dir_all(&library).unwrap();
    fs::write(
        library.join("index.js"),
        "export const ALPHABET = \"ABC\";\n",
    )
    .unwrap();
    fs::create_dir_all(
        root.path()
            .join(".venv/lib/python3.13/site-packages/requests"),
    )
    .unwrap();
    let mut policy = policy(root.path());
    policy
        .approvals
        .retain(|grant| *grant != Approval::DependencyChange);

    for path in [
        "node_modules/@acme/ledger-ids/index.js",
        ".venv/lib/python3.13/site-packages/requests/api.py",
        "vendor/serde/src/lib.rs",
    ] {
        let refused = write_file(&policy, Path::new(path), "tampered");
        let Err(ToolError::Denied(why)) = refused else {
            panic!("{path} was writable: {refused:?}");
        };
        assert!(why.contains("installed dependencies"), "{why}");
    }
    // Reading it is the whole point of searching it.
    let read = read_file(&policy, Path::new("node_modules/@acme/ledger-ids/index.js")).unwrap();
    assert!(read.content.contains("ALPHABET"));
    // The project's own files are untouched by this.
    write_file(&policy, Path::new("src/app.js"), "fine").unwrap();
    // And a run granted the dependency change may still do it.
    policy.approvals.push(Approval::DependencyChange);
    write_file(
        &policy,
        Path::new("node_modules/@acme/other/index.js"),
        "ok",
    )
    .unwrap();
}

/// Where no sandbox can be applied, a command is refused rather than run with
/// the person's full rights (backlog R.3). A root the Seatbelt profile cannot
/// express -- a quote in its path -- has no sandbox on macOS either, which is
/// how the platforms without an adapter behave everywhere.
#[test]
fn a_command_that_cannot_be_confined_is_refused_by_default() {
    if std::env::var("PWR_ALLOW_UNCONFINED").ok().as_deref() == Some("1") {
        common::skip("PWR_ALLOW_UNCONFINED=1: the person opted out on this machine");
        return;
    }
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("has\"quote");
    fs::create_dir_all(&root).unwrap();
    let mut policy = policy(&root);
    policy.sandbox = SandboxPolicy::Preferred;
    policy.allow_commands.push("true".into());
    assert!(!matches!(policy.will_sandbox(), Ok(true)));
    let refused = block_on(run_command(&policy, "true", &[]));
    let Err(ToolError::Denied(why)) = refused else {
        panic!("an unconfined command ran: {refused:?}");
    };
    assert!(why.contains("PWR_ALLOW_UNCONFINED"), "{why}");
}

/// A toolchain installed in the workspace is the one a command finds, for the
/// run and for the checks alike, because both start through `prepare_command`.
#[cfg(target_os = "macos")]
#[test]
fn a_toolchain_in_the_workspace_is_on_path_inside_the_sandbox() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join(".toolchains/fakelang/bin");
    fs::create_dir_all(&bin).unwrap();
    let program = bin.join("fakelang");
    fs::write(&program, "#!/bin/sh\necho fakelang 1.0\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("fakelang".into());
    let result = block_on(run_command(&policy, "fakelang", &["--version".into()])).unwrap();
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(result.sandboxed);
    assert!(result.stdout.contains("fakelang 1.0"), "{result:?}");
}

/// Measured 2026-09-26: every `dotnet` command in the sandbox died with EPERM
/// on /tmp/.dotnet/shm, the mutex NuGet's first-run migrations take -- and with
/// HOME in the workspace every run is a first run.
#[cfg(target_os = "macos")]
#[test]
fn dotnet_runs_in_the_sandbox_on_its_first_run() {
    let user_dotnet = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".dotnet/dotnet"));
    let dotnet = user_dotnet.filter(|path| path.is_file()).or_else(|| {
        [
            "/usr/local/share/dotnet/dotnet",
            "/opt/homebrew/bin/dotnet",
            "/usr/local/bin/dotnet",
        ]
        .into_iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.is_file())
    });
    let Some(dotnet) = dotnet else {
        common::skip("no .NET on this machine");
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy
        .allow_commands
        .push(dotnet.to_string_lossy().into_owned());
    let result = block_on(run_command(
        &policy,
        dotnet.to_str().unwrap(),
        &["nuget".into(), "--version".into()],
    ))
    .unwrap();
    assert!(result.sandboxed);
    assert!(!result.stderr.contains("EPERM"), "{result:?}");
    assert_eq!(result.exit_code, Some(0), "{result:?}");
}

/// Measured 2026-09-29: `dotnet build` in the sandbox hung until the 120 s
/// limit. MSBuild runs its build nodes as separate processes that talk over
/// Unix sockets in `TMPDIR`; `(deny network*)` refused the node's socket, the
/// node died in `NamedPipeServerStream`'s constructor, and the parent waited
/// for it. The sockets now stay allowed where `TMPDIR` points, the
/// workspace's scratch directory, and nowhere else.
#[cfg(target_os = "macos")]
#[test]
fn dotnet_build_runs_its_build_nodes_in_the_sandbox() {
    let dotnet = [
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".dotnet/dotnet")),
        Some(PathBuf::from("/usr/local/share/dotnet/dotnet")),
        Some(PathBuf::from("/opt/homebrew/bin/dotnet")),
    ]
    .into_iter()
    .flatten()
    .find(|path| path.is_file());
    let Some(dotnet) = dotnet else {
        common::skip("no .NET on this machine");
        return;
    };
    // Three projects, as a solution: one alone builds in MSBuild's own
    // process and never starts a node.
    let root = tempfile::tempdir().unwrap();
    let project = |name: &str, kind: &str, reference: Option<&str>| {
        let dir = root.path().join(name);
        fs::create_dir_all(&dir).unwrap();
        let reference = reference
            .map(|other| format!(r#"<ItemGroup><ProjectReference Include="../{other}/{other}.csproj" /></ItemGroup>"#))
            .unwrap_or_default();
        fs::write(
            dir.join(format!("{name}.csproj")),
            format!(r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>{kind}</OutputType><TargetFramework>net10.0</TargetFramework></PropertyGroup>{reference}</Project>"#),
        )
        .unwrap();
        let body = if kind == "Exe" {
            "System.Console.WriteLine(\"hi\");"
        } else {
            "public static class Marker { }"
        };
        fs::write(dir.join("Code.cs"), body).unwrap();
    };
    project("Shared", "Library", None);
    project("Client", "Library", Some("Shared"));
    project("App", "Exe", Some("Client"));
    // The solution as dotnet writes one, outside the sandbox: only the build
    // is under test.
    let setup = |args: &[&str]| {
        let status = std::process::Command::new(&dotnet)
            .args(args)
            .current_dir(root.path())
            .env("DOTNET_NOLOGO", "1")
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
    };
    setup(&["new", "sln", "--name", "Hello", "--format", "sln"]);
    setup(&[
        "sln",
        "Hello.sln",
        "add",
        "Shared/Shared.csproj",
        "Client/Client.csproj",
        "App/App.csproj",
    ]);
    let mut policy = policy(root.path());
    policy.sandbox = SandboxPolicy::Required;
    policy.timeout = Duration::from_secs(150);
    policy
        .allow_commands
        .push(dotnet.to_string_lossy().into_owned());
    // With the network closed, and granted -- Auto-approve's case, where
    // the sockets were never the problem and the write to /tmp was.
    for approvals in [vec![], vec![Approval::NetworkAccess]] {
        policy.approvals = approvals;
        let result = block_on(run_command(
            &policy,
            dotnet.to_str().unwrap(),
            &[
                "build".into(),
                "Hello.sln".into(),
                "--no-incremental".into(),
            ],
        ))
        .unwrap();
        assert!(result.sandboxed);
        assert!(
            !result.stdout.contains("NamedPipeServerStream"),
            "{result:?}"
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn dotnet_first_run_can_create_its_narrow_tmp_directory() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.sandbox = SandboxPolicy::Required;
    let result = block_on(async {
        policy
            .prepare_command(
                "/usr/bin/mktemp",
                &["-d".into(), "/tmp/.dotnet.XXXXXX".into()],
            )
            .unwrap()
            .output()
            .await
            .unwrap()
    });
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let created = PathBuf::from(String::from_utf8(result.stdout).unwrap().trim());
    let path = created.to_string_lossy();
    assert!(
        path.starts_with("/tmp/.dotnet.") || path.starts_with("/private/tmp/.dotnet."),
        "{path}"
    );
    fs::remove_dir(created).unwrap();
}

/// What the sandbox's refusal actually looks like from the tools a run
/// reaches for, so the conversation can tell it from any other failure. No
/// traffic leaves the machine: the network is denied throughout.
#[cfg(target_os = "macos")]
#[test]
fn the_sandbox_refusing_the_network_is_recognisable() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy
        .allow_commands
        .extend(["curl".into(), "python3".into()]);
    // Written so no argument names a URL: one that does is asked about before
    // it runs (`names_a_url`), and this is about what happens when it runs.
    let attempts: [(&str, Vec<String>); 3] = [
        ("curl", vec!["-sS".into(), "example.com".into()]),
        (
            "python3",
            vec![
                "-c".into(),
                "import urllib.request; urllib.request.urlopen('https:' + '//pypi.org/simple/')"
                    .into(),
            ],
        ),
        (
            "sh",
            vec![
                "-c".into(),
                "git ls-remote \"$0//github.com/rust-lang/rust\"".into(),
                "https:".into(),
            ],
        ),
    ];
    for (program, args) in attempts {
        let result = block_on(run_command(&policy, program, &args)).unwrap();
        assert!(result.sandboxed);
        assert_ne!(result.exit_code, Some(0), "{program}: {result:?}");
        let output = format!("{}\n{}", result.stdout, result.stderr);
        assert!(
            pwr_tools::looks_like_network_denied(&output),
            "{program} was refused the network and said: {output}"
        );
    }
}

/// Whether a daemon answers on `socket`: Docker's own liveness endpoint.
#[cfg(target_os = "macos")]
fn container_engine_answers(socket: &std::path::Path) -> bool {
    use std::io::{Read as _, Write as _};
    let Ok(mut stream) = std::os::unix::net::UnixStream::connect(socket) else {
        return false;
    };
    let timeout = Some(std::time::Duration::from_secs(5));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    if stream
        .write_all(b"GET /_ping HTTP/1.0\r\nHost: docker\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    reply.starts_with("HTTP/1.") && reply.trim_end().ends_with("OK")
}

/// The daemon is reachable through its socket only once the engine is
/// granted, and the client finds the plugins `build` and `compose` are.
#[cfg(target_os = "macos")]
#[test]
fn docker_reaches_its_daemon_only_when_the_engine_is_granted() {
    let Some(socket) = pwr_tools::container_socket() else {
        common::skip("no container engine socket on this machine");
        return;
    };
    // The socket file outlives the engine: Docker Desktop leaves it behind
    // when it quits, and a command then reaches nothing whatever it is
    // granted. Asked outside the sandbox, so the answer is the engine's.
    if !container_engine_answers(&socket) {
        common::skip("a container engine socket is present but no daemon answers on it");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("docker".into());
    let version = |policy: &ToolPolicy, args: &[&str]| {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        block_on(run_command(policy, "docker", &args)).unwrap()
    };
    let refused = version(&policy, &["version", "--format", "{{.Server.Version}}"]);
    assert!(refused.sandboxed);
    assert_ne!(refused.exit_code, Some(0), "{refused:?}");
    assert!(
        pwr_tools::looks_like_container_engine_denied(&format!(
            "{}{}",
            refused.stdout, refused.stderr
        )),
        "{refused:?}"
    );

    policy.approvals.push(Approval::ContainerEngine);
    let server = version(&policy, &["version", "--format", "{{.Server.Version}}"]);
    assert_eq!(server.exit_code, Some(0), "{server:?}");
    assert!(!server.stdout.trim().is_empty(), "{server:?}");
    for plugin in [["buildx", "version"], ["compose", "version"]] {
        let result = version(&policy, &plugin);
        assert_eq!(result.exit_code, Some(0), "{plugin:?}: {result:?}");
    }
}

/// A folder that is not there yet is answered with what is, so the next call
/// is not spent finding out.
#[test]
fn listing_a_missing_folder_says_what_the_root_holds() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("README.md"), "spec").unwrap();
    fs::create_dir_all(root.path().join("tests")).unwrap();
    let refused = pwr_tools::list_tree_under(&policy(root.path()), 100, Some("src")).unwrap_err();
    let said = refused.to_string();
    assert!(said.contains("does not exist yet"), "{said}");
    assert!(
        said.contains("README.md") && said.contains("tests/"),
        "{said}"
    );
}

/// The browser runs inside the sandbox and the screenshot lands where images
/// for the model are kept; a page elsewhere is refused before any browser.
#[cfg(target_os = "macos")]
#[test]
fn look_at_photographs_a_workspace_page_from_inside_the_sandbox() {
    if pwr_tools::browser_executable().is_none() {
        common::skip("no browser on this machine");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("index.html"),
        "<!doctype html><title>Preview test</title><body><h1>Look</h1><script>document.body.append('Rendered by JS'); console.error('Build error token=private123'); throw new Error('Preview exception');</script></body>",
    )
    .unwrap();
    let mut policy = policy(root.path());
    policy.timeout = Duration::from_secs(60);
    policy.approvals.push(Approval::LocalService);
    let result = block_on(pwr_tools::look_at(
        &policy,
        "index.html",
        Some(640),
        Some(400),
    ))
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(result.title.as_deref(), Some("Preview test"));
    assert!(result.text.contains("Rendered by JS"), "{:?}", result);
    assert!(
        result
            .console
            .iter()
            .any(|line| line.contains("Build error") && line.contains("[REDACTED]")),
        "{:?}",
        result.console
    );
    assert!(
        result
            .console
            .iter()
            .any(|line| line.contains("Preview exception")),
        "{:?}",
        result.console
    );
    assert!(!result.console.join("\n").contains("private123"));
    let bytes = fs::read(&result.image).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert!(
        result
            .image
            .starts_with(root.path().canonicalize().unwrap().join(".pwr/images"))
    );
    let refused = block_on(pwr_tools::look_at(
        &policy,
        "https://example.com",
        None,
        None,
    ))
    .unwrap_err();
    assert!(refused.to_string().contains("on this machine"), "{refused}");
}

// ------------------------------------------------- outside the workspace

/// Subfolders are the workspace; a parent or a sibling needs the person's
/// permission, and with it the tools reach there.
#[test]
fn a_folder_outside_the_workspace_needs_its_own_grant() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("web");
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(parent.path().join("notes.md"), "outside").unwrap();
    let mut policy = policy(&root);
    // A subfolder, and a path that leaves only to come back, need nothing.
    assert!(policy.resolve(Path::new("web")).is_ok());
    assert!(policy.resolve(Path::new("../web/web")).is_ok());
    assert!(!policy.leaves_root(Path::new("../web/web")));
    // The parent is refused without the grant, and asked about exactly.
    assert!(policy.resolve(Path::new("../notes.md")).is_err());
    let read = ActionProposal::ReadFile {
        path: "../notes.md".into(),
        first_line: None,
        max_lines: None,
    };
    let (approval, question) = leaves_workspace(&read, &policy).unwrap();
    assert_eq!(approval, Approval::OutsideWorkspace);
    assert!(question.contains("read ../notes.md"), "{question}");
    let inside = ActionProposal::ReadFile {
        path: "web/index.html".into(),
        first_line: None,
        max_lines: None,
    };
    assert!(leaves_workspace(&inside, &policy).is_none());
    // Granted, it is read and written like any path.
    policy.approvals.push(Approval::OutsideWorkspace);
    let resolved = policy.resolve(Path::new("../notes.md")).unwrap();
    assert_eq!(fs::read_to_string(resolved).unwrap(), "outside");
    let absolute = parent.path().canonicalize().unwrap().join("new.txt");
    assert!(policy.resolve(&absolute).is_ok());
}

/// `web/web` from a workspace named `web` names the subfolder `web`.
#[test]
fn a_cwd_with_the_workspace_name_in_front_says_which_folder_was_meant() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("site")).unwrap();
    let policy = policy(root.path());
    let refused = block_on(run_command_in(
        &policy,
        "echo",
        &["hi".into()],
        None,
        Some("project/site"),
    ))
    .unwrap_err();
    assert!(refused.to_string().contains("probably `site`"), "{refused}");
}

#[test]
fn frozen_paths_and_dependency_trees_are_read_only_for_commands() {
    if !cfg!(target_os = "macos") {
        common::skip("requires Seatbelt");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut policy = policy(dir.path());
    policy.protected = vec!["spec.md".into(), "specs".into()];
    for path in [
        "spec.md",
        "specs/nested.md",
        "node_modules/pkg/index.js",
        "lib/vendor/pkg/file.txt",
        ".venv/lib/site-packages/pkg/a.py",
    ] {
        let target = dir.path().join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "original").unwrap();
        let result = block_on(run_command(
            &policy,
            "sh",
            &["-c".into(), format!("echo changed > {path}")],
        ))
        .unwrap();
        assert_ne!(result.exit_code, Some(0), "{path}: {result:?}");
        assert_eq!(std::fs::read_to_string(target).unwrap(), "original");
    }
    policy.approvals.push(Approval::DependencyChange);
    let result = block_on(run_command(
        &policy,
        "sh",
        &[
            "-c".into(),
            "echo changed > node_modules/pkg/index.js".into(),
        ],
    ))
    .unwrap();
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    let result = block_on(run_command(
        &policy,
        "sh",
        &["-c".into(), "echo changed > specs/nested.md".into()],
    ))
    .unwrap();
    assert_ne!(result.exit_code, Some(0));
}

#[cfg(target_os = "macos")]
#[test]
fn commands_cannot_move_an_ancestor_of_frozen_acceptance_evidence() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("tests/nested")).unwrap();
    fs::write(dir.path().join("tests/nested/acceptance.rs"), "original").unwrap();
    let mut policy = policy(dir.path());
    policy.protected = vec!["tests/nested/acceptance.rs".into()];
    let result = block_on(run_command(
        &policy,
        "sh",
        &["-c".into(), "mv tests elsewhere".into()],
    ))
    .unwrap();
    assert_ne!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join("tests/nested/acceptance.rs")).unwrap(),
        "original"
    );
    let sibling = block_on(run_command(
        &policy,
        "sh",
        &["-c".into(), "echo allowed > tests/sibling.rs".into()],
    ))
    .unwrap();
    assert_eq!(sibling.exit_code, Some(0), "{sibling:?}");
}

/// A real local error page is captured and diagnosed, including HTTP 500.
#[cfg(target_os = "macos")]
#[test]
fn a_browser_check_reports_a_local_server_error() {
    use std::io::{Read, Write};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    if browser_executable().is_none() {
        common::skip("no browser");
        return;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let target = format!(
        "http://127.0.0.1:{}/",
        listener.local_addr().unwrap().port()
    );
    let done = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&done);
    let server = std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if let Ok((mut stream, _)) = listener.accept() {
                std::thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let _ = stream.read(&mut [0; 4096]);
                    let body = "<!doctype html><title>Build Error</title><h1>Module not found</h1><script>console.error('Missing globals.css');</script>";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                });
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    });
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.timeout = Duration::from_secs(60);
    // A missing local-service grant is refused before any browser is started.
    assert!(block_on(look_at(&policy, &target, None, None)).is_err());
    policy.approvals.push(Approval::LocalService);
    let full_root = tempfile::tempdir().unwrap();
    let mut full = policy.clone();
    full.root = full_root.path().to_path_buf();
    full.sandbox = SandboxPolicy::FullAccess;
    let full_result = block_on(look_at(&full, &target, None, None));
    let clean_root = tempfile::tempdir().unwrap();
    let mut clean = policy.clone();
    clean.root = clean_root.path().to_path_buf();
    clean.sandbox = SandboxPolicy::Disabled;
    let clean_result = block_on(look_at(&clean, &target, None, None));
    let result = block_on(look_at(&policy, &target, None, None));
    done.store(true, Ordering::Relaxed);
    server.join().unwrap();
    for (mode, result) in [
        ("full", full_result),
        ("clean", clean_result),
        ("sandbox", result),
    ] {
        // The tool itself says this failure is the host's and proves nothing
        // about the page: a runner with no display to initialise (CI run
        // 37513133681), or a machine too busy to (seen locally under load).
        if let Err(error) = &result
            && error.to_string().contains("CVDisplayLink")
        {
            common::skip("the host could not initialise a display for the browser (CVDisplayLink)");
            return;
        }
        let result = result.unwrap();
        assert_eq!(result.status, Some(500), "{mode}: {result:?}");
        assert!(result.text.contains("Module not found"), "{:?}", result);
        assert!(
            result
                .console
                .iter()
                .any(|line| line.contains("Missing globals.css")),
            "{:?}",
            result.console
        );
        assert!(result.bytes > 0 && result.image.is_file());
    }
}

/// The desktop rollback battery tried `npm install` without a manifest edit.
/// The sandbox refused node_modules, but the owner never got the dependency
/// question that could authorize it. Grant dependency writes before installing.
#[test]
fn dependency_installers_need_the_dependency_grant() {
    for (program, args) in [
        ("npm", vec!["install"]),
        ("/opt/homebrew/bin/npm", vec!["ci", "--offline"]),
        ("pnpm", vec!["add", "local-package"]),
        ("yarn", vec![]),
        ("bun", vec!["remove", "local-package"]),
        ("composer", vec!["install"]),
    ] {
        let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
        assert_eq!(
            command_approval(program, &args),
            Some(Approval::DependencyChange)
        );
    }
    for (program, args) in [
        ("npm", vec!["test"]),
        ("npm", vec!["run", "build"]),
        ("yarn", vec!["test"]),
        ("npm", vec!["--version"]),
        ("cargo", vec!["test"]),
    ] {
        let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
        assert_eq!(command_approval(program, &args), None);
    }
}

#[test]
fn a_local_npm_install_is_denied_then_runs_with_approval_in_the_sandbox() {
    if !cfg!(target_os = "macos") {
        common::skip("this installation probe requires the macOS sandbox");
        return;
    }
    if std::process::Command::new("npm")
        .arg("--version")
        .output()
        .is_err()
    {
        common::skip("npm is unavailable");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("package.json"), r#"{"name":"approval-probe","version":"1.0.0","dependencies":{"local-package":"file:./local-package"}}"#).unwrap();
    fs::create_dir(root.path().join("local-package")).unwrap();
    fs::write(
        root.path().join("local-package/package.json"),
        r#"{"name":"local-package","version":"1.0.0","main":"index.js"}"#,
    )
    .unwrap();
    fs::write(
        root.path().join("local-package/index.js"),
        "module.exports = 42;\n",
    )
    .unwrap();
    let mut policy = policy(root.path());
    policy.allow_commands.push("npm".into());
    let args = [
        "install",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
    ]
    .map(str::to_owned);
    let denied = block_on(run_command(&policy, "npm", &args));
    assert!(
        matches!(denied, Err(ToolError::Denied(ref why)) if why.contains("DependencyChange")),
        "{denied:?}"
    );
    assert!(!root.path().join("node_modules").exists());
    policy.approvals.push(Approval::DependencyChange);
    let installed = block_on(run_command(&policy, "npm", &args)).unwrap();
    assert_eq!(installed.exit_code, Some(0), "{installed:?}");
    assert!(installed.sandboxed, "{installed:?}");
    assert!(
        root.path()
            .join("node_modules/local-package/index.js")
            .is_file()
    );
}
