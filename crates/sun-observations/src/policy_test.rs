use super::*;

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
        let mut counter = 0;
        let actual: Vec<_> = (0..6).map(|_| policy.admit(&mut counter)).collect();
        assert_eq!(actual, expected, "{policy:?}");
    }
}
