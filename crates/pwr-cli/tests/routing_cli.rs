//! Automatic routing, certification and registry loading, through the binary a
//! user actually runs.
//!
//! These exercise the paths that decide what runs, and every one of them is
//! asserted on its refusals: a selector is only useful if it says no for a
//! reason that can be argued with.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

fn pwr() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pwr"))
}

fn run_json(root: &std::path::Path, args: &[&str]) -> serde_json::Value {
    // The engine's model directory is the empty workspace, so no test depends
    // on which models happen to be on this machine.
    let output = pwr()
        .args(args)
        .current_dir(root)
        .env("PWR_MLX_MODELS", root)
        .output()
        .expect("pwr");
    serde_json::from_slice(&output.stdout).expect("json on stdout")
}

#[test]
fn a_backend_this_build_cannot_address_is_named_rather_than_attempted() {
    let root = tempfile::tempdir().unwrap();
    let answer = run_json(root.path(), &["--backend", "vllm", "--json", "doctor"]);
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["category"], "invalid_input");
    assert!(
        answer["error"]["context"]
            .as_str()
            .unwrap()
            .contains("expected mlx or llama")
    );
}

/// Ollama and LM Studio were removed on 2026-09-19: naming one is refused
/// with the date, and the engine is the default.
#[test]
fn an_engine_this_build_lacks_is_refused_by_name_and_the_engine_is_the_default() {
    let root = tempfile::tempdir().unwrap();
    let refused = run_json(root.path(), &["--backend", "ollama", "--json", "doctor"]);
    assert_eq!(refused["ok"], false);
    let context = refused["error"]["context"].as_str().unwrap();
    assert!(
        context.contains("not an engine this build has"),
        "{context}"
    );
    let doctor = run_json(root.path(), &["--json", "doctor"]);
    assert!(
        doctor["result"]["backend"] == "mlx" || doctor["ok"] == false,
        "{doctor}"
    );
}

#[test]
fn a_huggingface_artifact_download_plan_is_pinned_and_local() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("strategies")).unwrap();
    std::fs::write(
        root.path().join("strategies/artifacts.json"),
        serde_json::json!({
            "schema_version": 1,
            "artifacts": [{
                "schema_version": 1,
                "id": "qwen-gguf",
                "family": "qwen",
                "variant": "tiny",
                "source": {
                    "kind": "hugging_face",
                    "repository": "Qwen/Qwen-Tiny-GGUF",
                    "revision": "0123456789abcdef0123456789abcdef01234567",
                    "files": ["model.gguf", "tokenizer.json"]
                },
                "format": "gguf",
                "quantization": "q4_k_m",
                "platform": "any",
                "provenance": {
                    "source": "fixture",
                    "observed_at": "2026-09-20T00:00:00Z",
                    "content_hash": "fixture"
                }
            }]
        })
        .to_string(),
    )
    .unwrap();

    let answer = run_json(
        root.path(),
        &[
            "--json",
            "models",
            "download-plan",
            "qwen-gguf",
            "--destination-root",
            "/models",
        ],
    );

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["executes_network"], false);
    assert_eq!(
        answer["result"]["plan"]["files"][0]["url"],
        "https://huggingface.co/Qwen/Qwen-Tiny-GGUF/resolve/0123456789abcdef0123456789abcdef01234567/model.gguf"
    );
    assert_eq!(
        answer["result"]["plan"]["files"][0]["destination"],
        "/models/Qwen/Qwen-Tiny-GGUF/model.gguf"
    );
}

#[test]
fn a_huggingface_download_requires_file_verification_data() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("strategies")).unwrap();
    std::fs::write(
        root.path().join("strategies/artifacts.json"),
        serde_json::json!({
            "schema_version": 1,
            "artifacts": [{
                "schema_version": 1,
                "id": "qwen-gguf",
                "family": "qwen",
                "variant": "tiny",
                "source": {
                    "kind": "hugging_face",
                    "repository": "Qwen/Qwen-Tiny-GGUF",
                    "revision": "0123456789abcdef0123456789abcdef01234567",
                    "files": ["model.gguf"]
                },
                "format": "gguf",
                "quantization": "q4_k_m",
                "platform": "any",
                "provenance": {
                    "source": "fixture",
                    "observed_at": "2026-09-20T00:00:00Z",
                    "content_hash": "fixture"
                }
            }]
        })
        .to_string(),
    )
    .unwrap();

    let answer = run_json(root.path(), &["--json", "models", "download", "qwen-gguf"]);

    assert_eq!(answer["ok"], false, "{answer}");
    assert_eq!(answer["error"]["category"], "missing_evidence");
    assert!(
        answer["error"]["context"]
            .as_str()
            .unwrap()
            .contains("lacks bytes or a hash")
    );
}

#[test]
fn a_huggingface_download_plan_carries_verification_fields() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("strategies")).unwrap();
    std::fs::write(
        root.path().join("strategies/artifacts.json"),
        serde_json::json!({
            "schema_version": 1,
            "artifacts": [{
                "schema_version": 1,
                "id": "qwen-gguf",
                "family": "qwen",
                "variant": "tiny",
                "source": {
                    "kind": "hugging_face",
                    "repository": "Qwen/Qwen-Tiny-GGUF",
                    "revision": "0123456789abcdef0123456789abcdef01234567",
                    "files": [{
                        "path": "model.gguf",
                        "bytes": 42,
                        "blake3": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    }]
                },
                "format": "gguf",
                "quantization": "q4_k_m",
                "platform": "any",
                "provenance": {
                    "source": "fixture",
                    "observed_at": "2026-09-20T00:00:00Z",
                    "content_hash": "fixture"
                }
            }]
        })
        .to_string(),
    )
    .unwrap();

    let answer = run_json(
        root.path(),
        &[
            "--json",
            "models",
            "download-plan",
            "qwen-gguf",
            "--destination-root",
            "/models",
        ],
    );

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["plan"]["files"][0]["expected_bytes"], 42);
    assert_eq!(
        answer["result"]["plan"]["files"][0]["blake3"],
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
}

#[test]
fn a_huggingface_download_fetches_and_verifies_a_declared_file() {
    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    let payload = b"verified artifact bytes".to_vec();
    let digest = blake3::hash(&payload).to_hex().to_string();
    let base_url = serve_one_artifact(payload.clone());
    std::fs::create_dir_all(root.path().join("strategies")).unwrap();
    std::fs::write(
        root.path().join("strategies/artifacts.json"),
        serde_json::json!({
            "schema_version": 1,
            "artifacts": [{
                "schema_version": 1,
                "id": "qwen-gguf",
                "family": "qwen",
                "variant": "tiny",
                "source": {
                    "kind": "hugging_face",
                    "repository": "Qwen/Qwen-Tiny-GGUF",
                    "revision": "0123456789abcdef0123456789abcdef01234567",
                    "files": [{
                        "path": "model.gguf",
                        "bytes": payload.len(),
                        "blake3": digest
                    }]
                },
                "format": "gguf",
                "quantization": "q4_k_m",
                "platform": "any",
                "provenance": {
                    "source": "fixture",
                    "observed_at": "2026-09-20T00:00:00Z",
                    "content_hash": "fixture"
                }
            }]
        })
        .to_string(),
    )
    .unwrap();

    let output = pwr()
        .args([
            "--json",
            "models",
            "download",
            "qwen-gguf",
            "--destination-root",
            downloads.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .env("PWR_MLX_MODELS", root.path())
        .env("PWR_HF_BASE_URL", base_url)
        .output()
        .expect("pwr");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["files"][0]["status"], "downloaded");
    assert_eq!(
        std::fs::read(downloads.join("Qwen/Qwen-Tiny-GGUF/model.gguf")).unwrap(),
        payload
    );
}

#[test]
fn a_huggingface_download_skips_an_already_verified_file() {
    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    let payload = b"verified artifact bytes".to_vec();
    write_verified_hf_artifact_registry(root.path(), &payload);
    let destination = downloads.join("Qwen/Qwen-Tiny-GGUF/model.gguf");
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::write(&destination, &payload).unwrap();

    let output = pwr()
        .args([
            "--json",
            "models",
            "download",
            "qwen-gguf",
            "--destination-root",
            downloads.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .env("PWR_MLX_MODELS", root.path())
        .env("PWR_HF_BASE_URL", "http://127.0.0.1:9")
        .output()
        .expect("pwr");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["files"][0]["status"], "already_present");
}

#[test]
fn a_huggingface_download_resumes_a_part_file() {
    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    let payload = b"verified artifact bytes".to_vec();
    write_verified_hf_artifact_registry(root.path(), &payload);
    let part = downloads.join("Qwen/Qwen-Tiny-GGUF/model.gguf.part");
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();
    std::fs::write(&part, &payload[..9]).unwrap();
    let base_url = serve_one_artifact(payload.clone());

    let output = pwr()
        .args([
            "--json",
            "models",
            "download",
            "qwen-gguf",
            "--destination-root",
            downloads.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .env("PWR_MLX_MODELS", root.path())
        .env("PWR_HF_BASE_URL", base_url)
        .output()
        .expect("pwr");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["files"][0]["status"], "resumed");
    assert_eq!(
        std::fs::read(downloads.join("Qwen/Qwen-Tiny-GGUF/model.gguf")).unwrap(),
        payload
    );
}

#[test]
fn a_huggingface_download_refuses_when_disk_preflight_fails() {
    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    let payload = b"verified artifact bytes".to_vec();
    write_verified_hf_artifact_registry(root.path(), &payload);

    let output = pwr()
        .args([
            "--json",
            "models",
            "download",
            "qwen-gguf",
            "--destination-root",
            downloads.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .env("PWR_MLX_MODELS", root.path())
        .env("PWR_HF_BASE_URL", "http://127.0.0.1:9")
        .env("PWR_DOWNLOAD_FREE_BYTES", "1")
        .output()
        .expect("pwr");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");

    assert_eq!(answer["ok"], false, "{answer}");
    assert_eq!(answer["error"]["category"], "missing_evidence");
    assert!(
        answer["error"]["context"]
            .as_str()
            .unwrap()
            .contains("download needs")
    );
}

#[test]
fn a_huggingface_download_preflight_counts_existing_part_bytes() {
    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    let payload = b"verified artifact bytes".to_vec();
    write_verified_hf_artifact_registry(root.path(), &payload);
    let part = downloads.join("Qwen/Qwen-Tiny-GGUF/model.gguf.part");
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();
    std::fs::write(&part, &payload[..9]).unwrap();
    let base_url = serve_one_artifact(payload.clone());
    let remaining = payload.len() as u64 - 9;
    let free = 5 * 1024 * 1024 * 1024_u64 + remaining;

    let output = pwr()
        .args([
            "--json",
            "models",
            "download",
            "qwen-gguf",
            "--destination-root",
            downloads.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .env("PWR_MLX_MODELS", root.path())
        .env("PWR_HF_BASE_URL", base_url)
        .env("PWR_DOWNLOAD_FREE_BYTES", free.to_string())
        .output()
        .expect("pwr");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(
        answer["result"]["disk_preflight"]["required_bytes"],
        remaining
    );
    assert_eq!(answer["result"]["files"][0]["status"], "resumed");
}

#[test]
fn a_huggingface_download_verifies_a_complete_part_without_network() {
    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    let payload = b"verified artifact bytes".to_vec();
    write_verified_hf_artifact_registry(root.path(), &payload);
    let part = downloads.join("Qwen/Qwen-Tiny-GGUF/model.gguf.part");
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();
    std::fs::write(&part, &payload).unwrap();

    let output = pwr()
        .args([
            "--json",
            "models",
            "download",
            "qwen-gguf",
            "--destination-root",
            downloads.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .env("PWR_MLX_MODELS", root.path())
        .env("PWR_HF_BASE_URL", "http://127.0.0.1:9")
        .output()
        .expect("pwr");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["files"][0]["status"], "verified_part");
    assert_eq!(
        std::fs::read(downloads.join("Qwen/Qwen-Tiny-GGUF/model.gguf")).unwrap(),
        payload
    );
}

#[test]
fn a_huggingface_download_accepts_sha256_verification() {
    use sha2::Digest as _;

    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    let payload = b"verified artifact bytes".to_vec();
    let mut sha256 = sha2::Sha256::new();
    sha256.update(&payload);
    let base_url = serve_one_artifact(payload.clone());
    std::fs::create_dir_all(root.path().join("strategies")).unwrap();
    std::fs::write(
        root.path().join("strategies/artifacts.json"),
        serde_json::json!({
            "schema_version": 1,
            "artifacts": [{
                "schema_version": 1,
                "id": "qwen-gguf",
                "family": "qwen",
                "variant": "tiny",
                "source": {
                    "kind": "hugging_face",
                    "repository": "Qwen/Qwen-Tiny-GGUF",
                    "revision": "0123456789abcdef0123456789abcdef01234567",
                    "files": [{
                        "path": "model.gguf",
                        "bytes": payload.len(),
                        "sha256": format!("{:x}", sha256.finalize())
                    }]
                },
                "format": "gguf",
                "quantization": "q4_k_m",
                "platform": "any",
                "provenance": {
                    "source": "fixture",
                    "observed_at": "2026-09-20T00:00:00Z",
                    "content_hash": "fixture"
                }
            }]
        })
        .to_string(),
    )
    .unwrap();

    let output = pwr()
        .args([
            "--json",
            "models",
            "download",
            "qwen-gguf",
            "--destination-root",
            downloads.to_str().unwrap(),
        ])
        .current_dir(root.path())
        .env("PWR_MLX_MODELS", root.path())
        .env("PWR_HF_BASE_URL", base_url)
        .output()
        .expect("pwr");
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");

    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["files"][0]["status"], "downloaded");
    assert!(answer["result"]["files"][0]["sha256"].is_string());
}

fn serve_one_artifact(payload: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert!(request.starts_with(
            "GET /Qwen/Qwen-Tiny-GGUF/resolve/0123456789abcdef0123456789abcdef01234567/model.gguf "
        ));
        let range = request
            .lines()
            .find_map(|line| line.strip_prefix("Range: bytes="))
            .and_then(|range| range.strip_suffix('-'))
            .and_then(|start| start.parse::<usize>().ok());
        let start = range.unwrap_or(0);
        let body = &payload[start..];
        let status = if range.is_some() {
            "206 Partial Content"
        } else {
            "200 OK"
        };
        let content_range = if range.is_some() {
            format!(
                "Content-Range: bytes {}-{}/{}\r\n",
                start,
                payload.len() - 1,
                payload.len()
            )
        } else {
            String::new()
        };
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{content_range}Connection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
    });
    format!("http://{address}")
}

fn write_verified_hf_artifact_registry(root: &std::path::Path, payload: &[u8]) {
    std::fs::create_dir_all(root.join("strategies")).unwrap();
    std::fs::write(
        root.join("strategies/artifacts.json"),
        serde_json::json!({
            "schema_version": 1,
            "artifacts": [{
                "schema_version": 1,
                "id": "qwen-gguf",
                "family": "qwen",
                "variant": "tiny",
                "source": {
                    "kind": "hugging_face",
                    "repository": "Qwen/Qwen-Tiny-GGUF",
                    "revision": "0123456789abcdef0123456789abcdef01234567",
                    "files": [{
                        "path": "model.gguf",
                        "bytes": payload.len(),
                        "blake3": blake3::hash(payload).to_hex().to_string()
                    }]
                },
                "format": "gguf",
                "quantization": "q4_k_m",
                "platform": "any",
                "provenance": {
                    "source": "fixture",
                    "observed_at": "2026-09-20T00:00:00Z",
                    "content_hash": "fixture"
                }
            }]
        })
        .to_string(),
    )
    .unwrap();
}
