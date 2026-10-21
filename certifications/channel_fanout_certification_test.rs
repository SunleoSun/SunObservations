use sun_certifications::*;
use sun_observations_certifications::certify as run_scenario;

#[test]
fn public_overlapping_channels_preserve_exact_multiplicity_and_envelope() {
    let records = run_scenario(env!("CARGO_BIN_EXE_scenario"), "fanout");
    let evidence = source_slice("fanout records", &records);
    certify("overlapping channels preserve exact multiplicity and envelope")
        .check("record count", evidence.count().compare(Equal(100)))
        .check(
            "namespace",
            evidence
                .matching_count(|r| r["namespace"] != "cert::point")
                .compare(Equal(0)),
        )
        .check(
            "sequence identity",
            measured(
                "fanout records",
                records
                    .iter()
                    .enumerate()
                    .filter(|(sequence, r)| {
                        r["data"]["sequence"].as_u64() != Some(*sequence as u64)
                    })
                    .count(),
            )
            .compare(Equal(0)),
        )
        .check(
            "channel is not persisted",
            evidence
                .matching_count(|r| r.get("channel").is_some())
                .compare(Equal(0)),
        )
        .assert_certify();
}
