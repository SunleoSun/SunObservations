use super::*;
use crate::certifications::*;
use crate::evidence::*;
use crate::{observe_span, try_observe, MemorySinkConfig};
use std::{cell::Cell, collections::BTreeSet, sync::Arc};

#[derive(Debug, serde::Serialize, serde::Deserialize, Observation)]
#[observation(namespace = "aggregate::work")]
struct Work {
    size: usize,
}

fn production(size: usize) -> Result<usize, ()> {
    let mut operations = 0;
    for _ in 0..2 {
        let span = observe_span!(Work { size }).map_err(|_| ())?;
        for item in 0..size {
            std::hint::black_box(item);
            operations += 1;
        }
        span.finish().map_err(|_| ())?;
    }
    try_observe!(Work { size }).map_err(|_| ())?;
    Ok(operations)
}

#[test]
fn captured_rows_keep_repeated_case_identity_input_values_and_canonical_provenance() {
    let checks = Cell::new(0);
    let mut runner = certification("associated captured rows").executor(|size: &usize| {
        capture_observations(CaptureConfig::new().activate_all(), || production(*size))
    });
    runner.data_source([2, 2, 4]);
    let case = runner.case();
    let cases = runner.case_aggregate();
    runner.check(fact!(*case.result()).expect(0));
    runner.check_aggregate(
        fact!({
            checks.set(checks.get() + 1);
            let spans = cases.spans::<Work>().unwrap();
            let source = spans.source("all work");
            assert_eq!(source.values().len(), 6);
            assert_eq!(
                source
                    .values()
                    .iter()
                    .map(|r| r.case().id())
                    .collect::<BTreeSet<_>>()
                    .len(),
                3
            );
            for row in source.values() {
                assert_eq!(row.measurement().value().size, *row.case().input());
                assert_eq!(row.case().result(), &(*row.case().input() * 2));
                let original = row.case().execution().dataset().records();
                assert!(Arc::ptr_eq(
                    &row.sources().start,
                    &original[row.sources().start_index]
                ));
                assert!(Arc::ptr_eq(
                    &row.sources().end,
                    &original[row.sources().end_index]
                ));
                assert_eq!(
                    row.measurement().duration_ns(),
                    row.sources().end.proc_ns - row.sources().start.proc_ns
                );
            }
            let entry = source.entry(2);
            let mut provenance = String::new();
            entry
                .provenance
                .unwrap()
                .describe(entry.index, &mut provenance)
                .unwrap();
            assert!(provenance.contains("case=CaseId(1)"));
            assert!(provenance.contains("start_index=0 end_index=1"));
            let observations = cases.observations::<Work>().unwrap();
            assert_eq!(observations.values().len(), 9);
            for row in observations.values() {
                assert_eq!(row.value().size, *row.case().input());
                let original = row.case().execution().dataset().records();
                let decoded = row.observation();
                assert!(Arc::ptr_eq(
                    decoded.source,
                    &original[decoded.dataset_index]
                ));
            }
            let paired = source.compare_with(&source).on(
                |r| (r.case().id(), r.sources().start_index),
                |r| (r.case().id(), r.sources().start_index),
            );
            assert!(paired.paired_comparison_fact().expect_valid().is_pass());
            assert!(paired
                .numeric(
                    |r| r.measurement().duration_ns() as f64,
                    |r| r.measurement().duration_ns() as f64
                )
                .difference()
                .expect(DifferenceExpectation::max_absolute_at_most(0.0).unwrap())
                .is_pass());
            source
                .numeric(|r| r.measurement().duration_ns() as f64)
                .quantile(0.5)
                .expect(Criterion::at_least(0.0).unwrap())
                .is_pass()
        })
        .expect(true),
    );
    let report = runner.report();
    assert!(!report.is_pass());
    assert_eq!(checks.get(), 1);
    assert!(report.aggregate_failures.is_empty(), "{report}");
}

#[derive(Clone, DomainInput)]
struct Request {
    size: usize,
}

fn family_report(broken: bool) -> PlannedCertificationReport {
    let mut runner = certification("resource scaling across executions").executor(|r: &Request| {
        capture_observations(CaptureConfig::new().activate_all(), || {
            if !(1..=4).contains(&r.size) {
                return Err(());
            }
            let operations = production(r.size)?;
            Ok(if broken && r.size == 4 {
                operations * 4
            } else {
                operations
            })
        })
    });
    runner.data_source([
        Request { size: 1 },
        Request { size: 1 },
        Request { size: 2 },
        Request { size: 3 },
        Request { size: 4 },
    ]);
    runner.domain(domain!(Request { size.in_range(1..=4) }));
    runner.cases_budget(24);
    let cases = runner.case_aggregate();
    runner.check_aggregate(
        fact!({
            let rows = collection("executions", cases.cases().unwrap());
            let paired = rows.compare_with(&rows).on(|r| r.id(), |r| r.id());
            paired
                .numeric(|r| r.input().size as f64, |r| *r.result() as f64)
                .relation()
                .observed
                .unwrap()
        })
        .expect(
            RelationExpectation::with_bound(RelationMeasure::Slope, Some(1.99), Some(2.01))
                .unwrap(),
        ),
    );
    runner.check_aggregate(
        fact!({
            let spans = cases.spans::<Work>().unwrap();
            spans
                .source("elapsed by parity")
                .group_by(|r| r.case().input().size % 2)
                .iter()
                .count()
        })
        .expect(2usize),
    );
    runner.check_aggregate(
        fact!({
            let spans = cases.spans::<Work>().unwrap();
            spans
                .source("elapsed by parity")
                .group_by(|r| r.case().input().size % 2)
                .numeric(|r| r.measurement().duration_ns() as f64)
                .summaries()
        })
        .expect(GroupExpectation::new(2, 0.0, f64::MAX).unwrap()),
    );
    runner.check_aggregate(
        fact!({
            let rows = collection("work per item", cases.cases().unwrap());
            rows.numeric(|r| *r.result() as f64 / r.input().size as f64)
                .outliers(OutlierConfig::default())
                .observed
                .unwrap()
        })
        .expect(AtMostAnomalies(0)),
    );
    runner.check_aggregate(
        fact!({
            let spans = cases.spans::<Work>().unwrap();
            spans
                .source("cross-case elapsed")
                .numeric(|r| r.measurement().duration_ns() as f64)
                .quantile(0.95)
                .observed
                .unwrap()
        })
        .expect(Criterion::at_least(0.0).unwrap()),
    );
    runner.report()
}

#[test]
fn domain_resource_relation_grouped_spans_and_anomalies_share_prepared_analysis_with_negative_control(
) {
    let report = family_report(false);
    assert!(report.is_pass(), "{report}");
    assert!(report.executed[1] > 0);
    let broken = family_report(true);
    assert!(!broken.is_pass());
    assert!(broken.failures.is_empty());
    assert!(broken
        .aggregate_failures
        .iter()
        .any(|f| f.class == CheckClass::StatisticalViolation));
}

#[derive(Clone, Copy)]
enum Corruption {
    MissingEnd,
    MissingStart,
    DuplicateStart,
    DuplicateEnd,
    Decode,
    Incomplete,
}

#[test]
fn one_invalid_positive_case_blocks_the_whole_span_or_observation_sample() {
    for corruption in [
        Corruption::MissingEnd,
        Corruption::MissingStart,
        Corruption::DuplicateStart,
        Corruption::DuplicateEnd,
        Corruption::Decode,
        Corruption::Incomplete,
    ] {
        let mut runner = certification("fail closed aggregate").executor(|size: &usize| {
            let mut run =
                capture_observations(CaptureConfig::new().activate_all(), || production(*size));
            if *size == 2 {
                let mut records = run.dataset.records().to_vec();
                match corruption {
                    Corruption::MissingEnd => {
                        records.remove(1);
                    }
                    Corruption::MissingStart => {
                        records.remove(0);
                    }
                    Corruption::DuplicateStart => records.push(records[0].clone()),
                    Corruption::DuplicateEnd => records.push(records[1].clone()),
                    Corruption::Decode => {
                        let mut start = (*records[0]).clone();
                        start.data = serde_json::json!({"size": "bad"});
                        records[0] = Arc::new(start);
                    }
                    Corruption::Incomplete => run.status.unfinished_contexts = true,
                }
                run.dataset = ObservationDataset::try_from_records(records).unwrap();
            }
            run
        });
        runner.data_source([1, 2, 3]);
        let cases = runner.case_aggregate();
        runner.check_aggregate(
            fact!({
                let error = cases.spans::<Work>().err().unwrap();
                match (&corruption, &error) {
                    (Corruption::Incomplete, AggregateError::Execution(_)) => (),
                    (
                        Corruption::Decode,
                        AggregateError::Evidence {
                            case: CaseId(1),
                            error: SpanError::Decode(_),
                        },
                    ) => (),
                    (
                        _,
                        AggregateError::Evidence {
                            case: CaseId(1),
                            error: SpanError::Lifecycle { .. },
                        },
                    ) => (),
                    _ => panic!("wrong error: {error:?}"),
                }
                cases.spans::<Work>().unwrap().values().len()
            })
            .expect(4usize),
        );
        if matches!(corruption, Corruption::Decode | Corruption::Incomplete) {
            runner.check_aggregate(
                fact!(cases.observations::<Work>().unwrap().values().len()).expect(6usize),
            );
        }
        let report = runner.report();
        assert!(!report.is_pass());
        assert!(!report.aggregate_failures.is_empty());
        assert!(report
            .aggregate_failures
            .iter()
            .all(|f| f.class == CheckClass::EvaluationError));
    }
}

#[test]
fn incomplete_retention_cannot_be_hidden_by_healthy_remaining_pairs() {
    let mut runner = certification("retention completeness").executor(|n: &usize| {
        capture_observations(
            CaptureConfig::new()
                .activate_all()
                .retention(MemorySinkConfig::new(if *n == 2 { 3 } else { 10 }, 1.0).unwrap()),
            || production(*n),
        )
    });
    runner.data_source([1, 2]);
    let cases = runner.case_aggregate();
    runner.check_aggregate(fact!(cases.spans::<Work>().unwrap().values().len()).expect(3usize));
    runner.check_aggregate(
        fact!(cases.observations::<Work>().unwrap().values().len()).expect(5usize),
    );
    let report = runner.report();
    assert!(!report.is_pass());
    assert_eq!(report.aggregate_failures.len(), 2);
    assert!(report
        .aggregate_failures
        .iter()
        .all(|f| f.class == CheckClass::EvaluationError));
}

#[test]
fn zero_span_positive_cases_are_retained_and_cardinality_is_explicit() {
    let mut runner = certification("empty span policy").executor(|_: &usize| {
        capture_observations(CaptureConfig::new().activate_all(), || Ok::<_, ()>(()))
    });
    runner.data_source([1, 1]);
    let cases = runner.case_aggregate();
    runner.check_aggregate(fact!(cases.cases().unwrap().len()).expect(2usize));
    runner.check_aggregate(fact!(cases.spans::<Work>().unwrap().values().len()).expect(0usize));
    runner.check_aggregate(
        fact!(cases.spans::<Work>().unwrap().values().len()).expect(CountRule::NonEmpty),
    );
    let report = runner.report();
    assert!(!report.is_pass());
    assert_eq!(report.aggregate_failures.len(), 1);
    assert_eq!(
        report.aggregate_failures[0].class,
        CheckClass::HardViolation
    );
}
