use super::*;

#[test]
fn malformed_metadata_returns_diagnostics_before_codegen() {
    for (source, diagnostic) in [
        ("struct Missing;", "missing required observation namespace"),
        (
            "#[observation(namespace = \"Upper\")] struct Bad;",
            "namespace must contain",
        ),
        (
            "#[observation(namespace = \"a\", level = DEBUG)] struct Bad;",
            "level must be INFO",
        ),
        (
            "#[observation(namespace = \"a\", tags(\"Bad\"))] struct Bad;",
            "tags must match",
        ),
        (
            "#[observation(namespace = \"a\", namespace = \"b\")] struct Bad;",
            "duplicate namespace",
        ),
        (
            "#[observation(namespace = \"a\", unknown)] struct Bad;",
            "unknown observation attribute",
        ),
    ] {
        let input = syn::parse_str(source).unwrap();
        assert!(expand(input).unwrap_err().to_string().contains(diagnostic));
    }
}
