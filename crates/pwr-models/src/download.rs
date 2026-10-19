//! Verified model downloads, shared by `pwr models download` and the app.
//!
//! A plan names every file, where it goes, how many bytes it has and the
//! checksum it must match. Nothing is downloaded that the plan does not name,
//! nothing is written outside the plan's destination, and nothing already on
//! disk is overwritten: a file that exists and matches is kept, one that
//! exists and does not is refused with the path to move.
//!
//! Bytes go to `<file>.part` and are published without replacement once verified,
//! so a half-written file never looks like a model. An interrupted download
//! -- cancelled, the network gone, the app quit -- leaves its `.part`, and the
//! next attempt resumes it with an HTTP range request.
//!
//! Checksums: BLAKE3 or SHA-256 where a registry declares them, the LFS
//! SHA-256 the Hub publishes for large files, and the git blob SHA-1 for the
//! small files git stores directly (`sha1("blob <len>\0" + content)`).

mod storage;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use storage::{Location, Root};

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
        let existing = partial_metadata(&part)?
            .map(|metadata| metadata.len())
            .unwrap_or(0);
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

/// A resumable partial must be a regular file itself. Following a `.part`
/// symlink here could overwrite a file outside the selected artifact folder.
fn partial_metadata(path: &Path) -> Result<Option<std::fs::Metadata>, DownloadError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(DownloadError::new(
                FailureKind::Conflict,
                format!(
                    "{} is not a regular partial file; move it before retrying",
                    path.display()
                ),
            ))
        }
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DownloadError::new(
            FailureKind::Io,
            format!("cannot inspect {}: {error}", path.display()),
        )),
    }
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
    let root = Root::new(&plan.destination_root).map_err(|e| {
        DownloadError::new(
            FailureKind::Conflict,
            if e.kind() == std::io::ErrorKind::WouldBlock { "Another download is using this destination; wait for it to finish or cancel it first.".into() } else { format!("cannot anchor download root: {e}") },
        )
    })?;
    let total = plan.total_bytes();
    let mut done_before = 0_u64;
    let mut outcomes = Vec::new();
    for file in &plan.files {
        let location = root.location(&file.destination).map_err(|e| {
            DownloadError::new(
                FailureKind::Conflict,
                format!("invalid download destination: {e}"),
            )
        })?;
        let outcome = download_file(
            client,
            file,
            &location,
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
        let _ = root.revision(revision);
    }
    root.ensure_current().map_err(|e| {
        DownloadError::new(FailureKind::Conflict, format!("download root changed: {e}"))
    })?;
    Ok(outcomes)
}

async fn download_file(
    client: &reqwest::Client,
    file: &PlannedFile,
    location: &Location,
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
    if location.read(false).is_ok() {
        progress(Phase::Verifying, file.expected_bytes);
        let observed = observe_location(location, false, needs).await?;
        if file.matches(&observed) {
            return Ok(outcome("already_present", observed));
        }
        return Err(conflict(destination));
    }
    let part = &location.part;
    let started_with = location.partial_len().map_err(|e| {
        DownloadError::new(FailureKind::Conflict, format!("invalid partial file: {e}"))
    })?;
    if started_with == file.expected_bytes {
        progress(Phase::Verifying, started_with);
        let observed = observe_location(location, true, needs).await?;
        return finish(file, location, observed, "verified_part").map(|o| outcome(&o.0, o.1));
    }
    // A transfer that stops after data arrived is resumed where the `.part`
    // ends: large files over a real connection are interrupted now and then,
    // and giving up there left a 19 GB download failing every few gigabytes.
    // One that never connects fails at once, so being offline is reported
    // promptly rather than after a round of waits.
    let mut retries_left = TRANSFER_RETRIES;
    let mut progressed_before = false;
    let written = loop {
        match fetch(client, file, location, auth_token, part, progress, stop).await {
            Ok(written) => break written,
            Err(attempt) => {
                let resumable = attempt.transient
                    && (attempt.progressed || progressed_before)
                    && retries_left > 0
                    && !stop.load(Ordering::Relaxed);
                if !resumable {
                    return Err(attempt.error);
                }
                progressed_before |= attempt.progressed;
                retries_left -= 1;
                let wait = RETRY_WAIT * 2_u32.pow(TRANSFER_RETRIES - retries_left - 1);
                if !pause(wait.min(MAX_RETRY_WAIT), stop).await {
                    return Err(DownloadError::new(
                        FailureKind::Cancelled,
                        "download cancelled; partial data was kept for resume",
                    ));
                }
                if attempt.progressed {
                    // Progress earns the patience back.
                    retries_left = TRANSFER_RETRIES;
                }
            }
        }
    };
    progress(Phase::Verifying, written);
    let observed = observe_location(location, true, needs).await?;
    let status = if started_with > 0 {
        "resumed"
    } else {
        "downloaded"
    };
    finish(file, location, observed, status).map(|o| outcome(&o.0, o.1))
}

/// How many times a stalled or dropped transfer is resumed in a row before
/// the download fails (the `.part` is kept either way).
const TRANSFER_RETRIES: u32 = 5;
const RETRY_WAIT: std::time::Duration = std::time::Duration::from_secs(1);
const MAX_RETRY_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

/// One failed attempt at a file: whether trying again can help, and whether
/// it got any bytes before it failed.
struct Attempt {
    error: DownloadError,
    transient: bool,
    progressed: bool,
}

/// Waits, checking for cancellation; false if the download was cancelled.
async fn pause(wait: std::time::Duration, stop: &AtomicBool) -> bool {
    let step = std::time::Duration::from_millis(200);
    let mut waited = std::time::Duration::ZERO;
    while waited < wait {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        tokio::time::sleep(step).await;
        waited += step;
    }
    !stop.load(Ordering::Relaxed)
}

/// Brings `part` up to the file's size from where it ends, returning the
/// bytes it then holds.
async fn fetch(
    client: &reqwest::Client,
    file: &PlannedFile,
    location: &Location,
    auth_token: Option<&str>,
    part: &Path,
    progress: &mut dyn FnMut(Phase, u64),
    stop: &AtomicBool,
) -> Result<u64, Attempt> {
    let fatal = |error: DownloadError| Attempt {
        error,
        transient: false,
        progressed: false,
    };
    let existing = location.partial_len().map_err(|e| {
        fatal(DownloadError::new(
            FailureKind::Conflict,
            format!("invalid partial file: {e}"),
        ))
    })?;
    progress(Phase::Downloading, existing);
    if existing > file.expected_bytes {
        return Err(fatal(DownloadError::new(
            FailureKind::Conflict,
            format!(
                "{} is larger than the registry expects; remove it before retrying",
                part.display()
            ),
        )));
    }
    if existing == file.expected_bytes {
        return Ok(existing);
    }
    let mut request = client.get(&file.url);
    if let Some(token) = auth_token.filter(|token| !token.trim().is_empty()) {
        request = request.bearer_auth(token);
    }
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = request.send().await.map_err(|error| Attempt {
        error: DownloadError::new(
            FailureKind::Network,
            format!("cannot download {}: {error}", file.url),
        ),
        transient: true,
        progressed: false,
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
        return Err(Attempt {
            error: DownloadError::new(
                FailureKind::Network,
                format!("{} returned HTTP {status}{hint}", file.url),
            ),
            // The server's own trouble passes; a refusal does not.
            transient: status.is_server_error(),
            progressed: false,
        });
    }
    let append = existing > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    let writer = location.writer(append, existing);
    let mut writer = writer.map_err(|error| {
        fatal(DownloadError::new(
            FailureKind::Io,
            format!("cannot open {}: {error}", part.display()),
        ))
    })?;
    let from = if append { existing } else { 0 };
    let mut written = from;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if stop.load(Ordering::Relaxed) {
            return Err(fatal(DownloadError::new(
                FailureKind::Cancelled,
                "download cancelled; partial data was kept for resume",
            )));
        }
        let chunk = chunk.map_err(|error| Attempt {
            error: DownloadError::new(
                FailureKind::Network,
                format!(
                    "download failed for {}: {error}; the partial file is kept, retry to resume",
                    file.url
                ),
            ),
            transient: true,
            progressed: written > from,
        })?;
        writer.write_all(&chunk).map_err(|error| {
            fatal(DownloadError::new(
                FailureKind::Io,
                format!("cannot write {}: {error}", part.display()),
            ))
        })?;
        written += chunk.len() as u64;
        if written > file.expected_bytes {
            drop(writer);
            let _ = location.remove_partial();
            return Err(fatal(DownloadError::new(
                FailureKind::Verification,
                format!(
                    "{} sent more than the {} bytes declared; the partial file was removed",
                    file.url, file.expected_bytes
                ),
            )));
        }
        progress(Phase::Downloading, written);
    }
    if written < file.expected_bytes {
        // The server closed the connection early: resume from here.
        return Err(Attempt {
            error: DownloadError::new(
                FailureKind::Network,
                format!(
                    "{} ended after {written} of {} bytes; the partial file is kept, retry to resume",
                    file.url, file.expected_bytes
                ),
            ),
            transient: true,
            progressed: written > from,
        });
    }
    Ok(written)
}

#[cfg(unix)]
fn pin_partial_directory(part: &Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags, openat};
    let path = part
        .parent()
        .ok_or(std::io::ErrorKind::InvalidInput)?
        .canonicalize()?;
    let mut directory = std::fs::File::open("/")?;
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            directory = openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?
            .into();
        }
    }
    Ok(directory)
}

#[cfg(unix)]
fn open_partial_at(
    directory: &std::fs::File,
    part: &Path,
    append: bool,
    existing: u64,
) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags, openat};
    use std::os::unix::fs::MetadataExt;
    let name = part.file_name().ok_or(std::io::ErrorKind::InvalidInput)?;
    let flags =
        OFlags::WRONLY | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let flags = if append {
        flags | OFlags::APPEND
    } else {
        flags
    };
    // Never truncate until the opened descriptor itself has been validated.
    let file: std::fs::File = openat(directory, name, flags, Mode::RUSR | Mode::WUSR)?.into();
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 || (append && metadata.len() != existing) {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    if !append {
        file.set_len(0)?;
    }
    Ok(file)
}

/// Publishes a verified `.part` without replacing a destination. A complete, wrong partial
/// is removed: resuming it could never succeed.
fn finish(
    file: &PlannedFile,
    location: &Location,
    observed: Observed,
    status: &str,
) -> Result<(String, Observed), DownloadError> {
    let part = &location.part;
    if !file.matches(&observed) {
        if observed.bytes >= file.expected_bytes {
            let _ = location.remove_partial();
        }
        return Err(DownloadError::new(
            FailureKind::Verification,
            format!(
                "{} downloaded but verification failed: got {} bytes blake3={:?} sha256={:?} git_sha1={:?}",
                file.file, observed.bytes, observed.blake3, observed.sha256, observed.git_sha1
            ),
        ));
    }
    // Atomically create the destination only if absent. rename would replace
    // a file another download/person installed while HTTP was in flight.
    location.publish().map_err(|error| {
        DownloadError::new(
            FailureKind::Io,
            format!(
                "cannot publish {} to {} without overwriting: {error}",
                part.display(),
                file.destination.display()
            ),
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = location
            .read(false)
            .and_then(|f| f.metadata())
            .map_err(|error| {
                DownloadError::new(
                    FailureKind::Io,
                    format!("cannot inspect published artifact: {error}"),
                )
            })?;
        if !metadata.is_file() || observed.identity != Some((metadata.dev(), metadata.ino())) {
            // Preserve the conflicting path: another writer may now own it.
            return Err(DownloadError::new(
                FailureKind::Verification,
                "partial file changed identity during publication",
            ));
        }
    }
    // Publication succeeded. Failure to remove a redundant partial does not
    // invalidate the verified destination; a later retry recognizes it.
    let _ = location.remove_partial();
    location.ensure_current().map_err(|e| {
        DownloadError::new(
            FailureKind::Conflict,
            format!("download destination changed: {e}"),
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
    #[cfg(unix)]
    identity: Option<(u64, u64)>,
    bytes: u64,
    blake3: Option<String>,
    sha256: Option<String>,
    git_sha1: Option<String>,
}

fn observe(path: &Path, needs: Needs) -> Result<Observed, DownloadError> {
    let io = |error: std::io::Error| {
        DownloadError::new(
            FailureKind::Io,
            format!("cannot read {}: {error}", path.display()),
        )
    };
    #[cfg(unix)]
    let file: std::fs::File = {
        use rustix::fs::{Mode, OFlags, openat};
        let directory = pin_partial_directory(path).map_err(io)?;
        let name = path
            .file_name()
            .ok_or_else(|| io(std::io::ErrorKind::InvalidInput.into()))?;
        openat(
            &directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|e| io(e.into()))?
        .into()
    };
    #[cfg(not(unix))]
    let file = std::fs::File::open(path).map_err(io)?;
    observe_file(file, path, needs)
}

fn observe_file(
    mut file: std::fs::File,
    path: &Path,
    needs: Needs,
) -> Result<Observed, DownloadError> {
    use sha1::Digest as _;
    let io = |error: std::io::Error| {
        DownloadError::new(
            FailureKind::Io,
            format!("cannot read {}: {error}", path.display()),
        )
    };
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() {
        return Err(io(std::io::ErrorKind::InvalidInput.into()));
    }
    let length = metadata.len();
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.dev(), metadata.ino()))
    };
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
        #[cfg(unix)]
        identity,
        bytes,
        blake3: blake3.map(|hasher| hasher.finalize().to_hex().to_string()),
        sha256: sha256.map(|hasher| format!("{:x}", hasher.finalize())),
        git_sha1: git_sha1.map(|hasher| format!("{:x}", hasher.finalize())),
    })
}

/// Hash the already opened descriptor off the event loop so cancellation and
/// progress requests stay responsive during multi-gigabyte verification.
async fn observe_location(
    location: &Location,
    partial: bool,
    needs: Needs,
) -> Result<Observed, DownloadError> {
    location.ensure_current().map_err(|e| {
        DownloadError::new(
            FailureKind::Conflict,
            format!("download destination changed: {e}"),
        )
    })?;
    let file = location.read(partial).map_err(|e| {
        DownloadError::new(FailureKind::Io, format!("cannot open anchored file: {e}"))
    })?;
    let path = if partial {
        location.part.clone()
    } else {
        location.path.clone()
    };
    let observed = tokio::task::spawn_blocking(move || observe_file(file, &path, needs))
        .await
        .map_err(|e| DownloadError::new(FailureKind::Io, e.to_string()))??;
    location.ensure_current().map_err(|e| {
        DownloadError::new(
            FailureKind::Conflict,
            format!("download destination changed during verification: {e}"),
        )
    })?;
    Ok(observed)
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

#[cfg(all(test, unix))]
mod publication_identity_tests {
    use super::*;

    #[test]
    fn replacing_a_hashed_partial_is_not_reported_as_verified() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("model.bin");
        let part = part_path(&destination);
        let bytes = b"model bytes";
        let file = PlannedFile {
            file: "model.bin".into(),
            url: String::new(),
            destination,
            expected_bytes: bytes.len() as u64,
            blake3: Some(blake3::hash(bytes).to_hex().to_string()),
            sha256: None,
            git_sha1: None,
        };
        std::fs::write(&part, bytes).unwrap();
        let observed = observe(&part, file.needs()).unwrap();
        std::fs::rename(&part, root.path().join("original")).unwrap();
        std::fs::write(&part, b"bad payload").unwrap();
        let location = Root::new(root.path())
            .unwrap()
            .location(&file.destination)
            .unwrap();
        let error = finish(&file, &location, observed, "downloaded").unwrap_err();
        assert_eq!(error.kind, FailureKind::Verification);
        assert_eq!(std::fs::read(root.path().join("original")).unwrap(), bytes);
    }
}
