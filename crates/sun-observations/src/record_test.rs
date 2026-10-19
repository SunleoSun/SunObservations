use crate::{support_test::record, ObservationRecord, Phase};

#[test]
fn point_record_roundtrips_and_omits_absent_metadata() {
    let record = record("records::point");
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 6);
    for absent in ["scope", "message", "phase", "span", "tags", "channel"] {
        assert!(value.get(absent).is_none());
    }
    assert_eq!(
        serde_json::from_value::<ObservationRecord>(value).unwrap(),
        record
    );
}

#[test]
fn lifecycle_and_message_roundtrip() {
    let mut record = record("records::span");
    record.scope = Some("run-a".into());
    record.message = Some("hello".into());
    record.phase = Some(Phase::Start);
    record.span = Some("span-a".into());
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["phase"], "start");
    assert_eq!(value["scope"], "run-a");
    assert_eq!(
        serde_json::from_value::<ObservationRecord>(value).unwrap(),
        record
    );
}
