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
fn snapshot_membership_and_shared_payload_identity_survive_append_and_drain() {
    let sink = MemorySink::new();
    let source = record(Input::NAMESPACE, json!({"row":1,"value":2.0}));
    sink.write_shared(source.clone()).unwrap();
    let dataset = ObservationDataset::from_memory(&sink);
    assert!(Arc::ptr_eq(&dataset.records()[0], &source));
    assert!(std::ptr::eq(&dataset.records()[0].data, &source.data));
    sink.write_shared(record(Input::NAMESPACE, json!({"row":2,"value":3.0})))
        .unwrap();
    assert_eq!(sink.len(), 2);
    assert_eq!(dataset.records().len(), 1);
    sink.drain_shared();
    assert_eq!(
        dataset.typed::<Input>().unwrap().values(),
        &[Input { row: 1, value: 2.0 }]
    );
    assert!(Arc::ptr_eq(&dataset.clone().records()[0], &source));
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
    assert_eq!(
        select(EnvelopeQuery {
            namespace: Some("module::input"),
            ..Default::default()
        }),
        vec![0]
    );
    assert_eq!(
        select(EnvelopeQuery {
            namespace_subtree: Some("module::input"),
            ..Default::default()
        }),
        vec![0, 1]
    );
    assert_eq!(
        select(EnvelopeQuery {
            scope: Some(Some("run")),
            ..Default::default()
        }),
        vec![0]
    );
    assert_eq!(
        select(EnvelopeQuery {
            scope: Some(None),
            ..Default::default()
        }),
        vec![1, 2, 3]
    );
    assert_eq!(
        select(EnvelopeQuery {
            level: Some(ObservationLevel::WARN),
            ..Default::default()
        }),
        vec![0]
    );
    assert_eq!(
        select(EnvelopeQuery {
            phase: Some(Some(Phase::Start)),
            ..Default::default()
        }),
        vec![0]
    );
    assert_eq!(
        select(EnvelopeQuery {
            phase: Some(None),
            ..Default::default()
        }),
        vec![1, 2, 3]
    );
    assert_eq!(
        select(EnvelopeQuery {
            span: Some(Some("span")),
            ..Default::default()
        }),
        vec![0]
    );
    assert_eq!(
        select(EnvelopeQuery {
            span: Some(None),
            ..Default::default()
        }),
        vec![1, 2, 3]
    );
    assert_eq!(
        select(EnvelopeQuery {
            proc: Some("test"),
            ..Default::default()
        }),
        vec![0, 1, 2, 3]
    );
    assert!(select(EnvelopeQuery {
        proc: Some("other"),
        ..Default::default()
    })
    .is_empty());
    let q = EnvelopeQuery {
        namespace_subtree: Some("module"),
        scope: Some(Some("run")),
        level: Some(ObservationLevel::WARN),
        phase: Some(Some(Phase::Start)),
        span: Some(Some("span")),
        proc: Some("test"),
        ..Default::default()
    };
    assert_eq!(select(q), vec![0]);
    assert!(Arc::ptr_eq(dataset.query(q).iter().next().unwrap().1, &a));
    assert!(select(EnvelopeQuery {
        level: Some(ObservationLevel::ERROR),
        ..q
    })
    .is_empty());
    assert!(select(EnvelopeQuery {
        scope: Some(Some("missing")),
        ..q
    })
    .is_empty());
    assert!(select(EnvelopeQuery {
        phase: Some(Some(Phase::End)),
        ..q
    })
    .is_empty());
    assert!(select(EnvelopeQuery {
        span: Some(Some("missing")),
        ..q
    })
    .is_empty());
    assert!(select(EnvelopeQuery {
        namespace: Some("absent"),
        ..q
    })
    .is_empty());
}

#[test]
fn strict_decode_reports_original_source_and_skips_only_lifecycle_ends() {
    for data in [
        json!({"row":1}),
        json!({"row":"wrong","value":2}),
        json!(null),
    ] {
        let dataset = ObservationDataset::from_shared(vec![
            record("other", json!(null)),
            record(Input::NAMESPACE, data),
        ]);
        let error = dataset.typed::<Input>().unwrap_err();
        assert_eq!(error.source.dataset_index, 1);
        assert_eq!(error.source.namespace, Input::NAMESPACE);
        assert_eq!(error.source.proc, "test");
        assert!(!error.message.is_empty());
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(serde_json::from_value::<DecodeError>(json).unwrap(), error);
    }
    let mut end = (*record(Input::NAMESPACE, json!(null))).clone();
    end.phase = Some(Phase::End);
    let dataset = ObservationDataset::from_shared(vec![
        Arc::new(end),
        record(Input::NAMESPACE, json!({"row":7,"value":8.0})),
    ]);
    let view = dataset.typed::<Input>().unwrap();
    assert_eq!(view.values().len(), 1);
    assert_eq!(view.sources()[0].0, 1);
    assert!(Arc::ptr_eq(&view.sources()[0].1, &dataset.records()[1]));
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
    assert_eq!(DECODES.load(Ordering::SeqCst), 5);
    for _ in 0..3 {
        assert_eq!(view.numeric(|v| v.0.value).summary().unwrap().count, 5);
        view.numeric(|v| v.0.value).robust_summary().unwrap();
        let joined = view.join_by(&view, |v| v.0.row, |v| v.0.row);
        assert!(joined.alignment().is_complete());
        assert_eq!(
            joined.alignment().left_namespace.as_deref(),
            Some(Input::NAMESPACE)
        );
        joined
            .numeric_relation(|v| v.0.value, |v| v.0.value, 2)
            .unwrap();
    }
    assert_eq!(DECODES.load(Ordering::SeqCst), 5);
}
