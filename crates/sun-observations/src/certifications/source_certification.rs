use super::*;
use crate::certifications::test_support::{KeyScenario, Mismatches};
use std::{cell::Cell, fmt};

struct Provenance;
impl SourceProvenance for Provenance {
    fn describe(&self, index: usize, writer: &mut dyn fmt::Write) -> fmt::Result {
        write!(writer, "row:{index}")
    }
}
fn execute(s: &KeyScenario) -> Result<Vec<String>, ()> {
    let (keys, start, end) = s.materialize();
    let rows: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (*k, (i as f64 - 3.) * 0.25))
        .collect();
    let source = collection("producer", &rows).with_provenance(&Provenance);
    let mut m = Mismatches::default();
    let unique = source.unique(|r| r.0);
    let contiguous = source.contiguous(|r| r.0, start, end);
    m.equal("unique source", &unique.source, &"producer".to_string());
    m.equal(
        "unique owner",
        &unique.observed.unwrap(),
        &check_unique(&rows, |r| r.0),
    );
    m.equal(
        "contiguous source",
        &contiguous.source,
        &"producer".to_string(),
    );
    m.equal(
        "contiguous owner",
        &contiguous.observed.unwrap(),
        &check_contiguous(&rows, |r| r.0, start, end),
    );
    let calls = Cell::new(0);
    let numeric = source.numeric(|r| {
        calls.set(calls.get() + 1);
        r.1
    });
    let direct = NumericSeries::new(rows.iter().map(|r| r.1));
    let ordinary = numeric.summary();
    let robust = numeric.robust_summary();
    m.equal("numeric source", &ordinary.source, &"producer".to_string());
    m.equal("robust source", &robust.source, &"producer".to_string());
    m.equal(
        "summary owner",
        &ordinary.observed.map_err(|e| e.message),
        &direct.summary().map_err(|e| e.to_string()),
    );
    m.equal(
        "robust owner",
        &robust.observed.map_err(|e| e.message),
        &direct.robust_summary().map_err(|e| e.to_string()),
    );
    m.equal("projection reuse", &calls.get(), &rows.len());
    for entry in source.iter() {
        m.equal("entry", &entry.value.0, &keys[entry.index]);
        let mut description = String::new();
        entry
            .provenance
            .unwrap()
            .describe(entry.index, &mut description)
            .unwrap();
        m.equal("provenance", &description, &format!("row:{}", entry.index));
    }
    let other = source.clone();
    let compared = source.compare_with(&other).on(|r| r.0, |r| r.0);
    m.equal(
        "left source",
        &compared.comparison().left_source(),
        &Some("producer"),
    );
    m.equal(
        "right source",
        &compared.comparison().right_source(),
        &Some("producer"),
    );
    for (_, left, right) in compared.matched_pairs() {
        m.equal("matched indices", &left.index, &right.index);
        m.require(
            "comparison provenance",
            left.provenance.is_some() && right.provenance.is_some(),
        );
    }
    let prepared = compared.numeric(|r| r.1, |r| r.1 * 2. + 1.);
    if compared.comparison().is_complete() {
        let pairs: Vec<_> = compared
            .matched_pairs()
            .map(|(_, a, b)| (a.value.1, b.value.1 * 2. + 1.))
            .collect();
        m.equal(
            "relation owner",
            &prepared.relation().observed.map_err(|e| e.message),
            &numeric_relation(pairs.iter().copied()).map_err(|e| e.to_string()),
        );
    } else {
        m.require(
            "incomplete correspondence fails closed",
            prepared.relation().observed.is_err(),
        );
    }
    Ok(m.finish())
}
#[test]
fn generated_prepared_sources_are_faithful_named_provenance_preserving_owner_views() {
    let mut c = certification("source canonical owner parity").executor(execute);
    c.domain(KeyScenario::domain());
    c.seed(463).cases_budget(128);
    let case = c.case();
    c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
    c.require_pairwise_coverage();
    c.assert_certify();
}

mod deterministic {
    use super::*;

    #[test]
    fn prepared_reports_preserve_source_names_across_correspondence_and_lag_entrypoints() {
        let rows = [(1u64, 2.), (2, 7.), (3, 4.)];
        let left = collection("producer", &rows);
        let right = collection("consumer", &rows);
        let generic = left.compare_with(&right).on(|v| v.0, |v| v.0);
        let presorted = left
            .compare_with(&right)
            .on_presorted(|v| v.0, |v| v.0)
            .unwrap();
        let keys = left.keys(|v| v.0);
        let other_keys = right.keys(|v| v.0);
        let cached = keys.compare_with(&other_keys);
        for paired in [&generic, &presorted, &cached] {
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
}
