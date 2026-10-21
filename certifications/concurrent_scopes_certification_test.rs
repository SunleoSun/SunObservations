use std::collections::BTreeSet;
use sun_certifications::*;
use sun_observations_certifications::certify as run_scenario;

#[test]
fn concurrent_scopes_have_exact_independent_policy_counts_and_semantic_ids() {
    let records = run_scenario(env!("CARGO_BIN_EXE_scenario"), "concurrent");
    let all = source_slice("concurrent observations", &records);
    let mut proof = certify("concurrent scopes preserve independent budgets and semantic identity");
    proof.check("record count", all.count().expect(8 * (1000 + 431 + 200)));

    let mut actual = BTreeSet::new();
    let mut expected = BTreeSet::new();
    for parent in 0u64..8 {
        for child in 0u64..4 {
            for sequence in 0u64..250 {
                expected.insert((parent, child, sequence));
            }
        }
        let expected_scope = format!("run-{parent}");
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["parent"] == parent)
            .collect();
        let own = source_slice(format!("parent {parent}"), &own);
        proof.section(format!("parent {parent}"), |c| {
            c.check(
                "scope",
                own.matching_count(|r| r["scope"] != expected_scope)
                    .expect(0),
            );
            for (namespace, count) in [
                ("cert::point", 1000usize),
                ("cert::limited", 431),
                ("cert::sample", 200),
            ] {
                c.check(
                    format!("{namespace} count"),
                    own.matching_count(|r| r["namespace"] == namespace)
                        .expect(count),
                );
            }
        });
    }

    for r in &records {
        if r["namespace"] == "cert::point" {
            if let (Some(parent), Some(child), Some(sequence)) = (
                r["data"]["parent"].as_u64(),
                r["data"]["child"].as_u64(),
                r["data"]["sequence"].as_u64(),
            ) {
                actual.insert((parent, child, sequence));
            }
        }
    }

    proof
        .check(
            "unique semantic records",
            all.unique(|r| (r["namespace"].to_string(), r["data"].to_string()))
                .expect_valid(),
        )
        .check(
            "parent bounds",
            all.matching_count(|r| {
                r["data"]["parent"]
                    .as_u64()
                    .is_none_or(|parent| parent >= 8)
            })
            .expect(0),
        )
        .check(
            "child and sequence bounds",
            all.matching_count(|r| {
                let sequence = r["data"]["sequence"].as_u64();
                if r["namespace"] == "cert::sample" {
                    sequence.is_none_or(|sequence| sequence >= 1000)
                } else {
                    r["data"]["child"].as_u64().is_none_or(|child| child >= 4)
                        || sequence.is_none_or(|sequence| sequence >= 250)
                }
            })
            .expect(0),
        )
        .check(
            "point identity fields are typed",
            all.matching_count(|r| {
                r["namespace"] == "cert::point"
                    && (r["data"]["parent"].as_u64().is_none()
                        || r["data"]["child"].as_u64().is_none()
                        || r["data"]["sequence"].as_u64().is_none())
            })
            .expect(0),
        )
        .check(
            "point semantic identities",
            fact("point identities", actual).expect(expected),
        )
        .assert_certify();
}
