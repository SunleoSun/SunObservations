use sun_observations_certifications::certify;
#[test]
fn public_overlapping_channels_preserve_exact_multiplicity_and_envelope() {
    let records = certify(env!("CARGO_BIN_EXE_scenario"), "fanout");
    assert_eq!(records.len(), 100);
    for (sequence, r) in records.iter().enumerate() {
        assert_eq!(r["namespace"], "cert::point");
        assert_eq!(r["data"]["sequence"], sequence);
        assert!(r.get("channel").is_none());
    }
}
