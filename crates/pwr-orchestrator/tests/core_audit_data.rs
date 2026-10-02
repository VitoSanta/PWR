use pwr_orchestrator::{graph, personal::Home, wiki};
#[test]
fn retrieval_hash_and_wiki_freshness_follow_current_bytes() {
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().to_owned();
    let root = base.join("wiki");
    std::fs::create_dir_all(&root).unwrap();
    let home = Home(base.join("home"));
    let path = root.join("needle.rs");
    std::fs::write(&path, "pub fn needle() -> &'static str { \"old\" }\n").unwrap();
    let (index, _) = pwr_repo::index_incremental(&root, Some(&root.join(".pwr"))).unwrap();
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::write(&path, "pub fn needle() -> &'static str { \"new\" }\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(mtime))
        .unwrap();
    let (cached, work) = pwr_repo::index_incremental(&root, Some(&root.join(".pwr"))).unwrap();
    let excerpts = pwr_repo::retrieve(&root, &cached, "needle", 5, 4096);
    let actual = pwr_domain::hash_bytes(std::fs::read(&path).unwrap());
    let excerpts = excerpts.unwrap();
    assert!(!excerpts.is_empty());
    assert_eq!(excerpts[0].content_hash, actual);
    println!(
        "retrieval_hash: reused={} read={} cached_hash={} actual_hash={} excerpts={:?}",
        work.reused, work.read, index.files[0].content_hash, actual, excerpts
    );
    let index = wiki::index(&root).unwrap();
    let id = "project";
    let summary = graph::Summary {
        text: "SUMMARY_SENTINEL: needle returns new".into(),
        source_hash: graph::source_hash(&index, id).unwrap(),
        model: "fake".into(),
        at: pwr_domain::now().to_rfc3339(),
    };
    wiki::save_summary(&root, id, summary).unwrap();
    wiki::rebuild_graph(&home, &root).unwrap();
    std::fs::write(
        &path,
        "pub fn needle() -> &'static str { \"externally changed\" }\n",
    )
    .unwrap();
    let query = wiki::query(&home, Some(&root), "", "wiki");
    assert!(query.contains("files changed since"));
    println!(
        "wiki_stale: contains_summary={} warns_changed={} query={:?}",
        query.contains("SUMMARY_SENTINEL"),
        query.contains("files changed since"),
        query
    );
    println!(
        "memory_default: absent={} empty_json={}",
        pwr_orchestrator::personal::load_profile(&home)
            .unwrap()
            .memory_enabled,
        serde_json::from_str::<pwr_orchestrator::personal::Profile>("{}")
            .unwrap()
            .memory_enabled
    );
}
