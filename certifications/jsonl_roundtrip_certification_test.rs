use sun_observations_certifications::{certify, pair, validate};
#[test]
fn independent_v1_consumer_validates_diverse_payloads_and_lifecycle() {
    let records = certify(env!("CARGO_BIN_EXE_scenario"), "roundtrip");
    assert_eq!(records.len(), 1004);
    pair(&records, 1);
    let proc = &records[0]["proc"];
    let mut elapsed = 0;
    for r in &records {
        assert_eq!(&r["proc"], proc);
        let next = r["proc_ms"].as_u64().unwrap();
        assert!(next >= elapsed);
        elapsed = next;
    }
    for (i, r) in records[..1000].iter().enumerate() {
        assert_eq!(r["data"]["nested"]["sequence"], i);
        assert_eq!(r["data"]["nested"]["object"]["text"], "λ 日本語\n\"\\");
        assert_eq!(r["data"]["nested"]["tags"], "payload");
        assert_eq!(r.get("message").is_some(), i % 2 == 0);
        assert!(r.get("scope").is_none());
        assert!(r.get("phase").is_none());
        assert!(r.get("span").is_none());
    }
    assert_eq!(records[1002]["level"], "WARN");
    assert_eq!(records[1003]["level"], "ERROR");
    let point = records[0].clone();
    for field in ["ts", "proc", "proc_ms", "namespace", "level", "data"] {
        let mut invalid = point.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(validate(&invalid).is_err());
    }
    for (field, value) in [
        ("scope", serde_json::Value::Null),
        ("level", serde_json::json!("DEBUG")),
        ("phase", serde_json::json!("start")),
        ("tags", serde_json::json!([])),
        ("ts", serde_json::json!("2026-01-01T00:00:00Z")),
    ] {
        let mut invalid = point.clone();
        invalid[field] = value;
        assert!(validate(&invalid).is_err());
    }
    let second = certify(env!("CARGO_BIN_EXE_scenario"), "roundtrip");
    assert_ne!(second[0]["proc"], *proc);
}
