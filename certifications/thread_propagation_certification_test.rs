use sun_certifications::*;
use sun_observations_certifications::certify as run_scenario;

#[test]
fn public_transitive_threads_persist_only_their_parent_scope() {
    let records = run_scenario(env!("CARGO_BIN_EXE_scenario"), "threads");
    let all = source_slice("thread observations", &records);
    let mut proof = certify("transitive threads preserve parent scope and policy budgets");
    proof.check("record count", all.count().compare(Equal(6 * (800 + 257))));
    for parent in 0..6 {
        let expected_scope = format!("thread-{parent}");
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
                    .compare(Equal(257)),
            )
            .check(
                "point count",
                own.matching_count(|r| r["namespace"] == "cert::point")
                    .compare(Equal(800)),
            );
        });
    }
    proof.assert_certify();
}
