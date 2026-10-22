use sun_observations::certifications::*;
use sun_observations_certifications::run_scenario;

#[test]
fn public_overlapping_channels_preserve_exact_multiplicity_and_envelope() {
    let records = run_scenario(env!("CARGO_BIN_EXE_scenario"), "fanout");
    let evidence = collection("fanout records", &records);
    certification("overlapping channels preserve exact multiplicity and envelope")
        .check("record count", evidence.count().expect(100))
        .check(
            "namespace",
            evidence
                .matching_count(|r| r["namespace"] != "cert::point")
                .expect(0),
        )
        .check(
            "sequence identity",
            fact(
                "fanout records",
                records
                    .iter()
                    .enumerate()
                    .filter(|(sequence, r)| {
                        r["data"]["sequence"].as_u64() != Some(*sequence as u64)
                    })
                    .count(),
            )
            .expect(0),
        )
        .check(
            "channel is not persisted",
            evidence
                .matching_count(|r| r.get("channel").is_some())
                .expect(0),
        )
        .assert_certify();
}
