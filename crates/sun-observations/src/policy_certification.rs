use super::*;

#[test]
fn non_counting_policies_do_not_touch_counter_and_counting_never_wraps() {
    let mut certification = sun_observations::certifications::certify(
        "non counting policies do not touch counter and counting never wraps",
    );
    use std::sync::atomic::{AtomicU64, Ordering};
    let counter = AtomicU64::new(7);
    certification.check_fact_expect(
        "Policy::Always.admit_atomic(&counter)",
        Policy::Always.admit_atomic(&counter),
        true,
    );
    certification.check_fact_expect(
        "Policy::Never.admit_atomic(&counter)",
        Policy::Never.admit_atomic(&counter),
        false,
    );
    certification.check_fact_expect(
        "counter.load(Ordering::Relaxed)",
        &(counter.load(Ordering::Relaxed)),
        &(7),
    );
    counter.store(u64::MAX, Ordering::Relaxed);
    certification.check_fact_expect(
        "Policy::FirstN(u64::MAX).admit_atomic(&counter)",
        Policy::FirstN(u64::MAX).admit_atomic(&counter),
        true,
    );
    certification.check_fact_expect(
        "Policy::EveryN(2).admit_atomic(&counter)",
        Policy::EveryN(2).admit_atomic(&counter),
        false,
    );
    certification.check_fact_expect(
        "counter.load(Ordering::Relaxed)",
        &(counter.load(Ordering::Relaxed)),
        &(u64::MAX),
    );
    certification.assert_certify();
}

#[test]
fn admission_sequences() {
    let mut certification = sun_observations::certifications::certify("admission sequences");
    for (policy, expected) in [
        (Policy::Always, vec![true; 6]),
        (Policy::Never, vec![false; 6]),
        (
            Policy::FirstN(2),
            vec![true, true, false, false, false, false],
        ),
        (
            Policy::MaxPerScope(2),
            vec![true, true, false, false, false, false],
        ),
        (
            Policy::EveryN(2),
            vec![false, true, false, true, false, true],
        ),
        (Policy::FirstN(0), vec![false; 6]),
        (Policy::MaxPerScope(0), vec![false; 6]),
        (Policy::EveryN(0), vec![false; 6]),
    ] {
        let counter = std::sync::atomic::AtomicU64::new(0);
        let actual: Vec<_> = (0..6).map(|_| policy.admit_atomic(&counter)).collect();
        certification.check_fact_expect(format!("{policy:?}"), &(actual), &(expected));
    }
    certification.assert_certify();
}
