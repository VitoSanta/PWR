//! Downloads against a local server: verification, resume, refusal.

use pwr_models::download::{
    self, DownloadEvent, DownloadState, FailureKind, LocalState, Phase, Plan, PlannedFile, Progress,
};
use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;

fn serve(payload: Vec<u8>, count: usize) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(count) {
            let mut stream = stream.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]).to_ascii_lowercase();
            let start = text
                .lines()
                .find_map(|line| line.strip_prefix("range: bytes="))
                .and_then(|range| {
                    range
                        .trim_end_matches('-')
                        .trim()
                        .trim_end_matches('-')
                        .parse::<usize>()
                        .ok()
                });
            let (status, body) = match start {
                Some(start) => ("206 Partial Content", &payload[start..]),
                None => ("200 OK", &payload[..]),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
        }
    });
    format!("http://{address}/file")
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn git_sha1(bytes: &[u8]) -> String {
    use sha1::Digest as _;
    let mut hasher = sha1::Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn plan_of(root: &std::path::Path, files: Vec<(&str, &[u8], String, bool)>) -> Plan {
    Plan {
        id: "test".into(),
        destination_root: root.to_path_buf(),
        files: files
            .into_iter()
            .map(|(name, bytes, url, lfs)| PlannedFile {
                file: name.into(),
                url,
                destination: root.join(name),
                expected_bytes: bytes.len() as u64,
                blake3: None,
                sha256: lfs.then(|| sha256(bytes)),
                git_sha1: (!lfs).then(|| git_sha1(bytes)),
            })
            .collect(),
        revision: None,
    }
}

fn run(
    plan: &Plan,
) -> (
    Result<Vec<download::FileOutcome>, download::DownloadError>,
    DownloadState,
) {
    let mut state = DownloadState::Preparing;
    let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = reqwest::Client::new();
        download::download(
            &client,
            plan,
            None,
            &mut |progress| state = state.clone().on(&DownloadEvent::Progress(progress.clone())),
            &AtomicBool::new(false),
        )
        .await
    });
    let state = match &result {
        Ok(_) => state.on(&DownloadEvent::Finished),
        Err(error) => state.on(&DownloadEvent::Failed(error.clone())),
    };
    (result, state)
}

#[test]
fn a_download_verifies_lfs_and_git_files_and_completes() {
    let dir = tempfile::tempdir().unwrap();
    let weights = b"weights weights weights".to_vec();
    let config = b"{\"model_type\": \"qwen3\"}".to_vec();
    let plan = plan_of(
        dir.path(),
        vec![
            ("config.json", &config, serve(config.clone(), 1), false),
            (
                "model.safetensors",
                &weights,
                serve(weights.clone(), 1),
                true,
            ),
        ],
    );
    assert_eq!(download::local_state(&plan).0, LocalState::Missing);
    let (result, state) = run(&plan);
    let outcomes = result.unwrap();
    assert_eq!(outcomes[0].status, "downloaded");
    assert_eq!(
        state,
        DownloadState::Completed {
            total: plan.total_bytes()
        }
    );
    assert_eq!(
        std::fs::read(dir.path().join("model.safetensors")).unwrap(),
        weights
    );
    assert_eq!(download::local_state(&plan).0, LocalState::Present);
    // A second run finds it and fetches nothing (the servers are gone).
    let (again, _) = run(&plan);
    assert!(again.unwrap().iter().all(|o| o.status == "already_present"));
}

#[test]
fn a_part_file_is_resumed_and_a_wrong_file_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let payload = b"0123456789abcdefghij".to_vec();
    let plan = plan_of(
        dir.path(),
        vec![("model.gguf", &payload, serve(payload.clone(), 1), true)],
    );
    std::fs::write(
        download::part_path(&dir.path().join("model.gguf")),
        &payload[..7],
    )
    .unwrap();
    assert_eq!(download::local_state(&plan), (LocalState::Partial, 7));
    let (result, _) = run(&plan);
    assert_eq!(result.unwrap()[0].status, "resumed");
    assert_eq!(
        std::fs::read(dir.path().join("model.gguf")).unwrap(),
        payload
    );

    // An unrelated file where the model would go is left alone.
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("model.gguf"), b"someone else's file").unwrap();
    let plan = plan_of(
        other.path(),
        vec![("model.gguf", &payload, "http://127.0.0.1:9/x".into(), true)],
    );
    let (result, state) = run(&plan);
    let error = result.unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert!(matches!(
        state,
        DownloadState::Failed {
            kind: FailureKind::Conflict,
            ..
        }
    ));
    assert_eq!(
        std::fs::read(other.path().join("model.gguf")).unwrap(),
        b"someone else's file"
    );
    assert!(download::preflight(&plan, u64::MAX).is_err());
}

#[cfg(unix)]
#[test]
fn a_partial_download_symlink_is_refused_without_touching_its_target() {
    use std::os::unix::fs::symlink;

    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("valuable.bin");
    std::fs::write(&target, b"keep these bytes unchanged").unwrap();
    let expected = b"model bytes";
    let plan = plan_of(
        destination.path(),
        vec![("model.gguf", expected, "http://127.0.0.1:9/x".into(), true)],
    );
    symlink(
        &target,
        download::part_path(&destination.path().join("model.gguf")),
    )
    .unwrap();

    let (result, _) = run(&plan);
    let error = result.unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"keep these bytes unchanged"
    );
}

/// Like `serve`, but the first response promises the whole payload and
/// hangs up after `cut` bytes: a connection dropped in the middle of a file.
fn serve_dropping_once(payload: Vec<u8>, cut: usize) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for (index, stream) in listener.incoming().take(2).enumerate() {
            let mut stream = stream.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]).to_ascii_lowercase();
            let start = text
                .lines()
                .find_map(|line| line.strip_prefix("range: bytes="))
                .and_then(|range| range.trim().trim_end_matches('-').parse::<usize>().ok());
            if index == 0 {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = stream.write_all(&payload[..cut]);
                continue;
            }
            let start = start.unwrap_or(0);
            let _ = write!(
                stream,
                "HTTP/1.1 206 Partial Content\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                payload.len() - start
            );
            let _ = stream.write_all(&payload[start..]);
        }
    });
    format!("http://{address}/file")
}

#[test]
fn a_transfer_dropped_midway_resumes_by_itself_and_completes() {
    let dir = tempfile::tempdir().unwrap();
    let payload: Vec<u8> = (0..200_000_u32).map(|i| (i % 251) as u8).collect();
    let plan = plan_of(
        dir.path(),
        vec![(
            "model.safetensors",
            &payload,
            serve_dropping_once(payload.clone(), 70_000),
            true,
        )],
    );
    let (result, state) = run(&plan);
    assert_eq!(result.unwrap()[0].status, "downloaded");
    assert_eq!(
        state,
        DownloadState::Completed {
            total: plan.total_bytes()
        }
    );
    assert_eq!(
        std::fs::read(dir.path().join("model.safetensors")).unwrap(),
        payload
    );
}

#[test]
fn corrupted_bytes_fail_verification_and_are_not_kept() {
    let dir = tempfile::tempdir().unwrap();
    let expected = b"the right bytes!".to_vec();
    let wrong = b"the wrong bytes!".to_vec();
    let plan = plan_of(
        dir.path(),
        vec![("model.gguf", &expected, serve(wrong, 1), true)],
    );
    let (result, state) = run(&plan);
    assert_eq!(result.unwrap_err().kind, FailureKind::Verification);
    assert!(matches!(
        state,
        DownloadState::Failed {
            kind: FailureKind::Verification,
            ..
        }
    ));
    assert!(!dir.path().join("model.gguf").exists());
    assert!(!download::part_path(&dir.path().join("model.gguf")).exists());
}

#[test]
fn a_download_that_does_not_fit_on_disk_is_refused_before_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let payload = vec![7_u8; 1000];
    let plan = plan_of(
        dir.path(),
        vec![("model.gguf", &payload, "http://127.0.0.1:9/x".into(), true)],
    );
    let error = download::preflight(&plan, 10).unwrap_err();
    assert_eq!(error.kind, FailureKind::InsufficientDisk);
    let ok = download::preflight(&plan, download::DISK_MARGIN_BYTES + 1000).unwrap();
    assert_eq!(ok.required_bytes, 1000);
    // Offline: a clear network failure, and the state says so.
    let (result, state) = run(&plan);
    assert_eq!(result.unwrap_err().kind, FailureKind::Network);
    assert!(matches!(
        state,
        DownloadState::Failed {
            kind: FailureKind::Network,
            ..
        }
    ));
}

#[test]
fn a_finished_download_state_cannot_be_revived_by_a_late_report() {
    let progress = |phase, bytes| {
        DownloadEvent::Progress(Progress {
            phase,
            file: "f".into(),
            file_bytes: bytes,
            file_total: 100,
            bytes,
            total: 100,
        })
    };
    let state = DownloadState::Preparing
        .on(&progress(Phase::Downloading, 40))
        .on(&DownloadEvent::Failed(download::DownloadError {
            kind: FailureKind::Cancelled,
            message: "cancelled".into(),
        }));
    assert_eq!(state, DownloadState::Cancelled { bytes: 40 });
    assert_eq!(state.clone().on(&progress(Phase::Downloading, 60)), state);
    assert_eq!(state.clone().on(&DownloadEvent::Finished), state);
    let done = DownloadState::Preparing
        .on(&progress(Phase::Downloading, 100))
        .on(&progress(Phase::Verifying, 100))
        .on(&DownloadEvent::Finished);
    assert_eq!(done, DownloadState::Completed { total: 100 });
}

#[cfg(unix)]
#[test]
fn replacing_a_partial_with_a_symlink_during_http_cannot_truncate_its_target() {
    late_partial_link_attack(false);
    late_partial_link_attack(true);
}

#[cfg(unix)]
fn late_partial_link_attack(hard_link: bool) {
    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("valuable.bin");
    std::fs::write(&target, b"keep these bytes unchanged").unwrap();
    let partial = download::part_path(&destination.path().join("model.gguf"));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let linked_target = target.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        if hard_link {
            std::fs::hard_link(linked_target, partial).unwrap();
        } else {
            std::os::unix::fs::symlink(linked_target, partial).unwrap();
        }
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\nconnection: close\r\n\r\nmodel bytes",
            )
            .unwrap();
    });
    let plan = plan_of(
        destination.path(),
        vec![(
            "model.gguf",
            b"model bytes",
            format!("http://{address}/file"),
            true,
        )],
    );
    let (result, _) = run(&plan);
    server.join().unwrap();
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(target).unwrap(),
        b"keep these bytes unchanged"
    );
}

#[test]
fn a_destination_created_during_http_is_never_overwritten() {
    let destination = tempfile::tempdir().unwrap();
    let target = destination.path().join("model.gguf");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let created_target = target.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        std::fs::write(created_target, b"a concurrently installed file").unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\nconnection: close\r\n\r\nmodel bytes",
            )
            .unwrap();
    });
    let plan = plan_of(
        destination.path(),
        vec![(
            "model.gguf",
            b"model bytes",
            format!("http://{address}/file"),
            true,
        )],
    );
    let (result, _) = run(&plan);
    server.join().unwrap();
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(target).unwrap(),
        b"a concurrently installed file"
    );
}

#[cfg(unix)]
#[test]
fn a_revision_marker_symlink_never_changes_its_external_target() {
    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("valuable.txt");
    std::fs::write(&target, b"keep these bytes unchanged").unwrap();
    std::os::unix::fs::symlink(&target, destination.path().join(download::REVISION_FILE)).unwrap();
    let mut plan = plan_of(
        destination.path(),
        vec![(
            "model.gguf",
            b"model bytes",
            serve(b"model bytes".to_vec(), 1),
            true,
        )],
    );
    plan.revision = Some("1234567890123456789012345678901234567890".into());
    assert!(run(&plan).0.is_ok());
    assert_eq!(
        std::fs::read(target).unwrap(),
        b"keep these bytes unchanged"
    );
}

#[cfg(unix)]
#[test]
fn replacing_the_download_root_during_http_cannot_publish_outside_it() {
    let sandbox = tempfile::tempdir().unwrap();
    let destination = sandbox.path().join("models");
    let old = sandbox.path().join("original");
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(&destination).unwrap();
    let external_part = outside.path().join("model.gguf.part");
    std::fs::write(&external_part, b"model bytes").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let redirect = outside.path().to_path_buf();
    let replaced = destination.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        std::fs::rename(&replaced, old).unwrap();
        std::os::unix::fs::symlink(redirect, replaced).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\nconnection: close\r\n\r\nmodel bytes",
            )
            .unwrap();
    });
    let plan = plan_of(
        &destination,
        vec![(
            "model.gguf",
            b"model bytes",
            format!("http://{address}/file"),
            true,
        )],
    );
    let result = run(&plan).0;
    server.join().unwrap();
    assert!(
        result.is_err(),
        "a renamed root must not be reported as a named installed artifact"
    );
    assert!(!outside.path().join("model.gguf").exists());
    assert_eq!(std::fs::read(external_part).unwrap(), b"model bytes");
}

#[cfg(unix)]
#[test]
fn nested_download_directories_cannot_escape_through_symlinks() {
    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), destination.path().join("nested")).unwrap();
    let plan = plan_of(
        destination.path(),
        vec![(
            "nested/model.gguf",
            b"model bytes",
            "http://127.0.0.1:9/file".into(),
            true,
        )],
    );
    let error = run(&plan).0.unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn a_download_destination_must_be_beneath_its_declared_root() {
    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut plan = plan_of(
        destination.path(),
        vec![(
            "model.gguf",
            b"model bytes",
            "http://127.0.0.1:9/file".into(),
            true,
        )],
    );
    plan.files[0].destination = outside.path().join("model.gguf");
    let error = run(&plan).0.unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    plan.files[0].destination = destination.path().join("../model.gguf");
    assert_eq!(run(&plan).0.unwrap_err().kind, FailureKind::Conflict);
}
