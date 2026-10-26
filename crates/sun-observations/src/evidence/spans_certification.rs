use super::*;
use crate::certifications::*;
use crate::evidence::ObservationsQuery;
use crate::test_support::record;
use crate::ObservationLevel;

#[derive(Debug, serde::Serialize, serde::Deserialize, Observation)]
#[observation(namespace = "profile::work")]
struct Work {
    size: u64,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Other {
    size: u64,
}
impl Observation for Other {
    const NAMESPACE: &'static str = "profile::other";
}

fn lifecycle(
    process: &str,
    scope: Option<&str>,
    span: &str,
    phase: Phase,
    ns: u64,
) -> Arc<ObservationRecord> {
    let mut r = record(Work::NAMESPACE);
    r.proc = process.into();
    r.scope = scope.map(str::to_owned);
    r.span = Some(span.into());
    r.phase = Some(phase);
    r.proc_ns = ns;
    r.data = if phase == Phase::Start {
        serde_json::json!({"size": ns})
    } else {
        serde_json::Value::Null
    };
    Arc::new(r)
}
fn dataset(records: Vec<Arc<ObservationRecord>>) -> ObservationDataset {
    ObservationDataset::try_from_records(records).unwrap()
}
#[test]
fn exact_nanoseconds_and_both_original_coordinates_survive_snapshot_drop_and_preparation() {
    let start = lifecycle("p", Some("case"), "a", Phase::Start, 900);
    let end = lifecycle("p", Some("case"), "a", Phase::End, 937);
    let spans = dataset(vec![
        Arc::new(record("point::ignored")),
        start.clone(),
        end.clone(),
    ])
    .to_spans::<Work>()
    .unwrap();
    let measurement = &spans.values()[0];
    assert_eq!(measurement.duration_ns(), 37);
    assert_eq!(measurement.duration(), Duration::from_nanos(37));
    assert_eq!(measurement.value().size, 900);
    assert_eq!(measurement.identity().process(), "p");
    assert_eq!(measurement.identity().scope(), Some("case"));
    assert_eq!(measurement.identity().namespace(), Work::NAMESPACE);
    assert_eq!(measurement.identity().span(), "a");
    let s = &spans.sources()[0];
    assert_eq!((s.start_index, s.end_index), (1, 2));
    assert!(Arc::ptr_eq(&s.start, &start));
    assert!(Arc::ptr_eq(&s.end, &end));
    let prepared = spans.source("work");
    assert!(prepared.count().expect(1usize).is_pass());
    let mut provenance = String::new();
    prepared
        .entry(0)
        .provenance
        .unwrap()
        .describe(0, &mut provenance)
        .unwrap();
    for text in [
        "start_index=1",
        "end_index=2",
        "proc=p",
        "case",
        "profile::work",
        "start_proc_ns=900",
        "end_proc_ns=937",
    ] {
        assert!(provenance.contains(text), "{provenance}");
    }
}
#[test]
fn interleaved_and_reverse_delivery_pair_by_process_scope_namespace_and_span_not_adjacency() {
    let identities = [
        ("p", None, "a"),
        ("p", Some("one"), "a"),
        ("p", Some("two"), "a"),
        ("q", None, "a"),
        ("p", None, "b"),
    ];
    let mut records = Vec::new();
    for (i, (p, scope, id)) in identities.iter().enumerate() {
        records.push(lifecycle(p, *scope, id, Phase::End, 100 + i as u64));
    }
    for (p, scope, id) in identities.iter().rev() {
        records.push(lifecycle(p, *scope, id, Phase::Start, 100));
    }
    let mut other_start = (*lifecycle("p", None, "a", Phase::Start, 40)).clone();
    other_start.namespace = Other::NAMESPACE.into();
    let mut other_end = (*lifecycle("p", None, "a", Phase::End, 47)).clone();
    other_end.namespace = Other::NAMESPACE.into();
    records.extend([Arc::new(other_start), Arc::new(other_end)]);
    let data = dataset(records);
    let spans = data.to_spans::<Work>().unwrap();
    assert_eq!(
        spans
            .values()
            .iter()
            .map(|s| s.duration().as_nanos())
            .collect::<Vec<_>>(),
        vec![4, 3, 2, 1, 0]
    );
    assert_eq!(
        spans
            .sources()
            .iter()
            .map(|s| s.start_index)
            .collect::<Vec<_>>(),
        vec![5, 6, 7, 8, 9]
    );
    let other = data.to_spans::<Other>().unwrap();
    assert_eq!(other.values()[0].duration().as_nanos(), 7);
    assert_eq!(other.values()[0].value().size, 40);
    let selected = data
        .query(ObservationsQuery::new().process("p").scope("one"))
        .to_spans::<Work>()
        .unwrap();
    assert_eq!(selected.values().len(), 1);
}
#[test]
fn missing_duplicate_reversed_and_inconsistent_lifecycles_fail_with_original_coordinates() {
    let start = lifecycle("p", None, "a", Phase::Start, 100);
    let end = lifecycle("p", None, "a", Phase::End, 101);
    let reversed = lifecycle("p", None, "a", Phase::End, 99);
    let mut mismatched = (*end).clone();
    mismatched.level = ObservationLevel::WARN;
    for (records, expected) in [
        (vec![start.clone()], SpanLifecycleError::MissingEnd),
        (vec![end.clone()], SpanLifecycleError::MissingStart),
        (
            vec![start.clone(), end.clone(), start.clone()],
            SpanLifecycleError::DuplicateStart,
        ),
        (
            vec![start.clone(), end.clone(), end.clone()],
            SpanLifecycleError::DuplicateEnd,
        ),
        (
            vec![start.clone(), reversed],
            SpanLifecycleError::ReversedTime,
        ),
        (
            vec![start.clone(), Arc::new(mismatched)],
            SpanLifecycleError::LevelMismatch,
        ),
    ] {
        let data = dataset(records);
        match data.to_spans::<Work>().unwrap_err() {
            SpanError::Lifecycle {
                kind,
                source,
                related,
            } => {
                assert_eq!(kind, expected);
                assert_eq!(source.proc, "p");
                assert_eq!(source.span.as_deref(), Some("a"));
                assert_eq!(
                    related.is_none(),
                    matches!(
                        kind,
                        SpanLifecycleError::MissingStart | SpanLifecycleError::MissingEnd
                    )
                );
            }
            error => panic!("{error}"),
        }
    }
    let data = dataset(vec![start, end]);
    assert!(matches!(
        data.query(ObservationsQuery::new().span_starts())
            .to_spans::<Work>(),
        Err(SpanError::Lifecycle {
            kind: SpanLifecycleError::MissingEnd,
            ..
        })
    ));
    assert!(matches!(
        data.query(ObservationsQuery::new().span_ends())
            .to_spans::<Work>(),
        Err(SpanError::Lifecycle {
            kind: SpanLifecycleError::MissingStart,
            ..
        })
    ));
}
#[test]
fn malformed_start_fails_strictly_while_points_and_empty_namespaces_are_not_durations() {
    let mut start = (*lifecycle("p", None, "a", Phase::Start, 0)).clone();
    start.data = serde_json::json!({"size":"invalid"});
    let data = dataset(vec![
        Arc::new(start),
        lifecycle("p", None, "a", Phase::End, u64::MAX),
    ]);
    assert!(
        matches!(data.to_spans::<Work>(), Err(SpanError::Decode(DecodeError { source, .. })) if source.dataset_index == 0)
    );
    let data = dataset(vec![Arc::new(record(Work::NAMESPACE))]);
    assert!(data.to_spans::<Work>().unwrap().values().is_empty());
    let data = dataset(vec![
        lifecycle("p", None, "a", Phase::Start, 0),
        lifecycle("p", None, "a", Phase::End, u64::MAX),
    ]);
    assert_eq!(
        data.to_spans::<Work>().unwrap().values()[0].duration(),
        Duration::from_nanos(u64::MAX)
    );
}
#[test]
fn prepared_span_evidence_reuses_numeric_group_quantile_comparison_and_anomaly_policy() {
    let mut records = Vec::new();
    for i in 0..5 {
        let id = i.to_string();
        let mut start = (*lifecycle("p", None, &id, Phase::Start, 10)).clone();
        start.data = serde_json::json!({"size":i});
        records.push(Arc::new(start));
        records.push(lifecycle(
            "p",
            None,
            &id,
            Phase::End,
            if i == 4 { 110 } else { 20 },
        ));
    }
    let spans = dataset(records).to_spans::<Work>().unwrap();
    let prepared = spans.source("elapsed ns");
    let numeric = prepared.numeric(|m| m.duration().as_nanos() as f64);
    let mut c = certification("generic analysis over span evidence");
    c.check("median", numeric.quantile(0.5).expect(10.0));
    c.check("finite", numeric.expect(NumericRule::Finite));
    c.check(
        "groups",
        fact(
            "grouped elapsed",
            prepared
                .group_by(|_| "work")
                .numeric(|m| m.duration().as_nanos() as f64)
                .summaries(),
        )
        .expect(GroupExpectation::new(5, 28.0, 28.0).unwrap()),
    );
    c.check(
        "one anomaly",
        numeric
            .outliers(OutlierConfig::default())
            .expect(AtMostAnomalies(1)),
    );
    assert!(!numeric
        .outliers(OutlierConfig::default())
        .expect(AtMostAnomalies(0))
        .is_pass());
    let reference = [(0u64, 10.0), (1, 10.0), (2, 10.0), (3, 10.0), (4, 100.0)];
    let paired = collection("reference", &reference)
        .compare_with(&prepared)
        .on(|r| r.0, |m| m.value().size);
    c.check(
        "correspondence",
        paired.paired_comparison_fact().expect_valid(),
    );
    let comparison = paired.numeric(|r| r.1, |m| m.duration().as_nanos() as f64);
    c.check(
        "reference",
        comparison
            .difference()
            .expect(DifferenceExpectation::max_absolute_at_most(0.0).unwrap()),
    );
    c.check(
        "relation",
        comparison
            .relation()
            .expect(RelationExpectation::spearman_at_least(1.0).unwrap()),
    );
    c.assert_certify();
}
