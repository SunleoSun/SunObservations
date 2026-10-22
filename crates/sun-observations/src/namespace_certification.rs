use crate::{MemorySink, Observation, ObservationEngine, Selection};
use serde::Serialize;
use std::sync::Arc;
#[derive(Serialize, Observation)]
#[observation(namespace = "compute::train")]
struct Renamed;
#[derive(Serialize, Observation)]
#[observation(namespace = "compute::train::epoch")]
struct Child;
#[derive(Serialize, Observation)]
#[observation(namespace = "compute::trainer")]
struct Sibling;
#[test]
fn explicit_identity_selection_and_routes_share_segment_boundaries() {
    let mut certification = sun_observations::certifications::certify(
        "explicit identity selection and routes share segment boundaries",
    );
    let engine = ObservationEngine::new();
    let sink = Arc::new(MemorySink::new());
    engine.add_channel_sink("train", sink.clone());
    engine
        .register_namespace_channel("compute::train", "train")
        .unwrap();
    let generation = engine.generation();
    let invalid = Arc::new(MemorySink::new());
    engine.add_channel_sink("invalid", invalid.clone());
    let attached_generation = engine.generation();
    certification.check_fact_expect(
        "attached_generation > generation",
        attached_generation > generation,
        true,
    );
    certification.check_fact_expect(
        "engine .register_namespace_channel(\"compute::train::\", \"invalid\") .is_err()",
        engine
            .register_namespace_channel("compute::train::", "invalid")
            .is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine.generation()",
        &(engine.generation()),
        &(attached_generation),
    );
    engine.set_selection(Selection::new().activate_namespace("compute::train"));
    certification.check_fact_expect(
        "engine.observe_if_active(|| Renamed).unwrap()",
        engine.observe_if_active(|| Renamed).unwrap(),
        true,
    );
    certification.check_fact_expect(
        "engine.observe_if_active(|| Child).unwrap()",
        engine.observe_if_active(|| Child).unwrap(),
        true,
    );
    certification.check_fact_expect(
        "engine .observe_if_active::<Sibling, _>(|| panic!(\"lazy\")) .unwrap()",
        engine
            .observe_if_active::<Sibling, _>(|| panic!("lazy"))
            .unwrap(),
        false,
    );
    engine.set_selection(Selection::new().activate_all());
    certification.check_fact_expect(
        "engine.observe_if_active(|| Sibling).unwrap()",
        engine.observe_if_active(|| Sibling).unwrap(),
        true,
    );
    let records = sink.records();
    certification.check_fact_expect("records.len()", &(records.len()), &(2));
    certification.check_fact_expect("invalid.is_empty()", invalid.is_empty(), true);
    certification.check_fact_expect(
        "records[0].namespace",
        records[0].namespace.as_str(),
        "compute::train",
    );
    certification.check_fact_expect(
        "records[1].namespace",
        records[1].namespace.as_str(),
        "compute::train::epoch",
    );
    certification.assert_certify();
}

mod grammar {
    use super::super::*;

    #[test]
    fn lowercase_segment_contract() {
        let mut certification =
            sun_observations::certifications::certify("lowercase segment contract");
        for valid in ["a", "alpha_2", "alpha::beta2::gamma_3"] {
            certification.check_fact_expect(
                "validate_namespace(valid).is_ok()",
                validate_namespace(valid).is_ok(),
                true,
            );
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
            certification.check_fact_expect(
                format!("{invalid:?}"),
                validate_namespace(invalid).is_err(),
                true,
            );
        }
        certification.assert_certify();
    }

    #[test]
    fn subtree_is_segment_aware() {
        let mut certification =
            sun_observations::certifications::certify("subtree is segment aware");
        certification.check_fact_expect(
            "matches_subtree(\"compute::train\", \"compute::train\")",
            matches_subtree("compute::train", "compute::train"),
            true,
        );
        certification.check_fact_expect(
            "matches_subtree(\"compute::train\", \"compute::train::epoch\")",
            matches_subtree("compute::train", "compute::train::epoch"),
            true,
        );
        certification.check_fact_expect(
            "matches_subtree(\"compute::train\", \"compute::trainer\")",
            matches_subtree("compute::train", "compute::trainer"),
            false,
        );
        certification.check_fact_expect(
            "matches_subtree(\"compute::train\", \"compute::infer\")",
            matches_subtree("compute::train", "compute::infer"),
            false,
        );
        certification.check_fact_expect(
            "matches_subtree(\"compute::train::epoch\", \"compute::train\")",
            matches_subtree("compute::train::epoch", "compute::train"),
            false,
        );
        certification.assert_certify();
    }
}
