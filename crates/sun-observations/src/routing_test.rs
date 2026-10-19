use super::*;

#[test]
fn routes_match_subtrees_and_fan_out_without_duplicate_channels() {
    let mut routes = Routing::default();
    routes.register("compute", "all").unwrap();
    routes.register("compute::train", "training").unwrap();
    routes.register("compute::train", "all").unwrap();
    routes.register("compute::train", "all").unwrap();
    routes.register("compute::infer", "inference").unwrap();
    let names: Vec<_> = routes
        .matching("compute::train::epoch")
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    assert_eq!(names, ["all", "training"]);
    assert_eq!(routes.matching("compute::trainer").len(), 1);
    assert!(routes.matching("other").is_empty());
    assert!(routes.register("Bad", "bad").is_err());
}
