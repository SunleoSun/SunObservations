use crate::{Observation, ObservationLevel};
use serde::Serialize;

#[derive(Serialize, Observation)]
#[observation(namespace = "metadata::stable", level = ERROR, tags("detail", "expensive"))]
struct ArbitraryRustName<T> {
    value: T,
}

#[derive(Serialize, Observation)]
#[observation(namespace = "metadata::default", tags())]
struct Defaulted;

#[test]
fn derive_emits_metadata_without_registration_and_supports_generics() {
    assert_eq!(ArbitraryRustName::<u32>::NAMESPACE, "metadata::stable");
    assert_eq!(ArbitraryRustName::<u32>::LEVEL, ObservationLevel::ERROR);
    assert_eq!(ArbitraryRustName::<u32>::TAGS, ["detail", "expensive"]);
    assert_eq!(Defaulted::LEVEL, ObservationLevel::INFO);
    assert!(Defaulted::TAGS.is_empty());
}

#[test]
fn downstream_compile_pass_and_compile_fail_metadata() {
    let tests = trybuild::TestCases::new();
    tests.pass("src/fixtures/valid_test.rs");
    tests.compile_fail("src/fixtures/invalid_*_test.rs");
}
