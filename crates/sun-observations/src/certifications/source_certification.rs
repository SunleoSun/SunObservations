use super::*;

#[test]
fn prepared_reports_preserve_source_names_across_correspondence_and_lag_entrypoints() {
    let rows = [(1u64, 2.), (2, 7.), (3, 4.)];
    let left = collection("producer", &rows);
    let right = collection("consumer", &rows);
    let generic = left.compare_with(&right).on(|v| v.0, |v| v.0);
    let integer = left.compare_with(&right).on_u64(|v| v.0, |v| v.0);
    let presorted = left
        .compare_with(&right)
        .on_presorted(|v| v.0, |v| v.0)
        .unwrap();
    let keys = left.keys(|v| v.0);
    let other_keys = right.keys(|v| v.0);
    let cached = keys.compare_with(&other_keys);
    for paired in [&generic, &integer, &presorted, &cached] {
        assert_eq!(paired.comparison().left_source(), Some("producer"));
        assert_eq!(paired.comparison().right_source(), Some("consumer"));
    }
    let config = LagConfig {
        range: LagRange::new(0, 0).unwrap(),
        measure: LagMeasure::Pearson,
    };
    let report = left
        .lag_with(&right, |v| (v.0 as i64, v.1), |v| (v.0 as i64, v.1), config)
        .observed
        .unwrap();
    assert_eq!(report.left_source.as_deref(), Some("producer"));
    assert_eq!(report.right_source.as_deref(), Some("consumer"));
}
#[test]
fn source_lag_entrypoints_share_self_projection_and_cross_source_semantics() {
    let values: Vec<_> = (0..40)
        .map(|i| (i, ((i * i * 13 + i * 17) % 101) as f64))
        .collect();
    let shifted: Vec<_> = values.iter().map(|(k, v)| (k + 2, *v)).collect();
    let a = collection("signal", &values);
    let b = collection("shifted", &shifted);
    let config = LagConfig {
        range: LagRange::new(-2, 2).unwrap(),
        measure: LagMeasure::Pearson,
    };
    let mut c = certification("source-owned lag");
    c.check(
        "self lag",
        a.lag_self(|v| *v, config)
            .expect(LagExpectation { best_lag: 0 }),
    );
    c.check(
        "same-source projections",
        a.lag_with(&a, |v| *v, |v| (v.0 + 2, v.1), config)
            .expect(LagExpectation { best_lag: 2 }),
    );
    c.check(
        "cross-source",
        a.lag_with(&b, |v| *v, |v| *v, config)
            .expect(LagExpectation { best_lag: 2 }),
    );
    c.check(
        "numeric integrity composition",
        a.numeric(|v| v.1).expect(NumericRule::Finite),
    );
    c.assert_certify();
}
#[test]
fn prepared_numeric_semantic_views_fill_and_reuse_only_compact_analysis_caches() {
    let values: Vec<_> = (0..10000).map(|v| (v, v as f64)).collect();
    let a = collection("input", &values);
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
