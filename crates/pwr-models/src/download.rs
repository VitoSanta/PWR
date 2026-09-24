//! Verified model downloads, shared by `pwr models download` and the app.
//!
//! A plan names every file, where it goes, how many bytes it has and the
//! checksum it must match. Nothing is downloaded that the plan does not name,
//! nothing is written outside the plan's destination, and nothing already on
//! disk is overwritten: a file that exists and matches is kept, one that
//! exists and does not is refused with the path to move.
//!
//! Bytes go to `<file>.part` and are renamed into place only once verified,
//! so a half-written file never looks like a model. An interrupted download
//! -- cancelled, the network gone, the app quit -- leaves its `.part`, and the
//! next attempt resumes it with an HTTP range request.
//!
//! Checksums: BLAKE3 or SHA-256 where a registry declares them, the LFS
//! SHA-256 the Hub publishes for large files, and the git blob SHA-1 for the
//! small files git stores directly (`sha1("blob <len>\0" + content)`).

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Free space kept on the disk after a download, for everything else.
pub const DISK_MARGIN_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// One file of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedFile {
    pub file: String,
    pub url: String,
    pub destination: PathBuf,
    pub expected_bytes: u64,
    pub blake3: Option<String>,
    pub sha256: Option<String>,
    pub git_sha1: Option<String>,
}

impl PlannedFile {
    fn needs(&self) -> Needs {
        Needs {
            blake3: self.blake3.is_some(),
            sha256: self.sha256.is_some(),
            git_sha1: self.git_sha1.is_some(),
        }
    }

    /// Whether what is on disk is this file: the right size, and at least one
    /// declared checksum matching.
    fn matches(&self, observed: &Observed) -> bool {
        observed.bytes == self.expected_bytes
            && (self
                .blake3
                .as_deref()
                .is_some_and(|hash| observed.blake3.as_deref() == Some(hash))
                || self
                    .sha256
                    .as_deref()
                    .is_some_and(|hash| observed.sha256.as_deref() == Some(hash))
                || self
                    .git_sha1
                    .as_deref()
                    .is_some_and(|hash| observed.git_sha1.as_deref() == Some(hash)))
    }
}

/// Everything one download writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub destination_root: PathBuf,
    pub files: Vec<PlannedFile>,
    /// The Hub commit the files were resolved at. Written beside them as
    /// [`REVISION_FILE`] once all are verified, so evidence gathered on the
    /// model later can name the revision it applies to.
    #[serde(default)]
    pub revision: Option<String>,
}

/// Where a finished download records its Hub commit.
pub const REVISION_FILE: &str = ".pwr-revision";

impl Plan {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.expected_bytes).sum()
    }
}

/// Why a download did not complete, in a form the app can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// The plan lacks sizes or checksums, so nothing could be verified.
    Unverifiable,
    /// Not enough free space for the download and the margin.
    InsufficientDisk,
    /// A file at the destination exists and is not this one.
    Conflict,
    /// The Hub could not be reached, or refused.
    Network,
    /// The bytes arrived and do not match their checksum.
    Verification,
    /// Reading or writing the disk failed.
    Io,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadError {
    pub kind: FailureKind,
    pub message: String,
}

impl DownloadError {
    fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// The CLI's error category for the same failure.
    pub fn category(&self) -> &'static str {
        match self.kind {
            FailureKind::Unverifiable
            | FailureKind::InsufficientDisk
            | FailureKind::Verification => "missing_evidence",
            FailureKind::Conflict => "invalid_input",
            FailureKind::Network => "external",
            FailureKind::Io => "internal",
            FailureKind::Cancelled => "interrupted",
        }
    }
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// What the disk check found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preflight {
    pub destination_root: String,
    pub required_bytes: u64,
    pub margin_bytes: u64,
    pub available_bytes: u64,
}

/// Where a download is, for progress notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preparing,
    Downloading,
    Verifying,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub file: String,
    pub file_bytes: u64,
    pub file_total: u64,
    /// Across every file of the plan.
    pub bytes: u64,
    pub total: u64,
}

/// Every file's plan ready to download: each has a size and a checksum.
pub fn require_verifiable(plan: &Plan) -> Result<(), DownloadError> {
    for file in &plan.files {
        if file.blake3.is_none() && file.sha256.is_none() && file.git_sha1.is_none() {
            return Err(DownloadError::new(
                FailureKind::Unverifiable,
                format!(
                    "{} cannot be downloaded safely: {} lacks bytes or a hash",
                    plan.id, file.file
                ),
            ));
        }
    }
    Ok(())
}

/// Checks the destination and the disk before a byte is fetched. A final file
/// that exists must already be this one (it is then skipped); a `.part` counts
/// as bytes already downloaded.
pub fn preflight(plan: &Plan, available_bytes: u64) -> Result<Preflight, DownloadError> {
    let mut required = 0_u64;
    for file in &plan.files {
        if file.destination.is_file() {
            let observed = observe(&file.destination, file.needs())?;
            if file.matches(&observed) {
                continue;
            }
            return Err(conflict(&file.destination));
        }
        let part = part_path(&file.destination);
        let existing = part.metadata().map(|m| m.len()).unwrap_or(0);
        if existing > file.expected_bytes {
            return Err(DownloadError::new(
                FailureKind::Conflict,
                format!(
                    "{} is larger than the registry expects; remove it before retrying",
                    part.display()
                ),
            ));
        }
        required = required.saturating_add(file.expected_bytes - existing);
    }
    let needed = required.saturating_add(DISK_MARGIN_BYTES);
    if available_bytes < needed && required > 0 {
        return Err(DownloadError::new(
            FailureKind::InsufficientDisk,
            format!(
                "download needs {required} bytes plus {DISK_MARGIN_BYTES} bytes of margin, but {} has only {available_bytes} bytes free",
                plan.destination_root.display()
            ),
        ));
    }
    Ok(Preflight {
        destination_root: plan.destination_root.display().to_string(),
        required_bytes: required,
        margin_bytes: DISK_MARGIN_BYTES,
        available_bytes,
    })
}

fn conflict(destination: &Path) -> DownloadError {
    DownloadError::new(
        FailureKind::Conflict,
        format!(
            "{} exists but does not match the registry; move it before retrying",
            destination.display()
        ),
    )
}

/// Free bytes where `path` will be written. `PWR_DOWNLOAD_FREE_BYTES`
/// overrides the reading, for tests.
pub fn available_disk_bytes(path: &Path) -> Result<u64, DownloadError> {
    if let Ok(value) = std::env::var("PWR_DOWNLOAD_FREE_BYTES") {
        return value.parse::<u64>().map_err(|error| {
            DownloadError::new(
                FailureKind::Io,
                format!("PWR_DOWNLOAD_FREE_BYTES is not a byte count: {error}"),
            )
        });
    }
    pwr_runtime::host::free_space(path)
        .map(|(free, _)| free)
        .ok_or_else(|| {
            DownloadError::new(
                FailureKind::Io,
                format!("cannot read the free disk space for {}", path.display()),
            )
        })
}

/// What one file's download came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileOutcome {
    pub file: String,
    pub destination: String,
    /// `already_present`, `verified_part`, `resumed` or `downloaded`.
    pub status: String,
    pub bytes: u64,
    pub blake3: Option<String>,
    pub sha256: Option<String>,
    pub git_sha1: Option<String>,
}

/// Downloads and verifies every file of `plan`, in order. `stop` is checked
/// between chunks: a cancelled download keeps its `.part` for next time.
pub async fn download(
    client: &reqwest::Client,
    plan: &Plan,
    auth_token: Option<&str>,
    progress: &mut dyn FnMut(&Progress),
    stop: &AtomicBool,
) -> Result<Vec<FileOutcome>, DownloadError> {
    let total = plan.total_bytes();
    let mut done_before = 0_u64;
    let mut outcomes = Vec::new();
    for file in &plan.files {
        let outcome = download_file(
            client,
            file,
            auth_token,
            &mut |phase, file_bytes| {
                progress(&Progress {
                    phase,
                    file: file.file.clone(),
                    file_bytes,
                    file_total: file.expected_bytes,
                    bytes: done_before + file_bytes,
                    total,
                });
            },
            stop,
        )
        .await?;
        done_before += file.expected_bytes;
        outcomes.push(outcome);
    }
    if let Some(revision) = plan.revision.as_deref().filter(|r| catalog_revision(r)) {
        // Best effort: a missing marker only leaves the revision unknown.
        let _ = std::fs::write(plan.destination_root.join(REVISION_FILE), revision);
    }
    Ok(outcomes)
}

async fn download_file(
    client: &reqwest::Client,
    file: &PlannedFile,
    auth_token: Option<&str>,
    progress: &mut dyn FnMut(Phase, u64),
    stop: &AtomicBool,
) -> Result<FileOutcome, DownloadError> {
    let destination = &file.destination;
    let needs = file.needs();
    let outcome = |status: &str, observed: Observed| FileOutcome {
        file: file.file.clone(),
        destination: destination.display().to_string(),
        status: status.to_owned(),
        bytes: observed.bytes,
        blake3: observed.blake3,
        sha256: observed.sha256,
        git_sha1: observed.git_sha1,
    };
    if destination.is_file() {
        progress(Phase::Verifying, file.expected_bytes);
        let observed = observe(destination, needs)?;
        if file.matches(&observed) {
            return Ok(outcome("already_present", observed));
        }
        return Err(conflict(destination));
    }
    let parent = destination.parent().ok_or_else(|| {
        DownloadError::new(
            FailureKind::Io,
            format!("{} has no parent directory", destination.display()),
        )
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        DownloadError::new(
            FailureKind::Io,
            format!("cannot create {}: {error}", parent.display()),
        )
    })?;
    let part = part_path(destination);
    let existing = part.metadata().map(|m| m.len()).unwrap_or(0);
    progress(Phase::Downloading, existing);
    if existing > file.expected_bytes {
        return Err(DownloadError::new(
            FailureKind::Conflict,
            format!(
                "{} is larger than the registry expects; remove it before retrying",
                part.display()
            ),
        ));
    }
    if existing == file.expected_bytes {
        progress(Phase::Verifying, existing);
        let observed = observe(&part, needs)?;
        return finish(file, &part, observed, "verified_part").map(|o| outcome(&o.0, o.1));
    }
    let mut request = client.get(&file.url);
    if let Some(token) = auth_token.filter(|token| !token.trim().is_empty()) {
        request = request.bearer_auth(token);
    }
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = request.send().await.map_err(|error| {
        DownloadError::new(
            FailureKind::Network,
            format!("cannot download {}: {error}", file.url),
        )
    })?;
    let status = response.status();
    if !(status.is_success() || (existing > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT)) {
        let hint = match status.as_u16() {
            401 | 403 => {
                " -- the repository may be gated: accept its terms on huggingface.co and set HF_TOKEN"
            }
            404 => " -- the file is no longer at that revision",
            429 => " -- the Hub is rate-limiting; wait a minute and retry",
            _ => "",
        };
        return Err(DownloadError::new(
            FailureKind::Network,
            format!("{} returned HTTP {status}{hint}", file.url),
        ));
    }
    let append = existing > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut writer = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&part)
        .map_err(|error| {
            DownloadError::new(
                FailureKind::Io,
                format!("cannot open {}: {error}", part.display()),
            )
        })?;
    let mut written = if append { existing } else { 0 };
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if stop.load(Ordering::Relaxed) {
            return Err(DownloadError::new(
                FailureKind::Cancelled,
                "download cancelled; partial data was kept for resume",
            ));
        }
        let chunk = chunk.map_err(|error| {
            DownloadError::new(
                FailureKind::Network,
                format!(
                    "download failed for {}: {error}; the partial file is kept, retry to resume",
                    file.url
                ),
            )
        })?;
        writer.write_all(&chunk).map_err(|error| {
            DownloadError::new(
                FailureKind::Io,
                format!("cannot write {}: {error}", part.display()),
            )
        })?;
        written += chunk.len() as u64;
        if written > file.expected_bytes {
            drop(writer);
            let _ = std::fs::remove_file(&part);
            return Err(DownloadError::new(
                FailureKind::Verification,
                format!(
                    "{} sent more than the {} bytes declared; the partial file was removed",
                    file.url, file.expected_bytes
                ),
            ));
        }
        progress(Phase::Downloading, written);
    }
    drop(writer);
    progress(Phase::Verifying, written);
    let observed = observe(&part, needs)?;
    let status = if existing > 0 {
        "resumed"
    } else {
        "downloaded"
    };
    finish(file, &part, observed, status).map(|o| outcome(&o.0, o.1))
}

/// Moves a verified `.part` into place. A `.part` that is complete and wrong
/// is removed: resuming it could never succeed.
fn finish(
    file: &PlannedFile,
    part: &Path,
    observed: Observed,
    status: &str,
) -> Result<(String, Observed), DownloadError> {
    if !file.matches(&observed) {
        if observed.bytes >= file.expected_bytes {
            let _ = std::fs::remove_file(part);
        }
        return Err(DownloadError::new(
            FailureKind::Verification,
            format!(
                "{} downloaded but verification failed: got {} bytes blake3={:?} sha256={:?} git_sha1={:?}",
                file.file, observed.bytes, observed.blake3, observed.sha256, observed.git_sha1
            ),
        ));
    }
    std::fs::rename(part, &file.destination).map_err(|error| {
        DownloadError::new(
            FailureKind::Io,
            format!(
                "cannot move {} to {}: {error}",
                part.display(),
                file.destination.display()
            ),
        )
    })?;
    Ok((status.to_owned(), observed))
}

/// `model.gguf` → `model.gguf.part`.
pub fn part_path(destination: &Path) -> PathBuf {
    destination.with_extension(format!(
        "{}part",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ))
}

/// A plan's state on disk, without hashing anything: cheap enough to compute
/// every time a list is drawn, and therefore never "verified".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalState {
    Missing,
    /// Some bytes are on disk, in `.part` files or completed files.
    Partial,
    /// Every file is present at its declared size (not re-verified).
    Present,
}

pub fn local_state(plan: &Plan) -> (LocalState, u64) {
    let mut present = 0usize;
    let mut bytes = 0_u64;
    for file in &plan.files {
        match file.destination.metadata() {
            Ok(metadata) if metadata.len() == file.expected_bytes => {
                present += 1;
                bytes += metadata.len();
            }
            Ok(_) => {}
            Err(_) => {
                bytes += part_path(&file.destination)
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or(0);
            }
        }
    }
    let state = if present == plan.files.len() && !plan.files.is_empty() {
        LocalState::Present
    } else if bytes > 0 {
        LocalState::Partial
    } else {
        LocalState::Missing
    };
    (state, bytes)
}

#[derive(Debug, Clone, Copy)]
struct Needs {
    blake3: bool,
    sha256: bool,
    git_sha1: bool,
}

#[derive(Debug, Clone, Default)]
struct Observed {
    bytes: u64,
    blake3: Option<String>,
    sha256: Option<String>,
    git_sha1: Option<String>,
}

fn observe(path: &Path, needs: Needs) -> Result<Observed, DownloadError> {
    use sha1::Digest as _;
    let io = |error: std::io::Error| {
        DownloadError::new(
            FailureKind::Io,
            format!("cannot read {}: {error}", path.display()),
        )
    };
    let length = path.metadata().map_err(io)?.len();
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut blake3 = needs.blake3.then(blake3::Hasher::new);
    let mut sha256 = needs.sha256.then(sha2::Sha256::new);
    let mut git_sha1 = needs.git_sha1.then(|| {
        let mut hasher = sha1::Sha1::new();
        hasher.update(format!("blob {length}\0").as_bytes());
        hasher
    });
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut bytes = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(io)?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        if let Some(hasher) = &mut blake3 {
            hasher.update(&buffer[..read]);
        }
        if let Some(hasher) = &mut sha256 {
            hasher.update(&buffer[..read]);
        }
        if let Some(hasher) = &mut git_sha1 {
            hasher.update(&buffer[..read]);
        }
    }
    Ok(Observed {
        bytes,
        blake3: blake3.map(|hasher| hasher.finalize().to_hex().to_string()),
        sha256: sha256.map(|hasher| format!("{:x}", hasher.finalize())),
        git_sha1: git_sha1.map(|hasher| format!("{:x}", hasher.finalize())),
    })
}

/// A download as the app follows it. The server holds one per download and
/// sends its state with every notification, so the app draws a state and does
/// not reconstruct one from events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DownloadState {
    Preparing,
    Downloading {
        bytes: u64,
        total: u64,
    },
    Verifying {
        bytes: u64,
        total: u64,
    },
    Completed {
        total: u64,
    },
    Failed {
        kind: FailureKind,
        message: String,
        bytes: u64,
    },
    Cancelled {
        bytes: u64,
    },
}

/// What happens to a download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadEvent {
    Progress(Progress),
    Finished,
    Failed(DownloadError),
}

impl DownloadState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DownloadState::Completed { .. }
                | DownloadState::Failed { .. }
                | DownloadState::Cancelled { .. }
        )
    }

    fn bytes(&self) -> u64 {
        match self {
            DownloadState::Preparing => 0,
            DownloadState::Downloading { bytes, .. }
            | DownloadState::Verifying { bytes, .. }
            | DownloadState::Failed { bytes, .. }
            | DownloadState::Cancelled { bytes } => *bytes,
            DownloadState::Completed { total } => *total,
        }
    }

    /// The next state. A terminal state is final: a late progress report from
    /// a download already cancelled or failed cannot revive it.
    pub fn on(self, event: &DownloadEvent) -> DownloadState {
        if self.is_terminal() {
            return self;
        }
        match event {
            DownloadEvent::Progress(progress) => match progress.phase {
                Phase::Preparing => DownloadState::Preparing,
                Phase::Downloading => DownloadState::Downloading {
                    bytes: progress.bytes,
                    total: progress.total,
                },
                Phase::Verifying => DownloadState::Verifying {
                    bytes: progress.bytes,
                    total: progress.total,
                },
            },
            DownloadEvent::Finished => DownloadState::Completed {
                total: match &self {
                    DownloadState::Downloading { total, .. }
                    | DownloadState::Verifying { total, .. } => *total,
                    _ => self.bytes(),
                },
            },
            DownloadEvent::Failed(error) if error.kind == FailureKind::Cancelled => {
                DownloadState::Cancelled {
                    bytes: self.bytes(),
                }
            }
            DownloadEvent::Failed(error) => DownloadState::Failed {
                kind: error.kind,
                message: error.message.clone(),
                bytes: self.bytes(),
            },
        }
    }
}

/// A full commit id and nothing else, so the marker never carries a path or
/// markup from a response.
fn catalog_revision(revision: &str) -> bool {
    crate::catalog::is_revision(revision)
}
