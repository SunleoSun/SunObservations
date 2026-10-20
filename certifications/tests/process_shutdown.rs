use sun_observations_certifications::certify;
#[test]
fn local_last_owner_drop_drains_before_real_child_exit() {
    check("local-drop");
}
#[test]
fn global_explicit_barrier_persists_before_real_child_exit() {
    check("global-flush");
}
fn check(scenario: &str) {
    let records = certify(env!("CARGO_BIN_EXE_scenario"), scenario);
    assert_eq!(records.len(), 40000);
    for (i, r) in records.iter().enumerate() {
        assert_eq!(r["data"]["sequence"], i);
    }
}
