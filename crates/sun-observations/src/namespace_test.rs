use super::*;

#[test]
fn lowercase_segment_contract() {
    for valid in ["a", "alpha_2", "alpha::beta2::gamma_3"] {
        assert!(validate_namespace(valid).is_ok());
    }
    for invalid in [
        "",
        "Encoder::Train",
        "encoder/Train",
        "encoder::::train",
        "::encoder",
        "encoder::",
        "2a",
        "a-b",
        "a::",
        "é",
    ] {
        assert!(validate_namespace(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn subtree_is_segment_aware() {
    assert!(matches_subtree("compute::train", "compute::train"));
    assert!(matches_subtree("compute::train", "compute::train::epoch"));
    assert!(!matches_subtree("compute::train", "compute::trainer"));
    assert!(!matches_subtree("compute::train", "compute::infer"));
    assert!(!matches_subtree("compute::train::epoch", "compute::train"));
}
