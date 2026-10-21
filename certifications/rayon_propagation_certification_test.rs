use sun_certifications::*;
use sun_observations_certifications::certify as run_scenario;

#[test]
fn public_nested_parallel_work_persists_pool_construction_scope() {
    let records = run_scenario(env!("CARGO_BIN_EXE_scenario"), "rayon");
    let all = source_slice("rayon observations", &records);
    let mut proof = certify("nested parallel work preserves pool construction scope");
    proof.check("record count", all.count().compare(Equal(2 * (1024 + 311))));
    for (parent, expected_scope) in [(0, "rayon-a"), (1, "rayon-b")] {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["parent"] == parent)
            .collect();
        let own = source_slice(format!("parent {parent}"), &own);
        proof.section(format!("parent {parent}"), |c| {
            c.check(
                "scope",
                own.matching_count(|r| r["scope"] != expected_scope)
                    .compare(Equal(0)),
            )
            .check(
                "limited budget",
                own.matching_count(|r| r["namespace"] == "cert::limited")
                    .compare(Equal(311)),
            );
        });
    }
    proof.assert_certify();
}
