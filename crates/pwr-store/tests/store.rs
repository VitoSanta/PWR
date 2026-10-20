use pwr_store::Store;

#[test]
fn a_corrupt_database_is_reported_and_never_replaced_with_an_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.sqlite");
    let original = b"corrupt database retained for assisted recovery";
    std::fs::write(&path, original).unwrap();
    let error = match Store::open(&path) {
        Ok(_) => panic!("corrupt database silently became an empty store"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("storage failure"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn concurrent_connections_keep_a_single_valid_chain() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.sqlite");
    let one = Store::open(&path).unwrap();
    let two = Store::open(&path).unwrap();
    let run = pwr_domain::new_id();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [one, two]
        .into_iter()
        .map(|store| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                for n in 0..100 {
                    store
                        .append(Some(run), "test", serde_json::json!({"n": n}))
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let store = Store::open(&path).unwrap();
    let verdict = store.verify_run_chain(run).unwrap();
    assert_eq!(verdict.events, 200);
    assert!(verdict.intact(), "{verdict:?}");
}
