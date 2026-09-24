//! A commit is a readable label; source bytes identify an uncommitted build.
use std::path::Path;

fn files(directory: &Path, found: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            files(&path, found)?;
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "toml")
        ) {
            found.push(path);
        }
    }
    Ok(())
}

pub fn source_fingerprint(root: &Path) -> std::io::Result<String> {
    let mut paths = vec![];
    files(&root.join("crates"), &mut paths)?;
    // A fingerprint of nothing is BLAKE3's empty hash, af1349b9...3262, and
    // it is a valid-looking revision. Builds that could not see the source
    // stamped exactly that for days, and 146 campaign reports carry it:
    // distinct harnesses under one identity, which is the confusion the
    // revision exists to prevent. No source is a build error, not a label.
    if paths.is_empty() {
        return Err(std::io::Error::other(format!(
            "no harness source found under {}; refusing to stamp a revision that cannot say \
             what it was built from",
            root.join("crates").display()
        )));
    }
    for name in ["Cargo.toml", "Cargo.lock", "docs/thresholds.json"] {
        if root.join(name).exists() {
            paths.push(root.join(name));
        }
    }
    paths.sort();
    let mut hash = blake3::Hasher::new();
    for path in paths {
        let name = path
            .strip_prefix(root)
            .expect("source below root")
            .to_string_lossy();
        let bytes = std::fs::read(&path)?;
        hash.update(&(name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    Ok(hash.finalize().to_hex().to_string())
}

fn main() {
    // Read when the script runs, not baked in when it was compiled: a build
    // script binary reused from another checkout would otherwise fingerprint
    // a path that is not this one.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let root = Path::new(&manifest).join("../..");
    let commit = std::process::Command::new("git")
        .current_dir(&root)
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into());
    let source = source_fingerprint(&root)
        .unwrap_or_else(|error| panic!("cannot fingerprint harness source: {error}"));
    println!("cargo:rustc-env=POORAI_HARNESS_REV={commit}-source-{source}");
    for name in [
        "crates",
        "Cargo.toml",
        "Cargo.lock",
        "docs/thresholds.json",
        ".git/HEAD",
        ".git/index",
        ".git/refs",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(name).display());
    }
}
