//! What a fetch puts in front of the model, and what it writes instead.

use pwr_tools::{Approval, SandboxPolicy, ToolPolicy, fetch_url};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

/// Serves one response to one request, and returns its URL.
fn serve_once(content_type: Option<&str>, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let content_type = content_type.map(str::to_owned);
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        let mut head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        if let Some(kind) = content_type {
            head.push_str(&format!("Content-Type: {kind}\r\n"));
        }
        head.push_str("\r\n");
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&body);
    });
    format!("http://{address}/file")
}

fn policy(root: &Path) -> ToolPolicy {
    ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: Vec::new(),
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(10),
        sandbox: SandboxPolicy::Disabled,
        approvals: vec![Approval::NetworkAccess],
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

/// An archive is not shown as text: it is named, sized, and the way to
/// download it is said.
#[test]
fn an_archive_is_described_not_shown() {
    let root = tempfile::tempdir().unwrap();
    let archive: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let url = serve_once(Some("application/gzip"), archive);
    let result = block_on(fetch_url(&policy(root.path()), &url, None)).unwrap();
    assert!(
        result.content.is_empty(),
        "{} bytes shown",
        result.content.len()
    );
    assert_eq!(result.bytes, Some(200_000));
    let note = result.note.unwrap_or_default();
    assert!(note.contains("save_as"), "{note}");
}

/// Without a media type, a NUL in the first bytes is enough to tell.
#[test]
fn an_unlabelled_binary_is_recognised() {
    let root = tempfile::tempdir().unwrap();
    let url = serve_once(None, b"\x7fELF\x02\x01\x01\x00\x00\x00binary".to_vec());
    let result = block_on(fetch_url(&policy(root.path()), &url, None)).unwrap();
    assert!(result.content.is_empty(), "{:?}", result.content);
    assert!(result.note.unwrap_or_default().contains("save_as"));
}

/// Saved whole, where it was asked for, with the checksum a release publishes.
#[test]
fn a_download_is_written_to_the_workspace_with_its_sha256() {
    let root = tempfile::tempdir().unwrap();
    let url = serve_once(Some("application/octet-stream"), b"hello\n".to_vec());
    let result = block_on(fetch_url(
        &policy(root.path()),
        &url,
        Some(".toolchains/hello.bin"),
    ))
    .unwrap();
    assert_eq!(
        std::fs::read(root.path().join(".toolchains/hello.bin")).unwrap(),
        b"hello\n"
    );
    assert_eq!(result.saved_as.as_deref(), Some(".toolchains/hello.bin"));
    assert_eq!(result.bytes, Some(6));
    assert_eq!(
        result.sha256.as_deref(),
        Some("5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03")
    );
    assert!(result.content.is_empty());
    // Nothing half-written is left beside it.
    let leftovers: Vec<_> = std::fs::read_dir(root.path().join(".toolchains"))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "{leftovers:?}");
}

/// Refused before any traffic: outside the workspace, or over a file.
#[test]
fn a_download_cannot_leave_the_workspace_or_overwrite() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("kept.txt"), "mine").unwrap();
    let policy = policy(root.path());
    for target in ["../escaped.bin", "kept.txt"] {
        let refused = block_on(fetch_url(&policy, "http://127.0.0.1:9/never", Some(target)));
        assert!(refused.is_err(), "{target}");
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join("kept.txt")).unwrap(),
        "mine"
    );
}

/// A long listing is cut to what a context can hold, and the cut is said.
#[test]
fn a_long_text_is_cut_and_says_how_to_have_it_all() {
    let root = tempfile::tempdir().unwrap();
    let listing = "go1.27.1.darwin-arm64.tar.gz 4f0e1c9b2d...\n".repeat(5_000);
    let url = serve_once(Some("application/json"), listing.into_bytes());
    let result = block_on(fetch_url(&policy(root.path()), &url, None)).unwrap();
    assert!(result.truncated);
    assert!(
        result.content.len() <= 24 * 1024,
        "{}",
        result.content.len()
    );
    assert!(result.note.unwrap_or_default().contains("save_as"));
}
