use sun_observations::certifications::*;

fn main() {
    let produced = [(1u64, 2.0), (2, 4.0), (3, 7.0)];
    let consumed = [produced[2], produced[0], produced[1]];
    let mut proof = certification("ordinary producer/consumer contract");
    let left = proof.collection("produced", &produced);
    let right = proof.collection("consumed", &consumed);
    let pair = left.compare_with(&right).on(|row| row.0, |row| row.0);
    proof.check("keys", pair.paired_comparison_fact().expect_valid());
    proof.check(
        "values",
        pair.equal(|row| row.1, |row| row.1, 3).expect_valid(),
    );
    let values = pair.numeric(|row| row.1, |row| row.1);
    proof.check(
        "association",
        values
            .correlation()
            .expect(CorrelationExpectation::spearman_at_least(0.99).unwrap()),
    );
    proof.check(
        "difference",
        values
            .difference()
            .expect(DifferenceExpectation::max_absolute_at_most(0.0).unwrap()),
    );
    proof.check(
        "mean",
        left.numeric(|row| row.1).summary().expect(criterion(
            "finite three-row summary",
            ViolationClass::Hard,
            |summary: &Summary| summary.count == 3 && summary.mean.is_finite(),
        )),
    );
    proof.assert_certify();
}
