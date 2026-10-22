use super::*;

#[test]
fn routes_match_subtrees_and_fan_out_without_duplicate_channels() {
    let mut certification =
        sun_certifications::certify("routes match subtrees and fan out without duplicate channels");
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
    certification.check_fact_expect(
        "names",
        names.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["all", "training"],
    );
    certification.check_fact_expect(
        "routes.matching(\"compute::trainer\").len()",
        &(routes.matching("compute::trainer").len()),
        &(1),
    );
    certification.check_fact_expect(
        "routes.matching(\"other\").is_empty()",
        routes.matching("other").is_empty(),
        true,
    );
    certification.check_fact_expect(
        "routes.register(\"Bad\", \"bad\").is_err()",
        routes.register("Bad", "bad").is_err(),
        true,
    );
    certification.assert_certify();
}
