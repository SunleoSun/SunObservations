use super::*;
#[test]
fn prepared_numeric_semantic_views_fill_and_reuse_only_compact_analysis_caches() {
    let values: Vec<_> = (0..10000).map(|v| (v, v as f64)).collect();
    let a = source_slice("input", &values);
    let pair = a.compare_with(&a).on(|v| v.0, |v| v.0);
    let numeric = pair.numeric(|v| v.1, |v| v.1);
    assert!(numeric.linear_cache.get().is_none());
    assert!(numeric.rank_cache.get().is_none());
    assert!(numeric.difference_cache.get().is_none());
    assert!(numeric.fit_cache.get().is_none());
    assert!(numeric.correlation().observed.is_ok());
    let moments = numeric.linear_cache.get().unwrap() as *const _;
    let ranks = numeric.rank_cache.get().unwrap() as *const _;
    assert!(numeric.difference_cache.get().is_none());
    assert!(numeric.fit_cache.get().is_none());
    assert!(numeric.linear_fit().observed.is_ok());
    assert!(numeric.difference_cache.get().is_none());
    assert!(numeric.difference().observed.is_ok());
    let difference = numeric.difference_cache.get().unwrap() as *const _;
    let fit = numeric.fit_cache.get().unwrap() as *const _;
    for _ in 0..4 {
        assert!(numeric.correlation().observed.is_ok());
        assert!(numeric.linear_fit().observed.is_ok());
        assert!(numeric.difference().observed.is_ok());
        assert!(numeric.relation().observed.is_ok());
        assert_eq!(moments, numeric.linear_cache.get().unwrap() as *const _);
        assert_eq!(ranks, numeric.rank_cache.get().unwrap() as *const _);
        assert_eq!(
            difference,
            numeric.difference_cache.get().unwrap() as *const _
        );
        assert_eq!(fit, numeric.fit_cache.get().unwrap() as *const _);
    }
}
