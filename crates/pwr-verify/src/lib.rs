//! Deterministic verification baselines and bounded recovery taxonomy.
pub mod web;
use pwr_domain::{hash_bytes, now};
use pwr_tools::{ToolError, ToolPolicy, ToolResult, run_command};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationBaseline {
    pub id: pwr_domain::Id,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    pub checks: Vec<CheckRecord>,
    pub environment_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRecord {
    pub command: String,
    pub result: ToolResult,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub previous_baseline_id: pwr_domain::Id,
    pub current_baseline_id: pwr_domain::Id,
    pub new_failures: Vec<String>,
    pub regression_free: bool,
}
/// `PartialEq` because a classification that cannot be compared forces every
/// assertion about it into `matches!`, which says "assertion failed" and not
/// which class it saw -- and the whole value of these is knowing which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureClass {
    Compilation,
    Assertion,
    Environment,
    Provider,
    Policy,
    NonDeterminism,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryDecision {
    EditAndRetry { remaining_edit_verify_cycles: u8 },
    RetryContextTier { remaining_context_retries: u8 },
    Stop { reason: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryBudget {
    pub max_edit_verify_cycles: u8,
    pub max_context_retries: u8,
}
impl Default for RecoveryBudget {
    fn default() -> Self {
        Self {
            max_edit_verify_cycles: 3,
            max_context_retries: 1,
        }
    }
}

/// A build system PWR can recognise, as data rather than as a branch.
///
/// Adding a language is adding a row. A registry keyed on marker files is the
/// only shape that satisfies "verification appropriate to the repository"
/// without the set of repositories being decided here.
pub struct BuildSystem {
    pub name: &'static str,
    /// A file whose presence identifies the project. Checked in order, so more
    /// specific markers precede more general ones.
    pub marker: &'static str,
    /// The toolchain binary. Also what the run must be allowed to execute:
    /// a project whose own tools are denied cannot be verified.
    pub executable: &'static str,
    /// Arguments for a narrow check, run after each edit.
    pub targeted: &'static [&'static str],
    /// Arguments for the full suite.
    pub full: &'static [&'static str],
}

/// Recognised build systems, most specific marker first.
///
/// A repository matching none of these is not silently unverifiable: it can
/// declare its own checks, and if it does neither the run records that it had
/// no verifier rather than proceeding as though it passed.
pub const BUILD_SYSTEMS: &[BuildSystem] = &[
    BuildSystem {
        name: "docker-compose",
        marker: "compose.yaml",
        executable: "docker",
        targeted: &["compose", "config", "-q"],
        full: &["compose", "config", "-q"],
    },
    BuildSystem {
        name: "docker-compose-legacy-name",
        marker: "docker-compose.yml",
        executable: "docker",
        targeted: &["compose", "config", "-q"],
        full: &["compose", "config", "-q"],
    },
    BuildSystem {
        name: "cargo",
        marker: "Cargo.toml",
        executable: "cargo",
        targeted: &["test", "--workspace", "--lib"],
        full: &["test", "--workspace"],
    },
    BuildSystem {
        name: "go",
        marker: "go.mod",
        executable: "go",
        targeted: &["test", "./..."],
        full: &["test", "./..."],
    },
    BuildSystem {
        name: "maven",
        marker: "pom.xml",
        executable: "mvn",
        targeted: &["-q", "test"],
        full: &["-q", "verify"],
    },
    BuildSystem {
        name: "gradle",
        marker: "build.gradle",
        executable: "gradle",
        targeted: &["test"],
        full: &["build"],
    },
    BuildSystem {
        name: "gradle-kotlin",
        marker: "build.gradle.kts",
        executable: "gradle",
        targeted: &["test"],
        full: &["build"],
    },
    BuildSystem {
        name: "dotnet",
        marker: "global.json",
        executable: "dotnet",
        targeted: &["test", "--nologo"],
        full: &["test", "--nologo"],
    },
    BuildSystem {
        name: "swift",
        marker: "Package.swift",
        executable: "swift",
        targeted: &["test"],
        full: &["test"],
    },
    BuildSystem {
        name: "flutter",
        marker: "pubspec.yaml",
        executable: "flutter",
        targeted: &["test"],
        full: &["test"],
    },
    BuildSystem {
        name: "elixir",
        marker: "mix.exs",
        executable: "mix",
        targeted: &["test"],
        full: &["test"],
    },
    BuildSystem {
        name: "poetry",
        marker: "poetry.lock",
        executable: "poetry",
        targeted: &["run", "pytest", "-q"],
        full: &["run", "pytest"],
    },
    BuildSystem {
        name: "python",
        marker: "pyproject.toml",
        executable: "pytest",
        targeted: &["-q"],
        full: &[],
    },
    BuildSystem {
        name: "python-legacy",
        marker: "setup.py",
        executable: "pytest",
        targeted: &["-q"],
        full: &[],
    },
    BuildSystem {
        name: "python-requirements",
        marker: "requirements.txt",
        executable: "pytest",
        targeted: &["-q"],
        full: &[],
    },
    BuildSystem {
        name: "ruby",
        marker: "Gemfile",
        executable: "bundle",
        targeted: &["exec", "rspec"],
        full: &["exec", "rspec"],
    },
    BuildSystem {
        name: "php",
        marker: "composer.json",
        executable: "composer",
        targeted: &["test"],
        full: &["test"],
    },
    BuildSystem {
        name: "make",
        marker: "Makefile",
        executable: "make",
        targeted: &["test"],
        full: &["test"],
    },
    BuildSystem {
        name: "cmake",
        marker: "CMakeLists.txt",
        executable: "ctest",
        targeted: &["--output-on-failure"],
        full: &["--output-on-failure"],
    },
];

/// Every toolchain binary a repository's own build systems need.
///
/// Derived from what the repository is, rather than being a fixed list: a
/// project whose tools are denied cannot be verified, and hard-coding the
/// permitted set decides in advance which languages the agent works in.
pub fn required_executables(root: &std::path::Path) -> Vec<String> {
    let mut executables: Vec<String> = BUILD_SYSTEMS
        .iter()
        .filter(|system| root.join(system.marker).is_file())
        .map(|system| system.executable.to_string())
        .collect();
    // A JavaScript project's runner is npm, and its runtime is node.
    if root.join("package.json").is_file() {
        executables.push("npm".into());
        executables.push("node".into());
    }
    if let Some(declared) = declared_checks(root) {
        executables.extend(declared.into_iter().map(|(executable, _)| executable));
    }
    if let Ok(known) = known_failure_checks(root) {
        executables.extend(known.into_iter().map(|(executable, _)| executable));
    }
    executables.extend(
        ci_declared_checks(root)
            .into_iter()
            .map(|(executable, _)| executable),
    );
    // Last, so it reaches every source above it. Interpreters and runners are
    // named several ways for the same thing, and denying `python` to a project
    // whose declared check says `python3` refuses the interpreter it is already
    // permitted to run. Expanding before the declared and CI-derived checks are
    // added covers only the marker registry, which is the narrower half and not
    // the half a project speaks for itself with.
    const ALIASES: [(&str, &[&str]); 6] = [
        ("python3", &["python"]),
        ("python", &["python3"]),
        ("pytest", &["python3", "python"]),
        ("poetry", &["python3"]),
        ("npm", &["node", "npx"]),
        ("flutter", &["dart"]),
    ];
    for (named, also) in ALIASES {
        if executables.iter().any(|e| e == named) {
            executables.extend(also.iter().map(|a| a.to_string()));
        }
    }
    executables.sort();
    executables.dedup();
    executables
}

/// Paths outside the workspace the declared checks need to read, at
/// `.pwr/checks.json`'s top-level `readable`, e.g. a browser a layout check
/// drives. The sandbox denies reads outside the workspace, and a check that
/// cannot load its own program fails in a way that looks like the product
/// failing -- measured 2026-09-22: headless Chrome aborting in `dlopen` of its
/// framework under `/Applications`, once per turn, each time raising macOS's
/// crash dialog. Only existing absolute paths count; nothing becomes writable.
pub fn declared_readable(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    #[derive(serde::Deserialize)]
    struct Declared {
        #[serde(default)]
        readable: Vec<std::path::PathBuf>,
    }
    std::fs::read(root.join(".pwr/checks.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Declared>(&bytes).ok())
        .map(|declared| declared.readable)
        .unwrap_or_default()
        .into_iter()
        .filter(|path| path.is_absolute() && path.exists())
        .collect()
}

/// Checks a repository declares for itself, at `.pwr/checks.json`.
///
/// The escape hatch that keeps the registry from being a closed world: a
/// project PWR does not recognise says how it is verified, rather than
/// being worked on blind.
fn declared_checks(root: &std::path::Path) -> Option<Vec<(String, Vec<String>)>> {
    #[derive(serde::Deserialize)]
    struct Declared {
        checks: Vec<Check>,
    }
    #[derive(serde::Deserialize)]
    struct Check {
        executable: String,
        #[serde(default)]
        args: Vec<String>,
    }
    let bytes = std::fs::read(root.join(".pwr/checks.json")).ok()?;
    let declared: Declared = serde_json::from_slice(&bytes).ok()?;
    Some(
        declared
            .checks
            .into_iter()
            .map(|check| (check.executable, check.args))
            .collect(),
    )
}

/// Acceptance checks are the repository owner's executable evidence that the
/// requested product behaviour works, not merely that its sources compile.
///
/// A check declaration can opt into this role with `"kind": "acceptance"`.
/// The normal verifier still runs every declared check; this function only
/// tells a goal-mode caller whether at least one of those green checks was an
/// explicit product-level acceptance check. That keeps the convention open to
/// browser, API, CLI, desktop, migration, and integration workflows rather
/// than treating one framework as special.
pub fn declared_acceptance_checks(
    root: &std::path::Path,
) -> Result<Vec<(String, Vec<String>)>, String> {
    #[derive(Deserialize)]
    struct Declared {
        #[serde(default)]
        checks: Vec<Check>,
    }
    #[derive(Deserialize)]
    struct Check {
        executable: String,
        #[serde(default)]
        args: Vec<String>,
        kind: Option<String>,
    }

    let path = root.join(".pwr/checks.json");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let declared: Declared =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut acceptance = Vec::new();
    for check in declared.checks {
        match check.kind.as_deref().unwrap_or("technical") {
            "technical" => {}
            "acceptance" => acceptance.push((check.executable, check.args)),
            kind => {
                return Err(format!(
                    "{}: check kind `{kind}` is unsupported; use `technical` or `acceptance`",
                    path.display()
                ));
            }
        }
    }
    Ok(acceptance)
}

/// Explicit exceptions supplied by the workspace owner, never inferred from a
/// red baseline or accepted from the model's completion rationale. These checks
/// are still run before and after, and must retain identical, untruncated output.
pub fn known_failure_checks(root: &std::path::Path) -> Result<Vec<(String, Vec<String>)>, String> {
    #[derive(serde::Deserialize)]
    struct Check {
        executable: String,
        #[serde(default)]
        args: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct Config {
        #[serde(default)]
        known_failures: Vec<Check>,
    }
    let bytes = match std::fs::read(root.join(".pwr/checks.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let config: Config = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    Ok(config
        .known_failures
        .into_iter()
        .map(|check| (check.executable, check.args))
        .collect())
}

/// Files where a project states how it verifies itself.
///
/// Continuous integration configuration is the strongest generic source there
/// is: it is not a guess about the project, it is the commands the project
/// runs to check itself, written by the people who wrote the project, and it
/// exists for languages and frameworks nobody has heard of.
const CI_CONFIGURATIONS: [&str; 8] = [
    ".github/workflows",
    ".gitlab-ci.yml",
    ".circleci/config.yml",
    "azure-pipelines.yml",
    "Jenkinsfile",
    ".travis.yml",
    "bitbucket-pipelines.yml",
    ".drone.yml",
];

/// Words that usually mark a verification step.
///
/// A preference, not a gate. Used to rank the steps a project declares, and
/// deliberately not used to exclude the ones it does not match: `rebar3 ct`
/// and `zig build test` are both verification, and a list of recognised words
/// is the same closed world as a list of recognised languages, one level down.
const VERIFICATION_WORDS: [&str; 8] = [
    "test", "check", "verify", "lint", "spec", "ci", "audit", "assert",
];

/// Words that mean a step reaches outside the workspace, whatever else it does.
const EXCLUDED_WORDS: [&str; 8] = [
    "deploy", "publish", "push", "release", "upload", "docker", "curl", "ssh",
];

/// Verification commands a project's CI configuration states for itself.
///
/// Read as text rather than parsed per CI vendor: the shapes differ but a
/// command line is a command line, and a parser per vendor would be the same
/// closed list one level down.
fn ci_declared_checks(root: &std::path::Path) -> Vec<(String, Vec<String>)> {
    let mut found: Vec<(bool, String, Vec<String>)> = Vec::new();
    for entry in CI_CONFIGURATIONS {
        let path = root.join(entry);
        let texts: Vec<String> = if path.is_dir() {
            std::fs::read_dir(&path)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .collect()
        } else {
            std::fs::read_to_string(&path).into_iter().collect()
        };
        for text in texts {
            // A bare `- item` is a command only where the list it belongs to is
            // a list of commands. GitLab and Travis write `script:` followed by
            // such a list; GitHub writes `- uses: actions/checkout@v5`, which is
            // a step that uses an action and is not a command at all. Taking
            // every list item produced a check named `uses:`, which cannot
            // execute and therefore failed on every turn of every run in that
            // repository -- scoring a correct fix as a failure.
            let mut in_script_list = false;
            for line in text.lines() {
                let indented = line;
                let line = line.trim();
                if let Some((key, rest)) = line.split_once(':')
                    && !key.contains(' ')
                    && !key.starts_with('-')
                {
                    in_script_list = matches!(
                        key.trim_start_matches("- "),
                        "script" | "before_script" | "commands" | "run"
                    ) && rest.trim().is_empty();
                }
                // A dedent to column zero ends any list.
                if !indented.starts_with(char::is_whitespace) && !line.starts_with('-') {
                    in_script_list = in_script_list && line.ends_with(':');
                }
                let command = line
                    .strip_prefix("- run:")
                    .or_else(|| line.strip_prefix("run:"))
                    .or_else(|| in_script_list.then(|| line.strip_prefix("- ")).flatten())
                    .map(str::trim)
                    .unwrap_or("");
                if command.is_empty() || command.contains(['{', '}', '$']) {
                    continue;
                }
                // A first word ending in a colon is a YAML key, never an
                // executable -- `uses:`, `with:`, `name:`.
                if command
                    .split_whitespace()
                    .next()
                    .is_some_and(|word| word.ends_with(':'))
                {
                    continue;
                }
                let lowered = command.to_lowercase();
                // Excluded on effect, not on vocabulary: a step that deploys or
                // publishes reaches outside the workspace whatever it is called.
                if EXCLUDED_WORDS.iter().any(|w| lowered.contains(w)) {
                    continue;
                }
                // A step that chains or redirects is a script, and running its
                // first word through the tool boundary would not mean what the
                // file says.
                if command.contains("&&") || command.contains('|') || command.contains('>') {
                    continue;
                }
                let mut words = command.split_whitespace();
                let Some(executable) = words.next() else {
                    continue;
                };
                let looks_like_verification =
                    VERIFICATION_WORDS.iter().any(|w| lowered.contains(w));
                found.push((
                    looks_like_verification,
                    executable.to_string(),
                    words.map(str::to_string).collect::<Vec<_>>(),
                ));
            }
        }
    }
    found.sort();
    found.dedup();
    // Steps that read as verification come first. Where none does, the rest
    // still stand: a project whose vocabulary nobody here anticipated is the
    // case this exists for.
    let preferred: Vec<(String, Vec<String>)> = found
        .iter()
        .filter(|(looks, _, _)| *looks)
        .map(|(_, e, a)| (e.clone(), a.clone()))
        .collect();
    if !preferred.is_empty() {
        return preferred;
    }
    found.into_iter().map(|(_, e, a)| (e, a)).collect()
}

/// Selects only deterministic, locally available checks from repository manifests.
pub fn discover_checks(
    root: &std::path::Path,
    scope: &str,
) -> Result<Vec<(String, Vec<String>)>, String> {
    if !matches!(scope, "targeted" | "full") {
        return Err("scope must be targeted or full".into());
    }
    // Ordered by how directly the source speaks for the repository. An explicit
    // declaration is the repository saying it; CI configuration is the
    // repository doing it; the registry is PWR guessing from a file name.
    if let Some(declared) = declared_checks(root) {
        return Ok(declared);
    }
    let from_ci = ci_declared_checks(root);
    if !from_ci.is_empty() {
        return Ok(from_ci);
    }
    if let Some(manifest) = std::fs::read_to_string(root.join("package.json")).ok()
        && let Some(scripts) = serde_json::from_str::<serde_json::Value>(&manifest)
            .ok()
            .and_then(|manifest| manifest.get("scripts").cloned())
    {
        let has = |name: &str| {
            scripts
                .get(name)
                .and_then(serde_json::Value::as_str)
                .is_some()
        };
        let mut npm_checks = Vec::new();
        // A JavaScript application's build is as important as its tests.  The
        // previous generic npm branch silently omitted it, leaving Angular,
        // Vite, Next and service bundles with only partial evidence.
        if has("build") {
            npm_checks.push(("npm".into(), vec!["run".into(), "build".into()]));
        }
        if has("test") {
            let args = if root.join("angular.json").is_file() {
                vec!["test", "--", "--watch=false"]
            } else {
                vec!["test", "--silent"]
            };
            npm_checks.push(("npm".into(), args.into_iter().map(str::to_string).collect()));
        }
        if !npm_checks.is_empty() {
            return Ok(npm_checks);
        }
    }
    for system in BUILD_SYSTEMS {
        if !root.join(system.marker).is_file() {
            continue;
        }
        // A build system with no distinct full command runs its targeted one:
        // some toolchains have a single test entry point and inventing a
        // second would be a command nobody chose.
        let args = match scope {
            "full" if !system.full.is_empty() => system.full,
            _ => system.targeted,
        };
        return Ok(vec![(
            system.executable.to_string(),
            args.iter().map(|a| a.to_string()).collect(),
        )]);
    }
    // Last, and only for a workspace nothing above speaks for: a page whose
    // project has no toolchain still has one deterministic property, that the
    // files it asks the browser to load exist. Ranked below every other source
    // because a project that states how it verifies itself has said something
    // stronger than this check can.
    if web::has_markup(root) {
        return Ok(vec![(web::WEB_ASSETS_CHECK.to_string(), Vec::new())]);
    }
    // A repository with no verifier we recognise is not an error: it has no
    // deterministic checks, which the caller records rather than refuses. A
    // run against it simply cannot claim verification, and completion is
    // judged on nothing rather than on something invented.
    Ok(Vec::new())
}
pub async fn baseline(
    policy: &ToolPolicy,
    checks: &[(String, Vec<String>)],
) -> Result<VerificationBaseline, ToolError> {
    let mut records = Vec::new();
    for (cmd, args) in checks {
        let result = if cmd == web::WEB_ASSETS_CHECK {
            web_assets_result(policy)
        } else {
            run_command(policy, cmd, args).await?
        };
        records.push(CheckRecord {
            command: format!("{} {}", cmd, args.join(" ")).trim_end().to_string(),
            result,
        });
    }
    let stable_record = records
        .iter()
        .map(|record| {
            (
                &record.command,
                record.result.exit_code,
                &record.result.artifact_hash,
            )
        })
        .collect::<Vec<_>>();
    let h = hash_bytes(serde_json::to_vec(&stable_record).unwrap_or_default());
    Ok(VerificationBaseline {
        id: pwr_domain::new_id(),
        captured_at: now(),
        checks: records,
        environment_hash: h,
    })
}
/// Runs the markup reference check and reports it as any other check reports.
///
/// A `ToolResult` because everything downstream -- the baseline hash, the
/// comparison, the known-failure rule, the recovery locator reading a file and
/// line out of the output -- is written against that one shape, and a second
/// shape for a second kind of check would have to be taught to all of them.
fn web_assets_result(policy: &ToolPolicy) -> ToolResult {
    let started = std::time::Instant::now();
    let findings = web::missing_assets(&policy.root);
    let (stdout, redacted) = policy.redact(&web::report(&findings));
    ToolResult {
        exit_code: Some(i32::from(!findings.is_empty())),
        stdout,
        stderr: String::new(),
        duration_ms: started.elapsed().as_millis(),
        redacted,
        artifact_hash: hash_bytes(findings.len().to_string()),
        stdout_truncated: false,
        stderr_truncated: false,
        // No process ran, so nothing was confined. Recorded as it happened
        // rather than as `true`: the field exists so an audit can see what was
        // outside a sandbox, and a check that claims confinement it never
        // needed teaches an auditor to trust the flag less.
        sandboxed: false,
        // Synthesised from the asset walk rather than run as a command, so
        // there is no compiler output to summarise.
        failing_files: None,
    }
}

pub fn compare(
    previous: &VerificationBaseline,
    current: &VerificationBaseline,
) -> BaselineComparison {
    let new_failures = current
        .checks
        .iter()
        .filter(|current_check| {
            current_check.result.exit_code != Some(0)
                && previous
                    .checks
                    .iter()
                    .find(|prior| prior.command == current_check.command)
                    .is_none_or(|prior| {
                        prior.result.exit_code == Some(0)
                            || !same_failure(&prior.result, &current_check.result)
                    })
        })
        .map(|check| check.command.clone())
        .collect::<Vec<_>>();
    BaselineComparison {
        previous_baseline_id: previous.id,
        current_baseline_id: current.id,
        regression_free: new_failures.is_empty(),
        new_failures,
    }
}

/// A pre-existing failure can only be preserved when its observable result is
/// unchanged. A red command masking new red tests is not evidence of safety.
/// Truncated or redacted output cannot establish equality of unseen diagnostics.
pub fn same_failure(previous: &ToolResult, current: &ToolResult) -> bool {
    previous.exit_code.is_some()
        && previous.exit_code != Some(0)
        && previous.exit_code == current.exit_code
        && !previous.redacted
        && !current.redacted
        && !previous.stdout_truncated
        && !previous.stderr_truncated
        && !current.stdout_truncated
        && !current.stderr_truncated
        && previous.stdout == current.stdout
        && previous.stderr == current.stderr
}

/// For an independently scored read-only answer, compare command exit status,
/// not volatile diagnostic text. The caller MUST also check unchanged source
/// contents. This is not an acceptance rule for code changes or known failures.
pub fn compare_read_only_status(
    previous: &VerificationBaseline,
    current: &VerificationBaseline,
) -> BaselineComparison {
    let new_failures: Vec<String> = current
        .checks
        .iter()
        .filter(|check| {
            check.result.exit_code != Some(0)
                && (check.result.exit_code.is_none()
                    || previous
                        .checks
                        .iter()
                        .find(|prior| prior.command == check.command)
                        .is_none_or(|prior| prior.result.exit_code != check.result.exit_code))
        })
        .map(|check| check.command.clone())
        .collect();
    BaselineComparison {
        previous_baseline_id: previous.id,
        current_baseline_id: current.id,
        regression_free: new_failures.is_empty(),
        new_failures,
    }
}

/// A failure located in the source, rather than described in prose.
///
/// Compiler and test output reached the deployment as bounded text, so
/// recovery aimed at a paragraph: the harness knew a check had failed and
/// nothing more, and finding the file and line was work the model paid actions
/// for. Every toolchain writes the same three facts in a different order, and
/// reading them is mechanical -- which is the definition of the harness's job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// `error`, `warning`, `failure`, or whatever the toolchain called it.
    pub severity: String,
    pub message: String,
}

/// Extracts located failures from a check's output.
///
/// Deliberately shallow, and shallow in a way that is safe: a line that does
/// not clearly carry a path and a position is not guessed at. A wrong location
/// is worse than none -- it sends the agent to edit a file that is fine.
///
/// Recognises the shapes the common toolchains actually emit rather than
/// parsing per vendor, for the same reason check discovery reads CI as text: a
/// parser per toolchain is a closed list of languages one level down.
pub fn diagnostics(output: &str) -> Vec<Diagnostic> {
    let mut found: Vec<Diagnostic> = Vec::new();
    for line in output.lines().take(MAX_DIAGNOSTIC_LINES) {
        let line = line.trim();
        if let Some(diagnostic) = rust_style(line)
            .or_else(|| gcc_style(line))
            .or_else(|| python_style(line))
            && !found.contains(&diagnostic)
        {
            found.push(diagnostic);
        }
        if found.len() >= MAX_DIAGNOSTICS {
            break;
        }
    }
    found
}

const MAX_DIAGNOSTIC_LINES: usize = 4_000;
const MAX_DIAGNOSTICS: usize = 32;

/// `  --> src/lib.rs:12:5`, which rustc writes under its message.
fn rust_style(line: &str) -> Option<Diagnostic> {
    let rest = line.strip_prefix("--> ")?;
    let (path, line_number, column) = split_position(rest)?;
    Some(Diagnostic {
        path,
        line: line_number,
        column,
        severity: "error".into(),
        message: String::new(),
    })
}

/// `src/main.c:12:5: error: message`, which gcc, clang, tsc, eslint and go
/// vet all approximate.
fn gcc_style(line: &str) -> Option<Diagnostic> {
    let (position, rest) = line.split_once(": ")?;
    let (path, line_number, column) = split_position(position)?;
    line_number?;
    let (severity, message) = rest.split_once(": ").unwrap_or(("error", rest));
    let severity = severity.trim().to_ascii_lowercase();
    // Only words a toolchain actually uses for a severity, or every colon in
    // a log line becomes a diagnostic.
    if !["error", "warning", "note", "fatal error", "failure"].contains(&severity.as_str()) {
        return None;
    }
    Some(Diagnostic {
        path,
        line: line_number,
        column,
        severity,
        message: message.trim().to_string(),
    })
}

/// `File "app/thing.py", line 12, in handler`, from a Python traceback.
fn python_style(line: &str) -> Option<Diagnostic> {
    let rest = line.strip_prefix("File \"")?;
    let (path, rest) = rest.split_once("\", line ")?;
    let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some(Diagnostic {
        path: path.to_string(),
        line: number.parse().ok(),
        column: None,
        severity: "error".into(),
        message: String::new(),
    })
}

/// `path:line:column` or `path:line`, with a path that may itself contain a
/// drive letter or no colon at all.
fn split_position(text: &str) -> Option<(String, Option<u32>, Option<u32>)> {
    let parts: Vec<&str> = text.rsplitn(3, ':').collect();
    match parts.as_slice() {
        [column, line, path] => {
            let line = line.parse().ok()?;
            Some(((*path).to_string(), Some(line), column.parse().ok()))
        }
        [line, path] => {
            let line = line.parse().ok()?;
            Some(((*path).to_string(), Some(line), None))
        }
        _ => None,
    }
}

/// Re-runs a failing check to tell a real failure from a flake.
///
/// Without this, `NonDeterminism` is unreachable: a flaky test classifies as
/// `Assertion`, which authorises an edit-and-retry cycle, so the agent edits
/// working code to chase a failure that was never in the code. A check whose
/// outcome changes on identical inputs is non-deterministic, and the recovery
/// taxonomy stops rather than edits.
pub async fn classify_with_reproduction(
    policy: &ToolPolicy,
    command: &str,
    args: &[String],
    first: &ToolResult,
) -> Result<FailureClass, ToolError> {
    if first.exit_code == Some(0) {
        return Ok(classify(first));
    }
    let second = run_command(policy, command, args).await?;
    if second.exit_code != first.exit_code {
        return Ok(FailureClass::NonDeterminism);
    }
    Ok(classify(first))
}

/// Refusals the code under edit cannot have caused.
///
/// The sandbox denying a write, the network being unreachable: no edit to the
/// repository produces these, so nothing about them is evidence against the
/// edit.
const DENIED: [&str; 3] = [
    "operation not permitted",
    "permission denied",
    "network is unreachable",
];

/// Absences that are usually the environment and occasionally the code.
///
/// A missing interpreter and a missing `#include` print the same words. They
/// are told apart by whether anything in the output points at a source
/// location: a compiler that cannot find a header says which file and line
/// asked for it, and `make: python: No such file or directory` says nothing of
/// the kind.
const MISSING: [&str; 2] = ["not found", "no such file"];

/// Stderr with every warning a tool printed taken out, header and indented
/// continuation alike.
///
/// A warning is, by the tool's own account, something that did not stop it, so
/// no word inside one can be what a check failed on. Measured in the R2 pilot of
/// 2026-09-14: `cargo test` on macOS under the sandbox prints `warning: output
/// of xcrun ... (errno=Operation not permitted)` because xcrun cannot write its
/// cache, then runs the tests and reports a real assertion failure. The denial
/// inside the warning made the baseline read as an environment failure, the
/// check was exempted as unrunnable, and 23 correct changes and two correct
/// answers on six red-baseline tasks were recorded as completions nothing could
/// verify.
///
/// A block starts at a line beginning `warning:` or `warning[` and runs over the
/// lines indented beneath it; a blank or unindented line ends it. That is the
/// rustc and cargo shape. Warnings in other shapes are left in, which errs
/// toward the behaviour this replaced rather than toward a new guess.
fn outside_warnings(stderr: &str) -> String {
    let mut kept = String::with_capacity(stderr.len());
    let mut in_warning = false;
    for line in stderr.lines() {
        let indented = line.starts_with([' ', '\t']);
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("warning:") || lower.starts_with("warning[") {
            in_warning = true;
            continue;
        }
        if in_warning && indented {
            continue;
        }
        in_warning = false;
        kept.push_str(&lower);
        kept.push('\n');
    }
    kept
}

pub fn classify(result: &ToolResult) -> FailureClass {
    let stderr = outside_warnings(&result.stderr);
    // Before the compiler heuristic, not after it. `error:` is printed by every
    // tool that has ever failed, and these are printed by a specific thing
    // going wrong. Measured on more-itertools under this sandbox: `make: error:
    // couldn't create cache file (errno=Operation not permitted)` followed by
    // `make: python: No such file or directory` was classified `Compilation`,
    // because `error:` matched first -- so the run told a deployment that its
    // correct one-line fix had broken the build, and spent nineteen of its
    // twenty-nine actions on that. The environment branch existed all along and
    // was unreachable.
    if DENIED.iter().any(|needle| stderr.contains(needle))
        || (MISSING.iter().any(|needle| stderr.contains(needle)) && result.failing_files.is_none())
    {
        FailureClass::Environment
    } else if result.stderr.contains("error[") || result.stderr.contains("error:") {
        FailureClass::Compilation
    } else if result.stderr.contains("assert") || result.stdout.contains("FAILED") {
        FailureClass::Assertion
    } else if result.exit_code.is_none() {
        FailureClass::Environment
    } else {
        // A verifier that ran reproducibly and returned a non-zero status is a
        // failed assertion even when it chose to print nothing (for example
        // `test condition` in a shell script).
        FailureClass::Assertion
    }
}
/// Decides recovery without granting any tool authority. Infrastructure failures never authorize edits.
pub fn recovery_decision(
    class: FailureClass,
    edit_attempts: u8,
    context_attempts: u8,
    budget: &RecoveryBudget,
) -> RecoveryDecision {
    match class {
        FailureClass::Compilation | FailureClass::Assertion
            if edit_attempts < budget.max_edit_verify_cycles =>
        {
            RecoveryDecision::EditAndRetry {
                remaining_edit_verify_cycles: budget.max_edit_verify_cycles - edit_attempts,
            }
        }
        FailureClass::Provider if context_attempts < budget.max_context_retries => {
            RecoveryDecision::RetryContextTier {
                remaining_context_retries: budget.max_context_retries - context_attempts,
            }
        }
        FailureClass::Compilation | FailureClass::Assertion => RecoveryDecision::Stop {
            reason: "edit-verify budget exhausted".into(),
        },
        FailureClass::Environment => RecoveryDecision::Stop {
            reason: "environment failure: classify and repair infrastructure before editing".into(),
        },
        FailureClass::Policy => RecoveryDecision::Stop {
            reason: "policy denial requires explicit authorization".into(),
        },
        FailureClass::NonDeterminism => RecoveryDecision::Stop {
            reason: "non-deterministic verification requires a reproducible failure".into(),
        },
        FailureClass::Provider => RecoveryDecision::Stop {
            reason: "context retry budget exhausted".into(),
        },
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn environment_failure_never_authorizes_edit() {
        assert!(matches!(
            recovery_decision(FailureClass::Environment, 0, 0, &RecoveryBudget::default()),
            RecoveryDecision::Stop { .. }
        ));
    }
    #[test]
    fn bounded_code_recovery_stops_after_budget() {
        let budget = RecoveryBudget {
            max_edit_verify_cycles: 1,
            max_context_retries: 1,
        };
        assert!(matches!(
            recovery_decision(FailureClass::Assertion, 0, 0, &budget),
            RecoveryDecision::EditAndRetry { .. }
        ));
        assert!(matches!(
            recovery_decision(FailureClass::Assertion, 1, 0, &budget),
            RecoveryDecision::Stop { .. }
        ));
    }
}
