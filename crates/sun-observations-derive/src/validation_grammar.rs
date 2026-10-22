use super::*;

#[test]
fn namespace_grammar_is_strict_ascii_lowercase() {
    for value in ["a", "alpha_1::beta2"] {
        assert!(namespace(value));
    }
    for value in ["", "Alpha", "a/b", "a::::b", "::a", "a::", "1a", "a-b", "é"] {
        assert!(!namespace(value), "{value:?}");
    }
    assert!(segment("detail_1"));
    assert!(!segment("a::b"));
}
