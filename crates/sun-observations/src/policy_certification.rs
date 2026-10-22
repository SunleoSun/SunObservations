use super::*;

#[test]
fn counting_policy_construction_rejects_zero_instead_of_assigning_magic_behavior() {
    let mut c = sun_observations::certifications::certification("positive policy counts");
    c.check_fact_expect("zero first", Policy::first_n(0), None);
    c.check_fact_expect("zero interval", Policy::every_n(0), None);
    c.check_fact_expect("positive first", Policy::first_n(1).is_some(), true);
    c.check_fact_expect("positive interval", Policy::every_n(1).is_some(), true);
    c.assert_certify();
}

#[test]
fn non_counting_policies_do_not_touch_counter_and_counting_never_wraps() {
    let mut certification = sun_observations::certifications::certification(
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
        "Policy::first_n(u64::MAX).unwrap().admit_atomic(&counter)",
        Policy::first_n(u64::MAX).unwrap().admit_atomic(&counter),
        true,
    );
    certification.check_fact_expect(
        "Policy::every_n(2).unwrap().admit_atomic(&counter)",
        Policy::every_n(2).unwrap().admit_atomic(&counter),
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
    let mut certification = sun_observations::certifications::certification("admission sequences");
    for (policy, expected) in [
        (Policy::Always, vec![true; 6]),
        (Policy::Never, vec![false; 6]),
        (
            Policy::first_n(2).unwrap(),
            vec![true, true, false, false, false, false],
        ),
        (
            Policy::first_n(2).unwrap(),
            vec![true, true, false, false, false, false],
        ),
        (
            Policy::every_n(2).unwrap(),
            vec![false, true, false, true, false, true],
        ),
        (Policy::Never, vec![false; 6]),
        (Policy::Never, vec![false; 6]),
        (Policy::Never, vec![false; 6]),
    ] {
        let counter = std::sync::atomic::AtomicU64::new(0);
        let actual: Vec<_> = (0..6).map(|_| policy.admit_atomic(&counter)).collect();
        certification.check_fact_expect(format!("{policy:?}"), &(actual), &(expected));
    }
    certification.assert_certify();
}
