use std::collections::BTreeSet;
use sun_observations_certifications::{certify, pair};
#[test]
fn release_gate_combines_admission_scopes_threads_routing_spans_jsonl_and_mutation() {
    let records = certify(env!("CARGO_BIN_EXE_scenario"), "runtime");
    let points = 480 + if cfg!(feature = "rayon") { 30 } else { 0 };
    assert_eq!(records.len(), 4 * (points + 73 + 160 + 16));
    pair(&records, 32);
    let mut unique = BTreeSet::new();
    for parent in 0..4 {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["scope"] == format!("release-{parent}"))
            .collect();
        assert_eq!(own.len(), points + 73 + 160 + 16);
        for r in &own {
            if r["phase"] != "end" {
                assert_eq!(r["data"]["parent"], parent);
                match r["namespace"].as_str().unwrap() {
                    "cert::work" => assert!(r["data"]["child"].as_u64().unwrap() < 8),
                    "cert::sample" => assert!(r["data"]["sequence"].as_u64().unwrap() < 480),
                    "cert::limited" => {
                        assert!(r["data"]["child"].as_u64().unwrap() < 4);
                        assert!(r["data"]["sequence"].as_u64().unwrap() < 60);
                    }
                    "cert::point" => (),
                    other => panic!("unexpected namespace {other}"),
                }
                assert!(unique.insert((r["namespace"].to_string(), r["data"].to_string())));
            }
        }
        for (namespace, n) in [
            ("cert::point", points),
            ("cert::limited", 73),
            ("cert::sample", 160),
            ("cert::work", 16),
        ] {
            assert_eq!(
                own.iter().filter(|r| r["namespace"] == namespace).count(),
                n
            );
        }
        let mut expected = BTreeSet::new();
        for child in 0..4 {
            for sequence in 0..120 {
                expected.insert((child, sequence));
            }
        }
        if cfg!(feature = "rayon") {
            for sequence in 0..30 {
                expected.insert((99, sequence));
            }
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
        assert_eq!(actual, expected);
    }
    assert!(records.iter().all(|r| r["namespace"] != "cert::blocked"));
}
