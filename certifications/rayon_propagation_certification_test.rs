use sun_observations_certifications::certify;
#[test]
fn public_nested_parallel_work_persists_pool_construction_scope() {
    let records = certify(env!("CARGO_BIN_EXE_scenario"), "rayon");
    assert_eq!(records.len(), 2 * (1024 + 311));
    for (parent, scope) in [(0, "rayon-a"), (1, "rayon-b")] {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["parent"] == parent)
            .collect();
        assert!(own.iter().all(|r| r["scope"] == scope));
        assert_eq!(
            own.iter()
                .filter(|r| r["namespace"] == "cert::limited")
                .count(),
            311
        );
    }
}
