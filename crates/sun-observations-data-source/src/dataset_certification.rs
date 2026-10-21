use super::*;
use crate::test_support::record;
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
    let dataset = ObservationDataset::from_memory(&memory);
    let decoded = dataset.typed::<Input>();
    memory
        .write_shared(record(Input::NAMESPACE, json!({"row":99,"value":99.0})))
        .unwrap();
    memory.drain_shared();
    drop(dataset);
    let mut c = certify("typed evidence retains source coordinates and lifetime");
    if let Ok(typed) = &decoded {
        for iteration in 0..3 {
            c.section(format!("iteration {iteration}"), |c| {
                c.check(
                    "membership",
                    measured("typed entries", typed.iter().len()).compare(Equal(4)),
                );
                for (i, entry) in typed.iter().enumerate() {
                    c.section(format!("row {i}"), |c| {
                        c.check(
                            "domain row",
                            measured("entry", entry.value.row).compare(Equal(i as u64)),
                        );
                        c.check(
                            "dataset coordinate",
                            measured("entry", entry.dataset_index).compare(Equal(i + 1)),
                        );
                        c.check(
                            "shared canonical record",
                            measured("entry", Arc::ptr_eq(entry.source, &sources[i]))
                                .compare(Equal(true)),
                        );
                        c.check(
                            "borrowed decoded value",
                            measured("entry", std::ptr::eq(entry.value, &typed.values()[i]))
                                .compare(Equal(true)),
                        );
                        c.check(
                            "process",
                            measured("entry", entry.source.proc.as_str()).compare(Equal("test")),
                        );
                        c.check(
                            "namespace",
                            measured("entry", entry.source.namespace.as_str())
                                .compare(Equal(Input::NAMESPACE)),
                        );
                    });
                }
                c.check(
                    "reverse coordinate",
                    measured(
                        "last entry",
                        typed.iter().next_back().map(|v| v.dataset_index),
                    )
                    .compare(Equal(Some(4))),
                );
                for group in typed.group_by(|v| v.row % 2).iter() {
                    for entry in group.iter() {
                        c.check(
                            "group source coordinate",
                            measured("group", entry.index).compare(Equal(entry.value.row as usize)),
                        );
                        let mut provenance = String::new();
                        let described = entry
                            .provenance
                            .is_some_and(|p| p.describe(entry.index, &mut provenance).is_ok());
                        c.check(
                            "group provenance available",
                            measured("group", described).compare(Equal(true)),
                        );
                        c.check(
                            "group dataset identity",
                            measured(
                                "group provenance",
                                provenance.contains(&format!("dataset_index={}", entry.index + 1)),
                            )
                            .compare(Equal(true)),
                        );
                    }
                }
            });
        }
    }
    c.check(
        "typed decoding",
        measurement("snapshot", decoded).compare(criterion(
            "four original rows",
            CheckClass::Hard,
            |v: &TypedView<Input>| v.values().len() == 4,
        )),
    );
    c.check(
        "empty source",
        measurement(
            "empty snapshot",
            ObservationDataset::from_shared(vec![]).typed::<Input>(),
        )
        .compare(criterion(
            "no entries",
            CheckClass::Hard,
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
    let dataset = ObservationDataset::from_memory(&sink);
    let mut c = certify("fixed shared snapshot survives append and drain");
    c.check(
        "canonical record shared",
        measured("snapshot", Arc::ptr_eq(&dataset.records()[0], &source)).compare(Equal(true)),
    );
    c.check(
        "canonical payload shared",
        measured(
            "snapshot",
            std::ptr::eq(&dataset.records()[0].data, &source.data),
        )
        .compare(Equal(true)),
    );
    sink.write_shared(record(Input::NAMESPACE, json!({"row":2,"value":3.0})))
        .unwrap();
    c.check(
        "live membership",
        measured("sink", sink.len()).compare(Equal(2)),
    );
    c.check(
        "fixed membership",
        measured("snapshot", dataset.records().len()).compare(Equal(1)),
    );
    sink.drain_shared();
    c.check(
        "decoded retained payload",
        measurement("drained snapshot", dataset.typed::<Input>()).compare(criterion(
            "original returned values",
            CheckClass::Hard,
            |v: &TypedView<Input>| v.values() == [Input { row: 1, value: 2.0 }],
        )),
    );
    c.check(
        "clone shares canonical record",
        measured(
            "snapshot clone",
            Arc::ptr_eq(&dataset.clone().records()[0], &source),
        )
        .compare(Equal(true)),
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
    let mut c = certify("envelope selection preserves execution and segment identity");
    let cases = [
        (
            "exact namespace",
            EnvelopeQuery {
                namespace: Some("module::input"),
                ..Default::default()
            },
            vec![0],
        ),
        (
            "namespace subtree",
            EnvelopeQuery {
                namespace_subtree: Some("module::input"),
                ..Default::default()
            },
            vec![0, 1],
        ),
        (
            "named scope",
            EnvelopeQuery {
                scope: Some(Some("run")),
                ..Default::default()
            },
            vec![0],
        ),
        (
            "absent scope",
            EnvelopeQuery {
                scope: Some(None),
                ..Default::default()
            },
            vec![1, 2, 3],
        ),
        (
            "level",
            EnvelopeQuery {
                level: Some(ObservationLevel::WARN),
                ..Default::default()
            },
            vec![0],
        ),
        (
            "span start",
            EnvelopeQuery {
                phase: Some(Some(Phase::Start)),
                ..Default::default()
            },
            vec![0],
        ),
        (
            "point",
            EnvelopeQuery {
                phase: Some(None),
                ..Default::default()
            },
            vec![1, 2, 3],
        ),
        (
            "named span",
            EnvelopeQuery {
                span: Some(Some("span")),
                ..Default::default()
            },
            vec![0],
        ),
        (
            "absent span",
            EnvelopeQuery {
                span: Some(None),
                ..Default::default()
            },
            vec![1, 2, 3],
        ),
        (
            "process",
            EnvelopeQuery {
                proc: Some("test"),
                ..Default::default()
            },
            vec![0, 1, 2, 3],
        ),
        (
            "foreign process",
            EnvelopeQuery {
                proc: Some("other"),
                ..Default::default()
            },
            vec![],
        ),
    ];
    for (name, query, expected) in cases {
        c.check(
            name,
            measured("selected dataset coordinates", select(query)).compare(Equal(expected)),
        );
    }
    let q = EnvelopeQuery {
        namespace_subtree: Some("module"),
        scope: Some(Some("run")),
        level: Some(ObservationLevel::WARN),
        phase: Some(Some(Phase::Start)),
        span: Some(Some("span")),
        proc: Some("test"),
        ..Default::default()
    };
    c.check(
        "all selectors intersect",
        measured("selected dataset coordinates", select(q)).compare(Equal(vec![0])),
    );
    c.check(
        "selection shares canonical record",
        measured(
            "query",
            dataset
                .query(q)
                .iter()
                .next()
                .is_some_and(|(_, r)| Arc::ptr_eq(r, &a)),
        )
        .compare(Equal(true)),
    );
    for (name, query) in [
        (
            "wrong level",
            EnvelopeQuery {
                level: Some(ObservationLevel::ERROR),
                ..q
            },
        ),
        (
            "wrong scope",
            EnvelopeQuery {
                scope: Some(Some("missing")),
                ..q
            },
        ),
        (
            "wrong phase",
            EnvelopeQuery {
                phase: Some(Some(Phase::End)),
                ..q
            },
        ),
        (
            "wrong span",
            EnvelopeQuery {
                span: Some(Some("missing")),
                ..q
            },
        ),
        (
            "wrong namespace",
            EnvelopeQuery {
                namespace: Some("absent"),
                ..q
            },
        ),
    ] {
        c.check(
            name,
            measured("selected dataset coordinates", select(query)).compare(Equal(vec![])),
        );
    }
    c.assert_certify();
}

#[test]
fn strict_decode_reports_original_source_and_skips_only_lifecycle_ends() {
    let mut c = certify("strict decoding rejects invalid evidence with source diagnostics");
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
        let decoded = dataset.typed::<Input>();
        c.section(format!("invalid schema {case}"), |c| {
            c.check(
                "rejected",
                measured("typed decoding", decoded.is_err()).compare(Equal(true)),
            );
            if let Err(error) = decoded {
                c.check(
                    "original dataset coordinate",
                    measured("decode error", error.source.dataset_index).compare(Equal(1)),
                );
                c.check(
                    "namespace",
                    measured("decode error", error.source.namespace.as_str())
                        .compare(Equal(Input::NAMESPACE)),
                );
                c.check(
                    "process",
                    measured("decode error", error.source.proc.as_str()).compare(Equal("test")),
                );
                c.check(
                    "diagnostic present",
                    measured("decode error", !error.message.is_empty()).compare(Equal(true)),
                );
                let roundtrip =
                    serde_json::from_value::<DecodeError>(serde_json::to_value(&error).unwrap());
                c.check(
                    "diagnostic serialization",
                    measurement("decode error", roundtrip).compare(Equal(error)),
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
    let decoded = dataset.typed::<Input>();
    if let Ok(view) = &decoded {
        c.check(
            "only point decoded",
            measured("typed view", view.values().len()).compare(Equal(1)),
        );
        c.check(
            "point coordinates and identity",
            measured(
                "typed view",
                view.sources()
                    .first()
                    .is_some_and(|(index, r)| *index == 1 && Arc::ptr_eq(r, &dataset.records()[1])),
            )
            .compare(Equal(true)),
        );
    }
    c.check(
        "end payload excluded",
        measurement("lifecycle decoding", decoded).compare(criterion(
            "one canonical point",
            CheckClass::Hard,
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
    let view = dataset.typed::<Counted>().unwrap();
    let source = view.source("decoded production trace");
    let pair = source.compare_with(&source).on(|v| v.0.row, |v| v.0.row);
    let values = pair.numeric(|v| v.0.value, |v| v.0.value);
    let numeric = source.numeric(|v| v.0.value);
    let groups = source.group_by(|v| v.0.row % 2);
    let grouped = groups.numeric(|v| v.0.value);
    let mut c = certify("one Observation decode shared by all generic operations");
    for _ in 0..3 {
        c.check(
            "iteration",
            measured(
                "typed entries",
                view.iter().map(|e| e.value.0.row).sum::<u64>(),
            )
            .compare(Equal(10)),
        )
        .check(
            "groups",
            measured("grouped", grouped.summaries().groups.len()).compare(Equal(2)),
        )
        .check("coverage", pair.coverage().compare(AllValid))
        .check(
            "relation",
            values
                .relation()
                .compare(RelationExpectation::positive(0.99).unwrap()),
        )
        .check(
            "numeric count",
            numeric.summary().compare(criterion(
                "all five rows",
                CheckClass::Hard,
                |s: &Summary| s.count == 5,
            )),
        )
        .check(
            "robust",
            numeric.robust_summary().compare(criterion(
                "median two",
                CheckClass::Hard,
                |s: &RobustSummary| s.median == 2.,
            )),
        );
        c.check(
            "lag search",
            source
                .lag_relation_by(
                    &source,
                    |v| (v.0.row as i64, v.0.value * v.0.value),
                    |v| (v.0.row as i64, v.0.value * v.0.value),
                    LagConfig {
                        range: LagRange { start: -1, end: 1 },
                        measure: LagMeasure::Pearson,
                    },
                )
                .compare(LagExpectation { best_lag: 0 }),
        );
    }
    c.check(
        "decode exactly once",
        measured("decoder calls", DECODES.load(Ordering::SeqCst)).compare(Equal(5)),
    )
    .assert_certify();
}
