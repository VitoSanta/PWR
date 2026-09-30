use pwr_store::Store;
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
