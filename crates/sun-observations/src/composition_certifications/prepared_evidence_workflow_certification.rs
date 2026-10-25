use crate::certifications::*;
use std::cell::Cell;

#[derive(Debug, PartialEq)]
struct Output {
    count: usize,
    score: f64,
}
#[derive(Debug)]
struct Row {
    fold: u8,
    target: u8,
    row: u64,
    signal: f64,
    strength: f64,
}
fn rows(n: usize) -> Vec<Row> {
    (0..n)
        .map(|i| Row {
            fold: (i % 3) as u8,
            target: (i % 2) as u8,
            row: i as u64,
            signal: i as f64,
            strength: (i * i) as f64,
        })
        .collect()
}
#[test]
fn scalar_struct_collection_and_empty_contracts() {
    let output = Output {
        count: 3,
        score: 1.5,
    };
    let mut proof = certification("ordinary canonical results");
    let result = proof.source("result", &output);
    proof
        .check(
            "structure",
            result
                .single_map(|v| Output {
                    count: v.count,
                    score: v.score,
                })
                .expect(Equal(Output {
                    count: 3,
                    score: 1.5,
                })),
        )
        .check(
            "count",
            result.single_map(|v| v.count).expect(CountRule::Exact(3)),
        )
        .check(
            "finite",
            result.single_map(|v| v.score).expect(NumericRule::Finite),
        );
    let values = [1u64, 2, 3];
    let values = proof.collection("loaded artifact", &values);
    proof.section("artifact", |c| {
        c.check("nonempty", values.count().expect(CountRule::NonEmpty))
            .check("unique", values.unique(|v| *v).expect(AllValid))
            .check(
                "contiguous",
                values.contiguous(|v| *v, 1, 3).expect(AllValid),
            )
            .check(
                "forbidden",
                values.matching_count(|v| *v > 3).expect(Equal(0)),
            );
    });
    proof
        .check(
            "empty allowed",
            collection::<u64>("excluded", &[]).count().expect(Equal(0)),
        )
        .assert_certify();
    assert_eq!(proof.report().checks, 8);
    assert!(proof.report().to_string().is_empty());
}
#[test]
fn preparation_reuses_keys_comparison_and_projected_values_across_fields() {
    let left = rows(10000);
    let mut right = rows(10000);
    right.reverse();
    let a = collection("producer", &left);
    let b = collection("consumer", &right);
    let key_calls = Cell::new(0);
    let ak = a.keys(|v| {
        key_calls.set(key_calls.get() + 1);
        (v.fold, v.target, v.row)
    });
    let bk = b.keys(|v| {
        key_calls.set(key_calls.get() + 1);
        (v.fold, v.target, v.row)
    });
    let comparison = ak.compare_with(&bk);
    let address = comparison.comparison().matched.as_ptr();
    let projections = Cell::new(0);
    let numeric = comparison.numeric(
        |v| {
            projections.set(projections.get() + 1);
            v.signal
        },
        |v| {
            projections.set(projections.get() + 1);
            v.signal
        },
    );
    for _ in 0..3 {
        let mut c = certification("producer-consumer composite identity");
        c.check(
            "paired_comparison_fact",
            comparison.paired_comparison_fact().expect(AllValid),
        );
        c.check(
            "signal",
            comparison
                .equal(|v| v.signal, |v| v.signal, 3)
                .expect(AllValid),
        );
        c.check(
            "strength",
            comparison
                .equal(|v| v.strength, |v| v.strength, 3)
                .expect(AllValid),
        );
        c.check(
            "relation",
            numeric
                .relation()
                .expect(RelationExpectation::spearman_at_least(0.99).unwrap()),
        );
        c.check(
            "zero difference",
            numeric.difference().expect(criterion(
                "zero maximum difference",
                ViolationClass::Hard,
                |r: &DifferenceSummary| r.absolute.max == 0.,
            )),
        );
        c.assert_certify();
    }
    assert_eq!(key_calls.get(), 20000);
    assert_eq!(projections.get(), 20000);
    assert_eq!(comparison.comparison().matched.as_ptr(), address);
    assert_eq!(ak.compare_with(&bk).comparison().matched.len(), 10000);
    let (key, l, r) = comparison.matched_pairs().next().unwrap();
    assert_eq!(*key, (l.value.fold, l.value.target, l.value.row));
    assert_eq!(l.value.row, r.value.row);
    assert!(std::ptr::eq(l.value, &left[l.index]));
}
#[test]
fn dedicated_domain_key_and_unordered_sources_preserve_semantic_correspondence() {
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Key(u8, u8, u64);
    let a = rows(20);
    let mut b = rows(20);
    b.rotate_left(7);
    let a = collection("a", &a);
    let b = collection("b", &b);
    let pair = a.compare_with(&b).on(
        |v| Key(v.fold, v.target, v.row),
        |v| Key(v.fold, v.target, v.row),
    );
    certification("typed key")
        .check(
            "paired_comparison_fact",
            pair.paired_comparison_fact().expect(AllValid),
        )
        .check(
            "values",
            pair.check_pairs(|l, r| l.signal == r.signal, 3)
                .expect(AllValid),
        )
        .assert_certify();
}
#[test]
fn missing_and_duplicate_evidence_fails_hard_and_blocks_only_dependent_facts() {
    let a = [(1, 1.), (2, 2.), (2, 3.), (4, 4.)];
    let b = [(1, 1.), (3, 3.)];
    let a = collection("left", &a);
    let b = collection("right", &b);
    let pair = a.compare_with(&b).on(|v| v.0, |v| v.0);
    let mut c = certification("incomplete correspondence");
    c.check(
        "paired_comparison_fact",
        pair.paired_comparison_fact().expect(AllValid),
    );
    c.check(
        "dependent",
        pair.numeric(|v| v.1, |v| v.1)
            .relation()
            .expect(RelationExpectation::spearman_at_least(0.9).unwrap()),
    );
    c.check("healthy", fact("independent", 2).expect(Equal(2)));
    let r = c.report();
    assert_eq!(r.failed, 2);
    assert_eq!(r.checks, 3);
    assert_eq!(r.failures[0].class, CheckClass::HardViolation);
    assert_eq!(r.failures[1].class, CheckClass::EvaluationError);
    let s = r.to_string();
    assert!(s.contains("left_only=2"));
    assert!(s.contains("duplicate left=1"));
    assert!(s.contains("left -> right"));
    assert_eq!(pair.comparison().matched.len(), 1);
}
#[test]
fn expensive_source_and_group_projections_are_cached() {
    let values = rows(100);
    let source = collection("values", &values);
    let calls = Cell::new(0);
    let numeric = source.numeric(|v| {
        calls.set(calls.get() + 1);
        v.signal
    });
    numeric.summary().observed.unwrap();
    numeric.robust_summary().observed.unwrap();
    numeric.quantile(0.5).observed.unwrap();
    assert_eq!(calls.get(), 100);
    let groups = source.group_by(|v| v.fold);
    let numeric = groups.numeric(|v| {
        calls.set(calls.get() + 1);
        v.signal
    });
    let summary = numeric.summaries();
    let robust = numeric.summaries_with_robust();
    assert_eq!(calls.get(), 200);
    assert_eq!(
        summary.groups.iter().map(|v| v.member_count).sum::<usize>(),
        100
    );
    certification("per-fold behavior")
        .check(
            "group mean",
            fact("folds", robust).expect(GroupExpectation::new(30, 40., 60.).unwrap()),
        )
        .assert_certify();
}
#[test]
fn hard_statistical_undefined_and_analysis_states_are_distinct_and_collect_all() {
    let mut c = certification("all independent failures");
    c.section("inputs", |c| {
        c.check("count", fact("input", 0usize).expect(CountRule::NonEmpty));
    });
    let a = [(1, 1.), (2, 2.), (3, 3.)];
    let b = [(1, 3.), (2, 2.), (3, 1.)];
    let a = collection("producer", &a);
    let b = collection("consumer", &b);
    let pair = a.compare_with(&b).on(|v| v.0, |v| v.0);
    c.section("encoder", |c| {
        c.section("prediction", |c| {
            c.check(
                "direction",
                pair.numeric(|v| v.1, |v| v.1)
                    .relation()
                    .expect(RelationExpectation::spearman_at_least(0.9).unwrap()),
            );
        });
    });
    c.check(
        "numeric input",
        try_fact("bad", NumericSeries::new([f64::NAN].into_iter()).summary()).expect(criterion(
            "mean > 0",
            ViolationClass::Statistical,
            |v: &Summary| v.mean > 0.,
        )),
    );
    c.check("output", fact("output", 2).expect(Equal(3)));
    let r = c.report();
    assert_eq!(r.failed, 4);
    assert_eq!(r.checks, 4);
    let s = r.to_string();
    for marker in [
        "all independent failures",
        "inputs/",
        "encoder/prediction/",
        "[HardViolation]",
        "[StatisticalViolation]",
        "[EvaluationError]",
        "direction",
        "numeric input",
        "output",
        "producer -> consumer",
    ] {
        assert!(s.contains(marker), "{s}");
    }
    let undefined = try_fact(
        "constant",
        numeric_relation([(1., 2.), (1., 2.)].into_iter()),
    )
    .expect(RelationExpectation::spearman_at_least(0.9).unwrap());
    assert_eq!(undefined.class, CheckClass::EvaluationError);
    assert!(undefined.observed.is_ok());
    assert!(undefined.analysis_error.is_some());
}
#[test]
fn direct_results_preserve_observation_and_typed_violations_including_invalid_criteria() {
    assert!(Criterion::between(5, 3).is_err());
    let r = fact("ordinary", 5).expect(Equal(6));
    assert_eq!(r.class, CheckClass::HardViolation);
    assert_eq!(r.observed.unwrap(), 5);
    assert_eq!(r.violations.len(), 1);
    assert!(fact("absence", 0).expect(Equal(0)).is_pass());
    assert!(fact("positive", 1.).expect(NumericRule::Positive).is_pass());
    let incomparable_observed = fact("nan", f64::NAN).expect(Criterion::greater_than(0.0).unwrap());
    assert_eq!(incomparable_observed.class, CheckClass::EvaluationError);
    assert!(incomparable_observed.analysis_error.is_some());
    assert!(Criterion::at_most(f64::NAN).is_err());
}
#[test]
fn anomaly_and_lag_expectations_fail_as_required_not_as_warnings() {
    let a = [0., 0., 0., 0., 10.];
    let source = collection("signal", &a);
    let n = source.numeric(|v| *v);
    let out = n
        .outliers(OutlierConfig::default())
        .expect(AtMostAnomalies(0));
    assert_eq!(out.class, CheckClass::StatisticalViolation);
    assert_eq!(out.observed.unwrap().suspicious[0].index, 4);
    let jump = n
        .jumps(JumpConfig::new(Some(5.), None, 1.).unwrap())
        .expect(AtMostAnomalies(0));
    assert_eq!(jump.class, CheckClass::StatisticalViolation);
    let a: Vec<_> = (0..30)
        .map(|i| (i, ((i * i * 13 + i * 17) % 101) as f64))
        .collect();
    for shift in [-2, 0, 2] {
        let b: Vec<_> = a.iter().map(|(i, v)| (i + shift, *v)).collect();
        let a = collection("left", &a);
        let b = collection("right", &b);
        let m = a.lag_with(
            &b,
            |v| *v,
            |v| *v,
            LagConfig {
                range: LagRange::new(-3, 3).unwrap(),
                measure: LagMeasure::Pearson,
            },
        );
        let r = m.expect(LagExpectation { best_lag: shift });
        assert!(r.is_pass());
        assert_eq!(r.observed.unwrap().best.unwrap().lag, shift);
    }
}
#[test]
fn reference_oracle_and_affine_metamorphic_evidence_are_independent() {
    let raw = [1., 4., 2., 8., 5.];
    let transformed: Vec<_> = raw.iter().map(|x| 3. * x + 7.).collect();
    let a: Vec<_> = raw.into_iter().enumerate().collect();
    let b: Vec<_> = transformed.into_iter().enumerate().collect();
    let a = collection("input", &a);
    let b = collection("scaled result", &b);
    let p = a.compare_with(&b).on(|v| v.0, |v| v.0);
    let r = p.numeric(|v| v.1, |v| v.1).relation().observed.unwrap();
    let mx = 4.;
    let my = 19.;
    let xy = raw
        .iter()
        .map(|x| (x - mx) * (3. * x + 7. - my))
        .sum::<f64>();
    let xx = raw.iter().map(|x| (x - mx).powi(2)).sum::<f64>();
    assert!((r.linear.slope.value().unwrap() - xy / xx).abs() < 1e-12);
    assert!((r.linear.intercept.value().unwrap() - 7.).abs() < 1e-12);
    assert!(r.residual.unwrap().absolute.max < 1e-12);
}
#[test]
fn million_record_failures_and_success_aggregation_keep_bounded_evidence() {
    let values: Vec<_> = (0..1_000_000u64).collect();
    let a = collection("million rows", &values);
    let mut c = certification("bounded million-row proof");
    c.limits(DiagnosticLimits {
        failures: 4,
        characters_per_failure: 500,
    });
    for _ in 0..100 {
        c.check("success count", a.count().expect(Equal(values.len())));
    }
    assert!(c.report().failures.is_empty());
    for _ in 0..10 {
        c.check(
            "failure",
            fact("large fact report", &values).expect(criterion(
                "forbidden evidence absent",
                ViolationClass::Hard,
                |_: &&Vec<u64>| false,
            )),
        );
    }
    let r = c.report();
    assert_eq!(r.checks, 110);
    assert_eq!(r.failed, 10);
    assert_eq!(r.failures.len(), 4);
    assert!(r.to_string().len() < 4000);
    assert!(r.to_string().contains("6 additional failures omitted"));
}
#[test]
fn aggregation_drops_success_reports_without_rendering_or_cloning_trace() {
    use std::{fmt, rc::Rc};
    struct Trace {
        rows: Vec<u64>,
        dropped: Rc<Cell<usize>>,
        rendered: Rc<Cell<usize>>,
    }
    impl fmt::Debug for Trace {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for row in &self.rows {
                self.rendered.set(self.rendered.get() + 1);
                write!(f, "{row},")?;
            }
            Ok(())
        }
    }
    impl Drop for Trace {
        fn drop(&mut self) {
            self.dropped.set(self.dropped.get() + 1);
        }
    }
    let dropped = Rc::new(Cell::new(0));
    let rendered = Rc::new(Cell::new(0));
    let trace = || Trace {
        rows: (0..1_000_000).collect(),
        dropped: dropped.clone(),
        rendered: rendered.clone(),
    };
    let mut c = certification("trace ownership");
    c.check(
        "success",
        fact("trace", trace()).expect(criterion("present", ViolationClass::Hard, |t: &Trace| {
            t.rows.len() == 1_000_000
        })),
    );
    assert_eq!(dropped.get(), 1);
    assert_eq!(rendered.get(), 0);
    assert!(c.report().failures.is_empty());
    c.limits(DiagnosticLimits {
        failures: 1,
        characters_per_failure: 256,
    });
    c.check(
        "failure",
        fact("trace", trace()).expect(criterion("absent", ViolationClass::Hard, |_: &Trace| false)),
    );
    assert_eq!(dropped.get(), 2);
    assert!(rendered.get() < 100);
    assert!(c.report().to_string().len() < 1000);
}

#[test]
fn empty_proof_and_empty_sections_fail_without_report_assertion_side_effects() {
    let mut proof = certification("empty");
    assert!(!proof.report().is_pass());
    assert!(std::panic::catch_unwind(|| proof.assert_certify()).is_err());
    proof.section("empty section", |_| {});
    assert!(!proof.report().is_pass());
    proof.check("meaningful", fact("value", 1).expect(1));
    assert!(proof.report().is_pass());
    proof.assert_certify();
}

#[test]
fn bounded_pair_diagnostics_preserve_counts_before_truncating_examples() {
    let left = rows(1000);
    let mut right = rows(1000);
    for row in &mut right {
        row.signal += 1.;
    }
    let a = collection("producer", &left);
    let b = collection("consumer", &right);
    let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
    let mut c = certification("bounded transfer evidence");
    c.limits(DiagnosticLimits {
        failures: 1,
        characters_per_failure: 512,
    });
    c.check(
        "exact transfer",
        pair.equal(|v| v.signal, |v| v.signal, 32).expect(AllValid),
    );
    let report = c.report().to_string();
    assert!(report.contains("matched=1000 mismatches=1000"), "{report}");
    assert!(report.contains("left index="));
    assert!(report.len() < 1000);
}

#[test]
fn final_assertion_panics_at_invocation_not_framework() {
    use std::sync::{Arc, Mutex};
    let captured = Arc::new(Mutex::new(None));
    let hook = captured.clone();
    let previous = std::panic::take_hook();
    let invocation_thread = std::thread::current().id();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() != invocation_thread {
            return;
        }
        if let Some(location) = info.location() {
            *hook.lock().unwrap() = Some((location.file().to_owned(), location.line()));
        }
    }));
    let c = {
        let mut c = certification("caller location");
        c.check("wrong", fact("x", 1).expect(Equal(2)));
        c
    };
    let expected_line = line!() + 1;
    let result = std::panic::catch_unwind(|| c.assert_certify());
    std::panic::set_hook(previous);
    assert!(result.is_err());
    let captured = captured.lock().unwrap();
    let (file, line) = captured.as_ref().unwrap();
    assert!(file.ends_with("prepared_evidence_workflow_certification.rs"));
    assert_eq!(*line, expected_line);
}
