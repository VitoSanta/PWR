//! Extraction as a capability: what the run gets, and what it is refused.

use pwr_tools::{SandboxPolicy, ToolPolicy, extract_document, read_file};
use std::{path::Path, time::Duration};

fn policy(root: &Path) -> ToolPolicy {
    ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: Vec::new(),
        output_limit: 100_000,
        timeout: Duration::from_secs(10),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    }
}

/// A minimal PDF of the shape the campaign's CV has: a declared Flate content
/// stream, a page tree that states its count, and a link annotation.
fn pdf(lines: &[&str]) -> Vec<u8> {
    use std::io::Write as _;
    let mut content = String::from("BT /F1 11 Tf 72 720 Td\n");
    for line in lines {
        content.push_str(&format!("({line}) Tj 0 -14 Td\n"));
    }
    content.push_str("ET");
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(content.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut out = b"%PDF-1.4\n1 0 obj\n<< /Type /Pages /Count 2 >>\nendobj\n\
                    2 0 obj\n<< /A << /URI (https://github.com/VitoSanta) >> >>\nendobj\n\
                    3 0 obj\n<< /Filter /FlateDecode >>\nstream\n"
        .to_vec();
    out.extend_from_slice(&compressed);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out
}

#[test]
fn a_document_becomes_a_file_the_run_can_read_and_quote_against() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("CV.pdf"),
        pdf(&["Vito Santanelli", "Software Engineer"]),
    )
    .unwrap();
    let policy = policy(root.path());

    // The read refuses, and names the capability that does not.
    let refusal = read_file(&policy, Path::new("CV.pdf"))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("extract_document"), "{refusal}");

    let extracted = extract_document(&policy, Path::new("CV.pdf")).unwrap();
    assert_eq!(extracted.path, "CV.pdf.txt");
    assert_eq!(extracted.pages, Some(2));
    assert_eq!(
        extracted.links,
        vec!["https://github.com/VitoSanta".to_string()]
    );

    // The artifact is a text file like any other, and carries where it came from.
    let read_back = read_file(&policy, Path::new("CV.pdf.txt")).unwrap();
    assert_eq!(read_back.artifact_hash, extracted.expected_hash);
    assert!(read_back.content.contains("# source-hash:"));
    assert!(read_back.content.contains("Vito Santanelli"));
    assert!(read_back.content.contains("Software Engineer"));
}

/// Twice is not an error -- a run that lost the path to a compaction should not
/// be punished for asking again -- but a different extraction over an existing
/// one is refused rather than silently written.
#[test]
fn extracting_the_same_document_twice_is_stable() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("CV.pdf"), pdf(&["once"])).unwrap();
    let policy = policy(root.path());
    let first = extract_document(&policy, Path::new("CV.pdf")).unwrap();
    let second = extract_document(&policy, Path::new("CV.pdf")).unwrap();
    assert_eq!(first.artifact_hash, second.artifact_hash);

    std::fs::write(root.path().join("CV.pdf.txt"), "something else").unwrap();
    let refusal = extract_document(&policy, Path::new("CV.pdf"))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("already exists"), "{refusal}");
}

#[test]
fn a_document_with_no_recoverable_text_is_refused_by_name() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("scan.pdf"),
        b"%PDF-1.4\n<< /Type /Page >>\n",
    )
    .unwrap();
    let refusal = extract_document(&policy(root.path()), Path::new("scan.pdf"))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("OCR"), "{refusal}");
    assert!(
        !root.path().join("scan.pdf.txt").exists(),
        "a refusal writes nothing"
    );
}

#[test]
fn extraction_stays_inside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    assert!(extract_document(&policy(root.path()), Path::new("../CV.pdf")).is_err());
}

/// The fiftieth and last action of the ornith-1.5:35b run of 2026-09-07
/// replaced 11 KiB of working JavaScript with an empty string, and the site it
/// had just built went from every block visible to none.
#[test]
fn a_whole_file_cannot_be_replaced_with_nothing() {
    use pwr_domain::hash_bytes;
    use pwr_tools::apply_replace;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.js"), "console.log('work');").unwrap();
    let policy = policy(root.path());
    let hash = hash_bytes("console.log('work');");

    let refusal = apply_replace(&policy, Path::new("main.js"), &hash, "")
        .unwrap_err()
        .to_string();
    assert!(
        refusal.contains("delete_path"),
        "the refusal names the tool for removal: {refusal}"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("main.js")).unwrap(),
        "console.log('work');",
        "the file is untouched"
    );

    // Writing content is still writing content.
    assert!(apply_replace(&policy, Path::new("main.js"), &hash, "console.log('ok');").is_ok());

    // Writing nothing to a file that is already empty used to be allowed here,
    // on the grounds that a file that is already empty is not being emptied by
    // anything. That is true, and it is also an edit that changes nothing,
    // which is refused for its own reasons now -- so the assertion is that it
    // is refused as a no-op and never as an emptying.
    std::fs::write(root.path().join("empty.js"), "").unwrap();
    let refusal = apply_replace(&policy, Path::new("empty.js"), &hash_bytes(""), "")
        .unwrap_err()
        .to_string();
    assert!(
        refusal.contains("already contains exactly this content"),
        "an empty file written empty is a no-op, not an emptying: {refusal}"
    );
    assert!(
        !refusal.contains("delete_path"),
        "nothing was emptied, so the refusal must not talk about removal: {refusal}"
    );
}

/// A refusal that only names the wall costs actions. The one that names
/// `extract_document` sent both measured deployments straight to it; this is
/// the same shape for the allowlist, which in a workspace that is not yet a
/// project refuses everything.
#[tokio::test]
async fn a_denied_command_says_how_a_workspace_gets_one() {
    use pwr_tools::{Approval, run_command};
    let root = tempfile::tempdir().unwrap();

    let mut granted = policy(root.path());
    granted.approvals = vec![Approval::VerifierProposal];
    let refusal = run_command(&granted, "python3", &["verify.py".into()])
        .await
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("propose_verifier"), "{refusal}");
    assert!(
        refusal.contains("declares none"),
        "an empty list is said to be empty: {refusal}"
    );

    // A project that does declare checks has an allowlist, and the refusal must
    // name it rather than telling the caller it has nothing. Caught live: an
    // Angular workspace whose `npm` was allowlisted all along was told it
    // declared no checks.
    let mut declaring = policy(root.path());
    declaring.allow_commands = vec!["npm".into(), "node".into(), "npx".into()];
    declaring.approvals = vec![Approval::VerifierProposal];
    let refusal = run_command(&declaring, "find", &[".".into()])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        refusal.contains("npm, node, npx"),
        "the refusal names what is permitted: {refusal}"
    );
    assert!(!refusal.contains("declares none"), "{refusal}");

    // Without the grant there is no way to widen it, and the refusal must not
    // send the caller to a tool that would be refused as well.
    let refusal = run_command(&policy(root.path()), "python3", &["verify.py".into()])
        .await
        .unwrap_err()
        .to_string();
    assert!(!refusal.contains("propose_verifier"), "{refusal}");
    assert!(
        refusal.contains("say which command the work needs"),
        "{refusal}"
    );
}

/// Nine of eighty-three actions in the gpt-oss:20b Angular run of 2026-09-07
/// went to `app.module.ts` and `app.component.ts` -- the file names an older
/// Angular structure would have. The workspace had `app.ts` and `app.config.ts`
/// all along, and the refusal was a bare `os error 2`.
#[test]
fn reading_a_file_that_is_not_there_says_what_is() {
    use pwr_tools::read_file;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::write(root.path().join("src/app/app.ts"), "export class App {}").unwrap();
    std::fs::write(
        root.path().join("src/app/app.config.ts"),
        "export const c = {};",
    )
    .unwrap();
    std::fs::write(root.path().join("src/app/hero.ts"), "export class Hero {}").unwrap();
    let policy = policy(root.path());

    let refusal = read_file(&policy, Path::new("src/app/app.module.ts"))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("does not exist"), "{refusal}");
    assert!(
        refusal.contains("src/app/app.ts"),
        "it names the near miss: {refusal}"
    );
    assert!(refusal.contains("app.config.ts"), "{refusal}");
    assert!(
        !refusal.contains("hero.ts"),
        "an unrelated file is not a suggestion: {refusal}"
    );

    // Nothing similar is said plainly rather than with an empty list.
    let refusal = read_file(&policy, Path::new("zzz.rs"))
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("Nothing in the workspace"), "{refusal}");

    // A file that is there is still read.
    assert!(read_file(&policy, Path::new("src/app/app.ts")).is_ok());
}
