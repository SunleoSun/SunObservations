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
    let mut certification = sun_observations::certifications::certification(
        "derive emits metadata without registration and supports generics",
    );
    certification.check_fact_expect(
        "ArbitraryRustName::<u32>::NAMESPACE",
        &(ArbitraryRustName::<u32>::NAMESPACE),
        &("metadata::stable"),
    );
    certification.check_fact_expect(
        "ArbitraryRustName::<u32>::LEVEL",
        &(ArbitraryRustName::<u32>::LEVEL),
        &(ObservationLevel::ERROR),
    );
    certification.check_fact_expect(
        "ArbitraryRustName::<u32>::TAGS",
        &(ArbitraryRustName::<u32>::TAGS),
        &(["detail", "expensive"].as_slice()),
    );
    certification.check_fact_expect(
        "Defaulted::LEVEL",
        &(Defaulted::LEVEL),
        &(ObservationLevel::INFO),
    );
    certification.check_fact_expect(
        "Defaulted::TAGS.is_empty()",
        Defaulted::TAGS.is_empty(),
        true,
    );
    certification.assert_certify();
}

#[test]
fn downstream_compile_pass_and_compile_fail_metadata() {
    let tests = trybuild::TestCases::new();
    tests.pass("src/fixtures/valid_test.rs");
    tests.compile_fail("src/fixtures/invalid_*_test.rs");
}
