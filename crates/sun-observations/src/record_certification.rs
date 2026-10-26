use crate::{test_support::record, ObservationRecord, Phase};

#[test]
fn point_record_roundtrips_and_omits_absent_metadata() {
    let mut certification = sun_observations::certifications::certification(
        "point record roundtrips and omits absent metadata",
    );
    let mut record = record("records::point");
    record.proc_ns = u64::MAX;
    let value = serde_json::to_value(&record).unwrap();
    certification.check_fact_expect(
        "nanosecond integer",
        value["proc_ns"].as_u64(),
        Some(u64::MAX),
    );
    let mut missing_clock = value.clone();
    missing_clock.as_object_mut().unwrap().remove("proc_ns");
    certification.check_fact_expect(
        "clock required",
        serde_json::from_value::<ObservationRecord>(missing_clock).is_err(),
        true,
    );
    certification.check_fact_expect(
        "value.as_object().unwrap().len()",
        &(value.as_object().unwrap().len()),
        &(6),
    );
    for absent in ["scope", "message", "phase", "span", "tags", "channel"] {
        certification.check_fact_expect(
            "value.get(absent).is_none()",
            value.get(absent).is_none(),
            true,
        );
    }
    certification.check_fact_expect(
        "serde_json::from_value::<ObservationRecord>(value).unwrap()",
        &(serde_json::from_value::<ObservationRecord>(value).unwrap()),
        &(record),
    );
    certification.assert_certify();
}

#[test]
fn lifecycle_and_message_roundtrip() {
    let mut certification =
        sun_observations::certifications::certification("lifecycle and message roundtrip");
    let mut record = record("records::span");
    record.scope = Some("run-a".into());
    record.message = Some("hello".into());
    record.phase = Some(Phase::Start);
    record.span = Some("span-a".into());
    let value = serde_json::to_value(&record).unwrap();
    certification.check_fact_expect("value[\"phase\"]", value["phase"].as_str(), Some("start"));
    certification.check_fact_expect("value[\"scope\"]", value["scope"].as_str(), Some("run-a"));
    certification.check_fact_expect(
        "serde_json::from_value::<ObservationRecord>(value).unwrap()",
        &(serde_json::from_value::<ObservationRecord>(value).unwrap()),
        &(record),
    );
    certification.assert_certify();
}
