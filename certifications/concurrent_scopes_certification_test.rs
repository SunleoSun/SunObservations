use std::collections::BTreeSet;
use sun_observations_certifications::certify;
#[test]
fn concurrent_scopes_have_exact_independent_policy_counts_and_semantic_ids() {
    let records = certify(env!("CARGO_BIN_EXE_scenario"), "concurrent");
    assert_eq!(records.len(), 8 * (1000 + 431 + 200));
    let mut actual = BTreeSet::new();
    let mut expected = BTreeSet::new();
    for parent in 0..8 {
        for child in 0..4 {
            for sequence in 0..250 {
                expected.insert((parent, child, sequence));
            }
        }
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["parent"] == parent)
            .collect();
        assert!(own.iter().all(|r| r["scope"] == format!("run-{parent}")));
        for (namespace, n) in [
            ("cert::point", 1000),
            ("cert::limited", 431),
            ("cert::sample", 200),
        ] {
            assert_eq!(
                own.iter().filter(|r| r["namespace"] == namespace).count(),
                n
            );
        }
    }
    let mut unique = BTreeSet::new();
    for r in &records {
        assert!(unique.insert((r["namespace"].to_string(), r["data"].to_string())));
        assert!(r["data"]["parent"].as_u64().unwrap() < 8);
        if r["namespace"] == "cert::sample" {
            assert!(r["data"]["sequence"].as_u64().unwrap() < 1000);
        } else {
            assert!(r["data"]["child"].as_u64().unwrap() < 4);
            assert!(r["data"]["sequence"].as_u64().unwrap() < 250);
        }
        if r["namespace"] == "cert::point" {
            actual.insert((
                r["data"]["parent"].as_u64().unwrap(),
                r["data"]["child"].as_u64().unwrap(),
                r["data"]["sequence"].as_u64().unwrap(),
            ));
        }
    }
    assert_eq!(actual, expected);
}
