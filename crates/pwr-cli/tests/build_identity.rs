#[allow(dead_code)]
#[path = "../build.rs"]
mod identity;

#[test]
fn source_identity_changes_for_uncommitted_edits_additions_and_deletions() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("crates/example/src");
    std::fs::create_dir_all(&source).unwrap();
    let path = source.join("lib.rs");
    std::fs::write(&path, "original").unwrap();
    // Present throughout, so deleting `lib.rs` leaves a source to fingerprint:
    // an empty tree is an error, tested below, not a revision.
    std::fs::write(source.join("keep.rs"), "kept").unwrap();
    let original = identity::source_fingerprint(root.path()).unwrap();
    std::fs::write(&path, "modified").unwrap();
    assert_ne!(original, identity::source_fingerprint(root.path()).unwrap());
    std::fs::write(&path, "original").unwrap();
    assert_eq!(original, identity::source_fingerprint(root.path()).unwrap());
    let added = source.join("new.rs");
    std::fs::write(&added, "new").unwrap();
    assert_ne!(original, identity::source_fingerprint(root.path()).unwrap());
    std::fs::remove_file(&added).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_ne!(original, identity::source_fingerprint(root.path()).unwrap());
}

#[test]
fn a_tree_with_no_source_is_refused_rather_than_given_the_empty_hash() {
    // BLAKE3 of nothing is af1349b9...3262, a plausible-looking revision that
    // 146 campaign reports carried when builds could not see the source.
    let root = tempfile::tempdir().unwrap();
    assert!(identity::source_fingerprint(root.path()).is_err());
    std::fs::create_dir_all(root.path().join("crates")).unwrap();
    assert!(identity::source_fingerprint(root.path()).is_err());
}
