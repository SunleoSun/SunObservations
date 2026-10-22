use super::*;

#[test]
fn query_builders_replace_incompatible_intent_and_compose_independent_filters() {
    let point = record("module::input", json!({}));
    let mut start = (*point).clone();
    start.scope = Some("run".into());
    start.phase = Some(Phase::Start);
    start.span = Some("span".into());
    let mut end = start.clone();
    end.phase = Some(Phase::End);
    end.data = serde_json::Value::Null;
    let dataset = ObservationDataset::try_from_records(vec![
        point,
        Arc::new(start),
        Arc::new(end),
        record("other::input", json!({})),
    ])
    .unwrap();
    let count = |query| dataset.query(query).iter().count();
    let mut c = certification("query composition");
    c.check_fact_expect("unrestricted", count(ObservationsQuery::new()), 4);
    c.check_fact_expect(
        "missing scope",
        count(ObservationsQuery::new().without_scope()),
        2,
    );
    c.check_fact_expect(
        "exact scope",
        count(ObservationsQuery::new().scope("run")),
        2,
    );
    c.check_fact_expect(
        "independent AND",
        count(
            ObservationsQuery::new()
                .namespace_subtree("module")
                .scope("run")
                .span("span")
                .span_starts()
                .level(ObservationLevel::INFO)
                .process("test"),
        ),
        1,
    );
    c.check_fact_expect(
        "missing span",
        count(ObservationsQuery::new().without_span()),
        2,
    );
    c.check_fact_expect(
        "end replaces points",
        count(ObservationsQuery::new().points().span_ends()),
        1,
    );
    c.check_fact_expect(
        "points replace span",
        count(ObservationsQuery::new().span("span").points()),
        2,
    );
    c.check_fact_expect(
        "span replaces points",
        count(ObservationsQuery::new().points().span("span")),
        2,
    );
    c.check_fact_expect(
        "exact namespace replaces subtree",
        count(
            ObservationsQuery::new()
                .namespace_subtree("module")
                .namespace("other::input"),
        ),
        1,
    );
    c.check_fact_expect(
        "subtree replaces exact namespace",
        count(
            ObservationsQuery::new()
                .namespace("other::input")
                .namespace_subtree("module"),
        ),
        3,
    );
    c.assert_certify();
}

#[test]
fn external_ingestion_rejects_malformed_records_and_accepts_unmatched_span_end() {
    let valid = record("module::input", json!({}));
    let mut c = certification("external ingestion boundary");
    c.check_fact_expect(
        "valid",
        ObservationDataset::try_from_records(vec![valid.clone()]).is_ok(),
        true,
    );
    for mutate in [
        (|r: &mut ObservationRecord| r.ts = "invalid".into()) as fn(&mut ObservationRecord),
        |r| r.namespace = "Upper".into(),
        |r| r.span = Some("span".into()),
        |r| r.phase = Some(Phase::Start),
        |r| {
            r.span = Some("span".into());
            r.phase = Some(Phase::End);
        },
    ] {
        let mut invalid = (*valid).clone();
        mutate(&mut invalid);
        let error = ObservationDataset::try_from_records(vec![valid.clone(), Arc::new(invalid)])
            .unwrap_err();
        c.check_fact_expect("invalid record index", error.index, 1);
    }
    let mut end = (*valid).clone();
    end.phase = Some(Phase::End);
    end.span = Some("span".into());
    end.data = serde_json::Value::Null;
    c.check_fact_expect(
        "unmatched end allowed",
        ObservationDataset::try_from_records(vec![Arc::new(end)]).is_ok(),
        true,
    );
    c.assert_certify();
}
use crate::evidence::test_support::record;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use sun_observations::Sink;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Input {
    row: u64,
    value: f64,
}
impl Observation for Input {
    const NAMESPACE: &'static str = "module::input";
}

#[test]
fn typed_entries_preserve_filtered_coordinates_identity_and_snapshot_after_source_drain() {
    let memory = MemorySink::new();
    memory
        .write_shared(record("other::type", json!({})))
        .unwrap();
    let sources: Vec<_> = (0..4)
        .map(|row| record(Input::NAMESPACE, json!({"row":row,"value":row as f64})))
        .collect();
    for source in &sources {
        memory.write_shared(source.clone()).unwrap();
    }
    let dataset = ObservationDataset::try_from_memory(&memory).unwrap();
    let decoded = dataset.to_typed::<Input>();
    memory
        .write_shared(record(Input::NAMESPACE, json!({"row":99,"value":99.0})))
        .unwrap();
    memory.drain_shared();
    drop(dataset);
    let mut c = certification("typed evidence retains source coordinates and lifetime");
    if let Ok(typed) = &decoded {
        for iteration in 0..3 {
            c.section(format!("iteration {iteration}"), |c| {
                c.check(
                    "membership",
                    fact("typed entries", typed.iter().len()).expect(4),
                );
                for (i, entry) in typed.iter().enumerate() {
                    c.section(format!("row {i}"), |c| {
                        c.check(
                            "domain row",
                            fact("entry", entry.value.row).expect(i as u64),
                        );
                        c.check(
                            "dataset coordinate",
                            fact("entry", entry.dataset_index).expect(i + 1),
                        );
                        c.check(
                            "shared canonical record",
                            fact("entry", Arc::ptr_eq(entry.source, &sources[i])).expect(true),
                        );
                        c.check(
                            "borrowed decoded value",
                            fact("entry", std::ptr::eq(entry.value, &typed.values()[i]))
                                .expect(true),
                        );
                        c.check(
                            "process",
                            fact("entry", entry.source.proc.as_str()).expect("test"),
                        );
                        c.check(
                            "namespace",
                            fact("entry", entry.source.namespace.as_str()).expect(Input::NAMESPACE),
                        );
                    });
                }
                c.check(
                    "reverse coordinate",
                    fact(
                        "last entry",
                        typed.iter().next_back().map(|v| v.dataset_index),
                    )
                    .expect(Some(4)),
                );
                for group in typed.source("inputs").group_by(|v| v.row % 2).iter() {
                    for entry in group.iter() {
                        c.check(
                            "group source coordinate",
                            fact("group", entry.index).expect(entry.value.row as usize),
                        );
                        let mut provenance = String::new();
                        let described = entry
                            .provenance
                            .is_some_and(|p| p.describe(entry.index, &mut provenance).is_ok());
                        c.check(
                            "group provenance available",
                            fact("group", described).expect(true),
                        );
                        c.check(
                            "group dataset identity",
                            fact(
                                "group provenance",
                                provenance.contains(&format!("dataset_index={}", entry.index + 1)),
                            )
                            .expect(true),
                        );
                    }
                }
            });
        }
    }
    c.check(
        "typed decoding",
        try_fact("snapshot", decoded).expect(criterion(
            "four original rows",
            ViolationClass::Hard,
            |v: &TypedView<Input>| v.values().len() == 4,
        )),
    );
    c.check(
        "empty source",
        try_fact(
            "empty snapshot",
            ObservationDataset::from_shared(vec![]).to_typed::<Input>(),
        )
        .expect(criterion(
            "no entries",
            ViolationClass::Hard,
            |v: &TypedView<Input>| v.iter().count() == 0,
        )),
    );
    c.assert_certify();
}

#[test]
fn snapshot_membership_and_shared_payload_identity_survive_append_and_drain() {
    let sink = MemorySink::new();
    let source = record(Input::NAMESPACE, json!({"row":1,"value":2.0}));
    sink.write_shared(source.clone()).unwrap();
    let dataset = ObservationDataset::try_from_memory(&sink).unwrap();
    let mut c = certification("fixed shared snapshot survives append and drain");
    c.check(
        "canonical record shared",
        fact("snapshot", Arc::ptr_eq(&dataset.records()[0], &source)).expect(true),
    );
    c.check(
        "canonical payload shared",
        fact(
            "snapshot",
            std::ptr::eq(&dataset.records()[0].data, &source.data),
        )
        .expect(true),
    );
    sink.write_shared(record(Input::NAMESPACE, json!({"row":2,"value":3.0})))
        .unwrap();
    c.check("live membership", fact("sink", sink.len()).expect(2));
    c.check(
        "fixed membership",
        fact("snapshot", dataset.records().len()).expect(1),
    );
    sink.drain_shared();
    c.check(
        "decoded retained payload",
        try_fact("drained snapshot", dataset.to_typed::<Input>()).expect(criterion(
            "original returned values",
            ViolationClass::Hard,
            |v: &TypedView<Input>| v.values() == [Input { row: 1, value: 2.0 }],
        )),
    );
    c.check(
        "clone shares canonical record",
        fact(
            "snapshot clone",
            Arc::ptr_eq(&dataset.clone().records()[0], &source),
        )
        .expect(true),
    );
    c.assert_certify();
}

#[test]
fn envelope_selectors_preserve_order_identity_and_segment_boundaries() {
    let mut a = (*record("module::input", json!({}))).clone();
    a.scope = Some("run".into());
    a.phase = Some(Phase::Start);
    a.span = Some("span".into());
    a.level = ObservationLevel::WARN;
    let a = Arc::new(a);
    let dataset = ObservationDataset::from_shared(vec![
        a.clone(),
        record("module::input::child", json!({})),
        record("module::inputs", json!({})),
        record("elsewhere", json!({})),
    ]);
    let select = |q| dataset.query(q).iter().map(|(i, _)| i).collect::<Vec<_>>();
    let mut c = certification("envelope selection preserves execution and segment identity");
    let cases = [
        (
            "exact namespace",
            ObservationsQuery::new().namespace("module::input"),
            vec![0],
        ),
        (
            "namespace subtree",
            ObservationsQuery::new().namespace_subtree("module::input"),
            vec![0, 1],
        ),
        (
            "named scope",
            ObservationsQuery::new().scope("run"),
            vec![0],
        ),
        (
            "absent scope",
            ObservationsQuery::new().without_scope(),
            vec![1, 2, 3],
        ),
        (
            "level",
            ObservationsQuery::new().level(ObservationLevel::WARN),
            vec![0],
        ),
        (
            "span start",
            ObservationsQuery::new().span_starts(),
            vec![0],
        ),
        ("point", ObservationsQuery::new().points(), vec![1, 2, 3]),
        ("named span", ObservationsQuery::new().span("span"), vec![0]),
        (
            "absent span",
            ObservationsQuery::new().without_span(),
            vec![1, 2, 3],
        ),
        (
            "process",
            ObservationsQuery::new().process("test"),
            vec![0, 1, 2, 3],
        ),
        (
            "foreign process",
            ObservationsQuery::new().process("other"),
            vec![],
        ),
    ];
    for (name, query, expected) in cases {
        c.check(
            name,
            fact("selected dataset coordinates", select(query)).expect(expected),
        );
    }
    let q = ObservationsQuery::new()
        .namespace_subtree("module")
        .scope("run")
        .level(ObservationLevel::WARN)
        .span_starts()
        .span("span")
        .process("test");
    c.check(
        "all selectors intersect",
        fact("selected dataset coordinates", select(q)).expect(vec![0]),
    );
    c.check(
        "selection shares canonical record",
        fact(
            "query",
            dataset
                .query(q)
                .iter()
                .next()
                .is_some_and(|(_, r)| Arc::ptr_eq(r, &a)),
        )
        .expect(true),
    );
    for (name, query) in [
        ("wrong level", q.level(ObservationLevel::ERROR)),
        ("wrong scope", q.scope("missing")),
        ("wrong phase", q.span_ends()),
        ("wrong span", q.span("missing")),
        ("wrong namespace", q.namespace("absent")),
    ] {
        c.check(
            name,
            fact("selected dataset coordinates", select(query)).expect(vec![]),
        );
    }
    c.assert_certify();
}

#[test]
fn strict_decode_reports_original_source_and_skips_only_lifecycle_ends() {
    let mut c = certification("strict decoding rejects invalid evidence with source diagnostics");
    for (case, data) in [
        json!({"row":1}),
        json!({"row":"wrong","value":2}),
        json!(null),
    ]
    .into_iter()
    .enumerate()
    {
        let dataset = ObservationDataset::from_shared(vec![
            record("other", json!(null)),
            record(Input::NAMESPACE, data),
        ]);
        let decoded = dataset.to_typed::<Input>();
        c.section(format!("invalid schema {case}"), |c| {
            c.check(
                "rejected",
                fact("typed decoding", decoded.is_err()).expect(true),
            );
            if let Err(error) = decoded {
                c.check(
                    "original dataset coordinate",
                    fact("decode error", error.source.dataset_index).expect(1),
                );
                c.check(
                    "namespace",
                    fact("decode error", error.source.namespace.as_str()).expect(Input::NAMESPACE),
                );
                c.check(
                    "process",
                    fact("decode error", error.source.proc.as_str()).expect("test"),
                );
                c.check(
                    "diagnostic present",
                    fact("decode error", !error.message.is_empty()).expect(true),
                );
                let roundtrip =
                    serde_json::from_value::<DecodeError>(serde_json::to_value(&error).unwrap());
                c.check(
                    "diagnostic serialization",
                    try_fact("decode error", roundtrip).expect(error),
                );
            }
        });
    }
    let mut end = (*record(Input::NAMESPACE, json!(null))).clone();
    end.phase = Some(Phase::End);
    let dataset = ObservationDataset::from_shared(vec![
        Arc::new(end),
        record(Input::NAMESPACE, json!({"row":7,"value":8.0})),
    ]);
    let decoded = dataset.to_typed::<Input>();
    if let Ok(view) = &decoded {
        c.check(
            "only point decoded",
            fact("typed view", view.values().len()).expect(1),
        );
        c.check(
            "point coordinates and identity",
            fact(
                "typed view",
                view.sources()
                    .first()
                    .is_some_and(|(index, r)| *index == 1 && Arc::ptr_eq(r, &dataset.records()[1])),
            )
            .expect(true),
        );
    }
    c.check(
        "end payload excluded",
        try_fact("lifecycle decoding", decoded).expect(criterion(
            "one canonical point",
            ViolationClass::Hard,
            |v: &TypedView<Input>| v.values() == [Input { row: 7, value: 8.0 }],
        )),
    );
    c.assert_certify();
}

#[test]
fn prepared_view_analyses_do_not_repeat_deserialization() {
    static DECODES: AtomicUsize = AtomicUsize::new(0);
    #[derive(Debug, Serialize)]
    struct Counted(Input);
    impl Observation for Counted {
        const NAMESPACE: &'static str = Input::NAMESPACE;
    }
    impl<'de> Deserialize<'de> for Counted {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            DECODES.fetch_add(1, Ordering::SeqCst);
            Input::deserialize(d).map(Self)
        }
    }
    let dataset = ObservationDataset::from_shared(
        (0..5)
            .map(|row| record(Input::NAMESPACE, json!({"row":row,"value":row as f64})))
            .collect(),
    );
    let view = dataset.to_typed::<Counted>().unwrap();
    let source = view.source("decoded production trace");
    let pair = source.compare_with(&source).on(|v| v.0.row, |v| v.0.row);
    let values = pair.numeric(|v| v.0.value, |v| v.0.value);
    let numeric = source.numeric(|v| v.0.value);
    let groups = source.group_by(|v| v.0.row % 2);
    let grouped = groups.numeric(|v| v.0.value);
    let mut c = certification("one Observation decode shared by all generic operations");
    for _ in 0..3 {
        c.check(
            "iteration",
            fact(
                "typed entries",
                view.iter().map(|e| e.value.0.row).sum::<u64>(),
            )
            .expect(10),
        )
        .check(
            "groups",
            fact("grouped", grouped.summaries().groups.len()).expect(2),
        )
        .check(
            "paired_comparison_fact",
            pair.paired_comparison_fact().expect_valid(),
        )
        .check(
            "relation",
            values
                .correlation()
                .expect(CorrelationExpectation::spearman_at_least(0.99).unwrap()),
        )
        .check(
            "fit reuse",
            values.linear_fit().slope().expect(close_to(1.).unwrap()),
        )
        .check(
            "difference reuse",
            values
                .difference()
                .expect(DifferenceExpectation::max_absolute_at_most(0.).unwrap()),
        )
        .check(
            "numeric count",
            numeric.summary().expect(criterion(
                "all five rows",
                ViolationClass::Hard,
                |s: &Summary| s.count == 5,
            )),
        )
        .check(
            "robust",
            numeric.robust_summary().expect(criterion(
                "median two",
                ViolationClass::Hard,
                |s: &RobustSummary| s.median == 2.,
            )),
        );
        c.check(
            "lag search",
            source
                .lag_with(
                    &source,
                    |v| (v.0.row as i64, v.0.value * v.0.value),
                    |v| (v.0.row as i64, v.0.value * v.0.value),
                    LagConfig {
                        range: LagRange::new(-1, 1).unwrap(),
                        measure: LagMeasure::Pearson,
                    },
                )
                .expect(LagExpectation { best_lag: 0 }),
        );
    }
    c.check(
        "decode exactly once",
        fact("decoder calls", DECODES.load(Ordering::SeqCst)).expect(5),
    )
    .assert_certify();
}
