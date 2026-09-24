//! Models on disk: what is listed, and what a deletion may and may not touch.

use pwr_models::catalog::Format;
use pwr_models::local;
use std::path::Path;

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![0_u8; bytes]).unwrap();
}

fn models() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    write(&r.join("mlx-community/Qwen3-4B-4bit/config.json"), 10);
    write(
        &r.join("mlx-community/Qwen3-4B-4bit/model.safetensors"),
        1000,
    );
    write(
        &r.join("mlx-community/Half-4bit/model.safetensors.part"),
        300,
    );
    write(&r.join("unsloth/Qwen3-8B-GGUF/Qwen3-8B-Q4_K_M.gguf"), 500);
    write(
        &r.join("unsloth/Qwen3-8B-GGUF/BF16/Qwen3-8B-BF16-00001-of-00002.gguf"),
        700,
    );
    write(
        &r.join("unsloth/Qwen3-8B-GGUF/BF16/Qwen3-8B-BF16-00002-of-00002.gguf"),
        600,
    );
    write(&r.join("unsloth/Qwen3-8B-GGUF/mmproj-F16.gguf"), 50);
    write(&r.join("someone/notes/readme.txt"), 5);
    root
}

#[test]
fn models_on_disk_are_listed_once_each_with_their_size() {
    let root = models();
    let mlx = local::list(root.path(), Format::Mlx);
    let refs: Vec<(&str, bool, u64)> = mlx
        .iter()
        .map(|m| (m.model_ref.as_str(), m.partial, m.bytes))
        .collect();
    assert_eq!(
        refs,
        [
            ("mlx-community/Half-4bit", true, 300),
            ("mlx-community/Qwen3-4B-4bit", false, 1010)
        ]
    );
    let gguf = local::list(root.path(), Format::Gguf);
    let refs: Vec<(&str, usize, u64)> = gguf
        .iter()
        .map(|m| (m.model_ref.as_str(), m.files, m.bytes))
        .collect();
    assert_eq!(
        refs,
        [
            (
                "unsloth/Qwen3-8B-GGUF/BF16/Qwen3-8B-BF16-00001-of-00002.gguf",
                2,
                1300
            ),
            ("unsloth/Qwen3-8B-GGUF/Qwen3-8B-Q4_K_M.gguf", 1, 500),
        ]
    );
}

#[test]
fn deleting_an_mlx_model_removes_its_folder_and_nothing_else() {
    let root = models();
    let done = local::delete(root.path(), Format::Mlx, "mlx-community/Qwen3-4B-4bit").unwrap();
    assert_eq!(done.freed_bytes, 1010);
    assert!(!root.path().join("mlx-community/Qwen3-4B-4bit").exists());
    // The partial download next to it, and everything else, stay.
    assert!(
        root.path()
            .join("mlx-community/Half-4bit/model.safetensors.part")
            .exists()
    );
    assert!(root.path().join("someone/notes/readme.txt").exists());
    // An unfinished download can be discarded the same way; its now-empty
    // owner folder goes too, the models folder itself never.
    local::delete(root.path(), Format::Mlx, "mlx-community/Half-4bit").unwrap();
    assert!(!root.path().join("mlx-community").exists());
    assert!(root.path().exists());
}

#[test]
fn deleting_a_gguf_split_removes_every_shard_and_leaves_other_quantizations() {
    let root = models();
    let done = local::delete(
        root.path(),
        Format::Gguf,
        "unsloth/Qwen3-8B-GGUF/BF16/Qwen3-8B-BF16-00001-of-00002.gguf",
    )
    .unwrap();
    assert_eq!(done.removed.len(), 2);
    assert_eq!(done.freed_bytes, 1300);
    assert!(!root.path().join("unsloth/Qwen3-8B-GGUF/BF16").exists());
    assert!(
        root.path()
            .join("unsloth/Qwen3-8B-GGUF/Qwen3-8B-Q4_K_M.gguf")
            .exists()
    );
    assert!(
        root.path()
            .join("unsloth/Qwen3-8B-GGUF/mmproj-F16.gguf")
            .exists()
    );
}

#[test]
fn a_deletion_never_leaves_the_models_folder_or_takes_what_is_not_a_model() {
    let root = models();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("precious/config.json"), 10);
    for bad in ["../x", "/etc", "mlx-community", "", "a/../../b"] {
        assert!(
            local::delete(root.path(), Format::Mlx, bad).is_err(),
            "{bad}"
        );
    }
    // Not a model: no config.json.
    assert!(local::delete(root.path(), Format::Mlx, "someone/notes").is_err());
    assert!(root.path().join("someone/notes/readme.txt").exists());
    // A symlink to a folder elsewhere is not followed out of the root.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            outside.path().join("precious"),
            root.path().join("someone/link"),
        )
        .unwrap();
        assert!(local::delete(root.path(), Format::Mlx, "someone/link").is_err());
        assert!(outside.path().join("precious/config.json").exists());
    }
    // A folder inside a model folder is not deleted with it.
    write(
        &root
            .path()
            .join("mlx-community/Qwen3-4B-4bit/extra/keep.txt"),
        1,
    );
    assert!(local::delete(root.path(), Format::Mlx, "mlx-community/Qwen3-4B-4bit").is_err());
    assert!(
        root.path()
            .join("mlx-community/Qwen3-4B-4bit/model.safetensors")
            .exists()
    );
    // Something missing is said to be missing.
    assert!(local::delete(root.path(), Format::Gguf, "unsloth/none.gguf").is_err());
}
