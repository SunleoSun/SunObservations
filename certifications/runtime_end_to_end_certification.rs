use std::collections::BTreeSet;
use sun_observations_certifications::{certify as capture, pair};
#[test]
fn release_gate_combines_admission_scopes_threads_routing_spans_jsonl_and_mutation() {
    use sun_observations::certifications::{certify as proof, *};
    let records = capture(env!("CARGO_BIN_EXE_scenario"), "runtime");
    let points = 510;
    let mut c = proof("runtime production contract");
    c.check(
        "total delivery",
        fact("persisted artifact", records.len()).expect(4 * (points + 73 + 160 + 16)),
    );
    pair(&records, 32);
    let mut unique = BTreeSet::new();
    let mut duplicate_count = 0;
    for parent in 0..4 {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["scope"] == format!("release-{parent}"))
            .collect();
        c.section(format!("execution {parent}"), |c| {
            c.check(
                "scope count",
                fact("scope", own.len()).expect(points + 73 + 160 + 16),
            );
            let valid = own.iter().filter(|r| r["phase"] != "end").all(|r| {
                r["data"]["parent"] == parent
                    && match r["namespace"].as_str() {
                        Some("cert::work") => r["data"]["child"].as_u64().is_some_and(|v| v < 8),
                        Some("cert::sample") => {
                            r["data"]["sequence"].as_u64().is_some_and(|v| v < 480)
                        }
                        Some("cert::limited") => {
                            r["data"]["child"].as_u64().is_some_and(|v| v < 4)
                                && r["data"]["sequence"].as_u64().is_some_and(|v| v < 60)
                        }
                        Some("cert::point") => true,
                        _ => false,
                    }
            });
            c.check(
                "propagated payload and admission bounds",
                fact("records", valid).expect(true),
            );
            for r in own.iter().filter(|r| r["phase"] != "end") {
                if !unique.insert((r["namespace"].to_string(), r["data"].to_string())) {
                    duplicate_count += 1;
                }
            }
            for (namespace, n) in [
                ("cert::point", points),
                ("cert::limited", 73),
                ("cert::sample", 160),
                ("cert::work", 16),
            ] {
                c.check(
                    namespace,
                    fact(
                        "namespace count",
                        own.iter().filter(|r| r["namespace"] == namespace).count(),
                    )
                    .expect(n),
                );
            }
            let mut expected = BTreeSet::new();
            for child in 0..4 {
                for sequence in 0..120 {
                    expected.insert((child, sequence));
                }
            }
            for sequence in 0..30 {
                expected.insert((99, sequence));
            }
            let actual: BTreeSet<_> = own
                .iter()
                .filter(|r| r["namespace"] == "cert::point")
                .map(|r| {
                    (
                        r["data"]["child"].as_u64().unwrap(),
                        r["data"]["sequence"].as_u64().unwrap(),
                    )
                })
                .collect();
            c.check(
                "exact domain correspondence",
                fact("child and sequence", actual).expect(expected),
            );
        });
    }
    c.check(
        "no duplicate points",
        fact("delivery", duplicate_count).expect(0),
    )
    .check(
        "forbidden namespace absent",
        source_slice("artifact", &records)
            .matching_count(|r| r["namespace"] == "cert::blocked")
            .expect(0),
    )
    .assert_certify();
}
