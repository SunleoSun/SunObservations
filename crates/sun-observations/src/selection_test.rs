use super::*;

#[test]
fn positives_are_namespace_or_any_tag() {
    let selection = Selection::new()
        .activate_namespace("compute::train")
        .activate_tags(["detail", "costly"]);
    assert!(selection.admits("compute::train::epoch", &[]));
    assert!(selection.admits("other", &["detail"]));
    assert!(selection.admits("other", &["unselected", "costly"]));
    assert!(!selection.admits("compute::trainer", &["unselected"]));
    assert!(!Selection::new().admits("compute", &["detail"]));
}

#[test]
fn any_exclusion_vetoes_all_activations() {
    let selection = Selection::new()
        .activate_all()
        .activate_tags(["detail"])
        .exclude_namespace("compute::train")
        .exclude_tags(["costly"]);
    assert!(!selection.admits("compute::train::epoch", &["detail"]));
    assert!(!selection.admits("other", &["detail", "costly"]));
    assert!(selection.admits("compute::trainer", &["detail"]));
}

#[test]
#[should_panic(expected = "invalid activation namespace")]
fn runtime_selectors_reject_invalid_namespaces() {
    let _ = Selection::new().activate_namespace("Upper");
}

#[test]
#[should_panic(expected = "invalid tag")]
fn runtime_selectors_reject_invalid_tags() {
    let _ = Selection::new().activate_tags(["Bad Tag"]);
}
