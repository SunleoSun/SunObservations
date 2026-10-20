use sun_observations_certifications::certify;
#[test]
fn public_transitive_threads_persist_only_their_parent_scope() {
    let records = certify(env!("CARGO_BIN_EXE_scenario"), "threads");
    assert_eq!(records.len(), 6 * (800 + 257));
    for parent in 0..6 {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["parent"] == parent)
            .collect();
        assert!(own.iter().all(|r| r["scope"] == format!("thread-{parent}")));
        assert_eq!(
            own.iter()
                .filter(|r| r["namespace"] == "cert::limited")
                .count(),
            257
        );
        assert_eq!(
            own.iter()
                .filter(|r| r["namespace"] == "cert::point")
                .count(),
            800
        );
    }
}
