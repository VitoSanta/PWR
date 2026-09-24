//! The model manager's core, without the network: Hub responses in the shape
//! the API returned them (recorded 2026-09-23, trimmed), fit on the machines
//! PWR targets, and downloads against a local server.

use pwr_models::catalog::{self, Format};
use pwr_models::download::{
    self, DownloadEvent, DownloadState, FailureKind, LocalState, Phase, Plan, PlannedFile, Progress,
};
use pwr_models::fit::{self, Capacity, FitLevel, Footprint};
use pwr_models::{Filters, apply_filters, hub::HubClient};
use serde_json::json;
use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;

const GIB: u64 = 1024 * 1024 * 1024;
const REV: &str = "0123456789abcdef0123456789abcdef01234567";

fn mlx_listing() -> serde_json::Value {
    json!({
        "_id": "x", "id": "mlx-community/Qwen3-4B-4bit", "author": "mlx-community",
        "cardData": {"license": "apache-2.0", "base_model": "Qwen/Qwen3-4B", "tags": ["mlx"]},
        "gated": false, "likes": 12, "downloads": 34567, "sha": REV,
        "safetensors": {"parameters": {"U32": 3_900_000_000u64, "BF16": 120_000_000u64}, "total": 4_022_468_096u64},
        "tags": ["mlx", "safetensors", "qwen3", "4-bit", "license:apache-2.0"],
        "pipeline_tag": "text-generation"
    })
}

fn mlx_tree() -> serde_json::Value {
    json!([
        {"type": "file", "oid": "52373fe24473b1aa44333d318f578ae6bf04b49b", "size": 1570, "path": ".gitattributes"},
        {"type": "file", "oid": "56a0296b7d72cd9b0438f99913ca38b200a22ce3", "size": 857, "path": "README.md"},
        {"type": "file", "oid": "032ad326a0daaaeeec00f2a585262acf5f692861", "size": 937, "path": "config.json"},
        {"type": "file", "oid": "31349551d90c7606f325fe0f11bbb8bd5fa0d7c7", "size": 1671853, "path": "merges.txt"},
        {"type": "file", "oid": "bb9a9794962c1adf3234c9c4ffe113edbcdc4a34", "size": 2263022529u64,
         "lfs": {"oid": "e240c0bdc0ebb0681bf0da0f98d9719fd6ebe269a3633f81542c13e81345651d", "size": 2263022529u64, "pointerSize": 135},
         "path": "model.safetensors"},
        {"type": "file", "oid": "07e230a328dd17a09d96ee045d49b27596656aff", "size": 63924, "path": "model.safetensors.index.json"},
        {"type": "file", "oid": "1111111111111111111111111111111111111111", "size": 400, "path": "modeling_custom.py"},
        {"type": "directory", "oid": "2222222222222222222222222222222222222222", "size": 0, "path": "images"},
        {"type": "file", "oid": "3333333333333333333333333333333333333333", "size": 9000, "path": "images/chart.png"}
    ])
}

/// Qwen3-4B's own config, as far as the estimate reads it.
fn qwen3_4b_config() -> serde_json::Value {
    json!({
        "model_type": "qwen3", "num_hidden_layers": 36, "num_attention_heads": 32,
        "num_key_value_heads": 8, "head_dim": 128, "hidden_size": 2560,
        "max_position_embeddings": 40960, "torch_dtype": "bfloat16",
        "quantization": {"group_size": 64, "bits": 4}
    })
}

fn gguf_tree() -> serde_json::Value {
    let lfs = |sha: char, size: u64| json!({"oid": sha.to_string().repeat(64), "size": size});
    json!([
        {"type": "file", "oid": "a".repeat(40), "size": 100, "path": "README.md"},
        {"type": "file", "oid": "b".repeat(40), "size": 1, "path": "Qwen3-8B-Q4_K_M.gguf", "lfs": lfs('c', 5_027_783_488)},
        {"type": "file", "oid": "b".repeat(40), "size": 1, "path": "Qwen3-8B-UD-Q4_K_XL.gguf", "lfs": lfs('d', 5_100_000_000)},
        {"type": "file", "oid": "b".repeat(40), "size": 1, "path": "BF16/Qwen3-8B-BF16-00001-of-00002.gguf", "lfs": lfs('e', 9_000_000_000)},
        {"type": "file", "oid": "b".repeat(40), "size": 1, "path": "BF16/Qwen3-8B-BF16-00002-of-00002.gguf", "lfs": lfs('f', 7_400_000_000)},
        {"type": "file", "oid": "b".repeat(40), "size": 1, "path": "Q8_0/Qwen3-8B-Q8_0-00001-of-00002.gguf", "lfs": lfs('1', 5_000_000_000)},
        {"type": "file", "oid": "b".repeat(40), "size": 1, "path": "mmproj-F16.gguf", "lfs": lfs('2', 600_000_000)},
        {"type": "file", "oid": "b".repeat(40), "size": 1, "path": "imatrix_unsloth.gguf", "lfs": lfs('3', 5_000_000)}
    ])
}

fn mac(total_gib: u64) -> Capacity {
    Capacity {
        total_memory_bytes: Some(total_gib * GIB),
        unified_memory: true,
        apple_silicon: true,
        vram_bytes: None,
    }
}

#[test]
fn hub_listings_are_read_as_the_hub_wrote_them() {
    let model = catalog::parse_model(&mlx_listing()).unwrap();
    assert_eq!(model.repository, "mlx-community/Qwen3-4B-4bit");
    assert_eq!(model.revision.as_deref(), Some(REV));
    assert_eq!(model.license.as_deref(), Some("apache-2.0"));
    assert_eq!(model.base_models, vec!["Qwen/Qwen3-4B"]);
    assert_eq!(model.safetensors_parameters, Some(4_022_468_096));
    assert_eq!(model.format(), Some(Format::Mlx));
    assert!(!model.is_gated());
    // Missing fields stay missing.
    let bare = catalog::parse_model(&json!({"id": "someone/thing"})).unwrap();
    assert_eq!(bare.downloads, None);
    assert_eq!(bare.license, None);
    assert_eq!(bare.revision, None);
    // A moving ref is not a revision a download can be pinned to.
    let branch = catalog::parse_model(&json!({"id": "a/b", "sha": "main"})).unwrap();
    assert_eq!(branch.revision, None);
    let gated = catalog::parse_model(&json!({"id": "a/b", "gated": "manual"})).unwrap();
    assert!(gated.is_gated());
}

#[test]
fn an_mlx_variant_takes_the_model_files_and_never_code() {
    let files = catalog::parse_tree(&mlx_tree());
    assert_eq!(files.len(), 8, "directories are not files");
    let variants = catalog::variants(
        "mlx-community/Qwen3-4B-4bit",
        Format::Mlx,
        &files,
        Some(&qwen3_4b_config()),
    );
    assert_eq!(variants.len(), 1);
    let variant = &variants[0];
    let paths: Vec<&str> = variant.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "config.json",
            "merges.txt",
            "model.safetensors",
            "model.safetensors.index.json"
        ]
    );
    assert_eq!(variant.quantization.as_deref(), Some("4-bit"));
    assert_eq!(variant.quantization_source.as_deref(), Some("config"));
    assert_eq!(variant.model_ref, "mlx-community/Qwen3-4B-4bit");
    // LFS files verify by SHA-256, git files by blob SHA-1.
    let weights = variant
        .files
        .iter()
        .find(|f| f.path == "model.safetensors")
        .unwrap();
    assert!(weights.sha256.is_some() && weights.git_sha1.is_none());
    let config = variant
        .files
        .iter()
        .find(|f| f.path == "config.json")
        .unwrap();
    assert!(config.sha256.is_none() && config.git_sha1.is_some());
    assert!(!catalog::needs_remote_code(&qwen3_4b_config()));
    assert!(catalog::needs_remote_code(
        &json!({"auto_map": {"AutoModel": "x.Y"}})
    ));
}

#[test]
fn gguf_variants_are_one_per_quantization_with_every_shard() {
    let files = catalog::parse_tree(&gguf_tree());
    let variants = catalog::variants("unsloth/Qwen3-8B-GGUF", Format::Gguf, &files, None);
    let ids: Vec<&str> = variants.iter().map(|v| v.id.as_str()).collect();
    // The Q8_0 split is missing a shard, so it is not offered; the projector
    // and the importance matrix are not models.
    assert_eq!(
        ids,
        [
            "Qwen3-8B-Q4_K_M.gguf",
            "Qwen3-8B-UD-Q4_K_XL.gguf",
            "BF16/Qwen3-8B-BF16-00001-of-00002.gguf"
        ]
    );
    let bf16 = &variants[2];
    assert_eq!(bf16.files.len(), 2);
    assert_eq!(bf16.bytes, 16_400_000_000);
    assert_eq!(bf16.quantization.as_deref(), Some("BF16"));
    assert_eq!(variants[0].quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(variants[1].quantization.as_deref(), Some("UD-Q4_K_XL"));
    assert_eq!(variants[0].quantization_source.as_deref(), Some("filename"));
    assert_eq!(
        variants[0].model_ref,
        "unsloth/Qwen3-8B-GGUF/Qwen3-8B-Q4_K_M.gguf"
    );
    assert_eq!(
        catalog::gguf_quantization("model-IQ3_XXS.gguf").as_deref(),
        Some("IQ3_XXS")
    );
    assert_eq!(catalog::gguf_quantization("model.gguf"), None);
}

#[test]
fn names_that_could_escape_are_refused() {
    assert!(catalog::is_repository("mlx-community/Qwen3-4B-4bit"));
    for bad in ["../etc", "a/..", "a/b/c", "a", "a/b c", "/a/b"] {
        assert!(!catalog::is_repository(bad), "{bad}");
    }
    assert!(catalog::is_safe_relative("BF16/model-00001-of-00002.gguf"));
    for bad in ["../x.gguf", "/x.gguf", "a/../../x", "C:\\x", "a//b", ""] {
        assert!(!catalog::is_safe_relative(bad), "{bad}");
    }
    assert!(catalog::is_revision(REV));
    assert!(!catalog::is_revision("main"));
}

#[test]
fn a_small_model_is_recommended_on_a_16_gb_mac_and_a_large_one_is_not() {
    let config = qwen3_4b_config();
    let small = fit::estimate(
        &mac(16),
        &Footprint::new(Format::Mlx, 2_300_000_000, Some(&config)),
    );
    assert!(
        matches!(small.level, FitLevel::Recommended | FitLevel::ShouldFit),
        "{small:?}"
    );
    assert!(small.window_tokens.unwrap() >= 16_384);
    assert!(
        small.explanation.contains("16 GB of unified memory"),
        "{}",
        small.explanation
    );
    // File size is not the memory needed: the cache is added.
    assert!(small.expected_bytes > small.weights_bytes + fit::RUNTIME_OVERHEAD_BYTES);

    // 18 GB of weights cannot load where 8 GB is left after the reserve.
    let big = fit::estimate(
        &mac(16),
        &Footprint::new(Format::Mlx, 18 * GIB, Some(&config)),
    );
    assert_eq!(big.level, FitLevel::NotRecommended, "{big:?}");
    // The same model on a 64 GB machine.
    let roomy = fit::estimate(
        &mac(64),
        &Footprint::new(Format::Mlx, 18 * GIB, Some(&config)),
    );
    assert!(roomy.level.fits(), "{roomy:?}");
}

#[test]
fn fit_is_monotonic_in_memory_and_in_size() {
    let config = qwen3_4b_config();
    let order = |level: FitLevel| match level {
        FitLevel::Recommended => 0,
        FitLevel::ShouldFit => 1,
        FitLevel::TightFit => 2,
        FitLevel::NotRecommended => 3,
        _ => 9,
    };
    let mut previous = 0;
    for memory in [128, 64, 32, 24, 16, 8] {
        let level = fit::estimate(
            &mac(memory),
            &Footprint::new(Format::Mlx, 6 * GIB, Some(&config)),
        )
        .level;
        assert!(
            order(level) >= previous,
            "{memory} GB rated better than more memory"
        );
        previous = order(level);
    }
    let mut previous = 0;
    for size in [1, 3, 5, 7, 9, 12] {
        let level = fit::estimate(
            &mac(16),
            &Footprint::new(Format::Mlx, size * GIB, Some(&config)),
        )
        .level;
        assert!(
            order(level) >= previous,
            "{size} GB rated better than a smaller model"
        );
        previous = order(level);
    }
}

#[test]
fn what_cannot_be_judged_is_said_rather_than_guessed() {
    let mlx_on_windows = Capacity {
        total_memory_bytes: Some(32 * GIB),
        unified_memory: false,
        apple_silicon: false,
        vram_bytes: Some(12 * GIB),
    };
    let mlx = fit::estimate(&mlx_on_windows, &Footprint::new(Format::Mlx, 2 * GIB, None));
    assert_eq!(mlx.level, FitLevel::Incompatible);
    let unknown_memory = Capacity {
        total_memory_bytes: None,
        ..mac(16)
    };
    assert_eq!(
        fit::estimate(
            &unknown_memory,
            &Footprint::new(Format::Gguf, 2 * GIB, None)
        )
        .level,
        FitLevel::Unknown
    );
    // No config: the context cost is unknown and the estimate says so.
    let no_shape = fit::estimate(&mac(16), &Footprint::new(Format::Gguf, 2 * GIB, None));
    assert!(no_shape.window_tokens.is_none());
    assert!(
        no_shape.assumptions.iter().any(|a| a.contains("unknown")),
        "{no_shape:?}"
    );
}

#[test]
fn a_discrete_gpu_counts_but_a_split_model_is_never_recommended() {
    let config = qwen3_4b_config();
    let pc = Capacity {
        total_memory_bytes: Some(32 * GIB),
        unified_memory: false,
        apple_silicon: false,
        vram_bytes: Some(12 * GIB),
    };
    let resident = fit::estimate(&pc, &Footprint::new(Format::Gguf, 5 * GIB, Some(&config)));
    assert_eq!(resident.gpu_resident, Some(true));
    let split = fit::estimate(&pc, &Footprint::new(Format::Gguf, 16 * GIB, Some(&config)));
    assert_eq!(split.gpu_resident, Some(false));
    assert_ne!(split.level, FitLevel::Recommended, "{split:?}");
}

fn sample_entries() -> Vec<pwr_models::CatalogEntry> {
    let hub = HubClient::new("https://huggingface.co", None).unwrap();
    let root = tempfile::tempdir().unwrap();
    let mlx_model = catalog::parse_model(&mlx_listing()).unwrap();
    let mlx_files = catalog::parse_tree(&mlx_tree());
    let config = qwen3_4b_config();
    let installed = vec!["mlx-community/Qwen3-4B-4bit".to_owned()];
    let mlx = pwr_models::entry(pwr_models::EntryInput {
        model: &mlx_model,
        format: Format::Mlx,
        files: &mlx_files,
        config: Some(&config),
        capacity: &mac(16),
        models_root: root.path(),
        installed: &installed,
        hub: &hub,
        has_token: false,
    });
    let gguf_model = catalog::parse_model(&json!({
        "id": "unsloth/Qwen3-8B-GGUF", "sha": REV, "tags": ["gguf"],
        "gguf": {"total": 8_190_000_000u64, "architecture": "qwen3", "context_length": 40960},
        "cardData": {"base_model": ["Qwen/Qwen3-8B"], "license": "apache-2.0"}
    }))
    .unwrap();
    let gguf_files = catalog::parse_tree(&gguf_tree());
    let gguf = pwr_models::entry(pwr_models::EntryInput {
        model: &gguf_model,
        format: Format::Gguf,
        files: &gguf_files,
        config: Some(&config),
        capacity: &mac(16),
        models_root: root.path(),
        installed: &[],
        hub: &hub,
        has_token: false,
    });
    vec![mlx, gguf]
}

#[test]
fn a_card_carries_metadata_with_its_source_and_marks_what_is_installed() {
    let entries = sample_entries();
    let mlx = &entries[0];
    assert_eq!(mlx.parameters, Some(4_022_468_096));
    assert_eq!(
        mlx.parameters_source.as_deref(),
        Some("safetensors metadata (Hub)")
    );
    assert_eq!(mlx.context_length, Some(40960));
    assert_eq!(mlx.context_source.as_deref(), Some("config.json"));
    assert_eq!(mlx.architecture.as_deref(), Some("qwen3"));
    assert_eq!(
        mlx.url,
        "https://huggingface.co/mlx-community/Qwen3-4B-4bit"
    );
    assert!(mlx.variants[0].installed);
    assert_eq!(mlx.variants[0].local, LocalState::Missing);
    assert_eq!(mlx.variants[0].blocked, None);
    let gguf = &entries[1];
    assert_eq!(
        gguf.parameters_source.as_deref(),
        Some("GGUF metadata (Hub)")
    );
    assert_eq!(gguf.context_length, Some(40960));
    assert!(!gguf.variants[0].installed);
}

#[test]
fn filters_are_applied_in_the_core() {
    let all = sample_entries();
    let only_gguf_q4 = apply_filters(
        all.clone(),
        &Filters {
            quantization: Some("q4_k".into()),
            ..Filters::default()
        },
    );
    assert_eq!(only_gguf_q4.len(), 1);
    assert_eq!(only_gguf_q4[0].variants.len(), 2);
    let small = apply_filters(
        all.clone(),
        &Filters {
            max_parameters: Some(5_000_000_000),
            ..Filters::default()
        },
    );
    assert_eq!(small.len(), 1);
    assert_eq!(small[0].repository, "mlx-community/Qwen3-4B-4bit");
    let fitting = apply_filters(
        all.clone(),
        &Filters {
            compatible_only: true,
            ..Filters::default()
        },
    );
    // The 16 GB BF16 split does not fit a 16 GB Mac.
    assert!(fitting.iter().all(|entry| {
        entry
            .variants
            .iter()
            .all(|variant| variant.fit.level.fits() || variant.installed)
    }));
    let qwen = apply_filters(
        all,
        &Filters {
            family: Some("QWEN".into()),
            max_bytes: Some(3 * GIB),
            ..Filters::default()
        },
    );
    assert_eq!(qwen.len(), 1);
}

// ------------------------------------------------------------------ downloads

/// Serves `payload` at any path, honouring a `Range: bytes=N-` header, for
/// as many requests as `count`.
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

#[test]
fn plans_stay_inside_the_model_folder_and_need_checksums() {
    let hub = HubClient::new("https://huggingface.co", None).unwrap();
    let root = tempfile::tempdir().unwrap();
    let files = catalog::parse_tree(&gguf_tree());
    let variant = &catalog::variants("unsloth/Qwen3-8B-GGUF", Format::Gguf, &files, None)[2];
    let plan =
        pwr_models::plan_for(&hub, "unsloth/Qwen3-8B-GGUF", REV, variant, root.path()).unwrap();
    assert!(plan.files.iter().all(|f| {
        f.destination
            .starts_with(root.path().join("unsloth/Qwen3-8B-GGUF"))
    }));
    assert_eq!(
        plan.files[0].url,
        format!(
            "https://huggingface.co/unsloth/Qwen3-8B-GGUF/resolve/{REV}/BF16/Qwen3-8B-BF16-00001-of-00002.gguf"
        )
    );
    assert!(
        pwr_models::plan_for(&hub, "unsloth/Qwen3-8B-GGUF", "main", variant, root.path())
            .is_err()
    );
    let mut unsafe_variant = variant.clone();
    unsafe_variant.files[0].path = "../../escape.gguf".into();
    assert!(
        pwr_models::plan_for(
            &hub,
            "unsloth/Qwen3-8B-GGUF",
            REV,
            &unsafe_variant,
            root.path()
        )
        .is_err()
    );
    let mut unverified = variant.clone();
    unverified.files[0].sha256 = None;
    assert!(
        pwr_models::plan_for(&hub, "unsloth/Qwen3-8B-GGUF", REV, &unverified, root.path())
            .is_err()
    );
}

#[test]
fn only_models_a_conversation_can_use_are_offered() {
    let with = |tag: Option<&str>| {
        let mut listing = json!({"id": "a/b"});
        if let Some(tag) = tag {
            listing["pipeline_tag"] = json!(tag);
        }
        catalog::is_language_model(&catalog::parse_model(&listing).unwrap())
    };
    assert!(with(Some("text-generation")));
    assert!(with(Some("image-text-to-text")));
    // Quantizers often leave it out; that is not evidence against the model.
    assert!(with(None));
    for tag in [
        "automatic-speech-recognition",
        "feature-extraction",
        "text-to-image",
        "text-to-speech",
    ] {
        assert!(!with(Some(tag)), "{tag}");
    }
}
