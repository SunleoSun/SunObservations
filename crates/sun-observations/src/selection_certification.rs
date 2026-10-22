use super::*;

#[test]
fn positives_are_namespace_or_any_tag() {
    let mut certification =
        sun_observations::certifications::certification("positives are namespace or any tag");
    let selection = Selection::new()
        .activate_namespace("compute::train")
        .unwrap()
        .activate_tags(["detail", "costly"])
        .unwrap();
    certification.check_fact_expect(
        "selection.admits(\"compute::train::epoch\", &[])",
        selection.admits("compute::train::epoch", &[]),
        true,
    );
    certification.check_fact_expect(
        "selection.admits(\"other\", &[\"detail\"])",
        selection.admits("other", &["detail"]),
        true,
    );
    certification.check_fact_expect(
        "selection.admits(\"other\", &[\"unselected\", \"costly\"])",
        selection.admits("other", &["unselected", "costly"]),
        true,
    );
    certification.check_fact_expect(
        "selection.admits(\"compute::trainer\", &[\"unselected\"])",
        selection.admits("compute::trainer", &["unselected"]),
        false,
    );
    certification.check_fact_expect(
        "Selection::new().admits(\"compute\", &[\"detail\"])",
        Selection::new().admits("compute", &["detail"]),
        false,
    );
    certification.assert_certify();
}

#[test]
fn any_exclusion_vetoes_all_activations() {
    let mut certification =
        sun_observations::certifications::certification("any exclusion vetoes all activations");
    let selection = Selection::new()
        .activate_all()
        .activate_tags(["detail"])
        .unwrap()
        .exclude_namespace("compute::train")
        .unwrap()
        .exclude_tags(["costly"])
        .unwrap();
    certification.check_fact_expect(
        "selection.admits(\"compute::train::epoch\", &[\"detail\"])",
        selection.admits("compute::train::epoch", &["detail"]),
        false,
    );
    certification.check_fact_expect(
        "selection.admits(\"other\", &[\"detail\", \"costly\"])",
        selection.admits("other", &["detail", "costly"]),
        false,
    );
    certification.check_fact_expect(
        "selection.admits(\"compute::trainer\", &[\"detail\"])",
        selection.admits("compute::trainer", &["detail"]),
        true,
    );
    certification.assert_certify();
}

#[test]
fn runtime_selectors_reject_invalid_namespaces() {
    assert_eq!(
        Selection::new()
            .activate_namespace(String::from("Upper"))
            .unwrap_err(),
        crate::namespace::InvalidNamespace("Upper".into())
    );
}

#[test]
fn runtime_selectors_reject_invalid_tags() {
    assert_eq!(
        Selection::new()
            .activate_tags([String::from("Bad Tag")])
            .unwrap_err(),
        crate::namespace::InvalidTag("Bad Tag".into())
    );
}
