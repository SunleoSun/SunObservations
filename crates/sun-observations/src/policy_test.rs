use super::*;

#[test]
fn non_counting_policies_do_not_touch_counter_and_counting_never_wraps() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let counter = AtomicU64::new(7);
    assert!(Policy::Always.admit_atomic(&counter));
    assert!(!Policy::Never.admit_atomic(&counter));
    assert_eq!(counter.load(Ordering::Relaxed), 7);
    counter.store(u64::MAX, Ordering::Relaxed);
    assert!(Policy::FirstN(u64::MAX).admit_atomic(&counter));
    assert!(!Policy::EveryN(2).admit_atomic(&counter));
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
}

#[test]
fn admission_sequences() {
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
        assert_eq!(actual, expected, "{policy:?}");
    }
}
