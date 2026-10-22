use sun_observations::certifications::*;
use sun_observations_certifications::{certify as run_scenario, pair, validate};
#[test]
fn independent_v1_consumer_validates_diverse_payloads_and_lifecycle() {
    let records = run_scenario(env!("CARGO_BIN_EXE_scenario"), "roundtrip");
    let mut c = certify("independent v1 consumer");
    c.check(
        "record count",
        source_slice("persisted records", &records)
            .count()
            .expect(1004usize),
    );
    pair(&records, 1);
    let proc = &records[0]["proc"];
    let mut elapsed = 0;
    c.section("envelope", |c| {
        for r in &records {
            c.check_fact_expect("process identity", &r["proc"], proc);
            let next = r["proc_ms"].as_u64().unwrap();
            c.check(
                "monotonic clock",
                fact("elapsed milliseconds", next).expect(Criterion::AtLeast(elapsed)),
            );
            elapsed = next;
        }
    });
    for (i, r) in records.iter().take(1000).enumerate() {
        c.section(format!("point {i}"), |c| {
            c.check_fact_expect(
                "sequence",
                &r["data"]["nested"]["sequence"],
                &serde_json::json!(i),
            );
            c.check_fact_expect(
                "unicode and escaping",
                r["data"]["nested"]["object"]["text"].as_str(),
                Some("λ 日本語\n\"\\"),
            );
            c.check_fact_expect(
                "payload tags",
                r["data"]["nested"]["tags"].as_str(),
                Some("payload"),
            );
            c.check_fact_expect("message presence", r.get("message").is_some(), i % 2 == 0);
            for absent in ["scope", "phase", "span"] {
                c.check_fact_expect(format!("absent {absent}"), r.get(absent).is_none(), true);
            }
        });
    }
    c.check_fact_expect(
        "warning level",
        records[1002]["level"].as_str(),
        Some("WARN"),
    );
    c.check_fact_expect(
        "error level",
        records[1003]["level"].as_str(),
        Some("ERROR"),
    );
    let point = records[0].clone();
    c.section("schema rejection", |c| {
        for field in ["ts", "proc", "proc_ms", "namespace", "level", "data"] {
            let mut invalid = point.clone();
            invalid.as_object_mut().unwrap().remove(field);
            c.check_fact_expect(
                format!("missing {field}"),
                validate(&invalid).is_err(),
                true,
            );
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
            c.check_fact_expect(
                format!("invalid {field}"),
                validate(&invalid).is_err(),
                true,
            );
        }
    });
    let second = run_scenario(env!("CARGO_BIN_EXE_scenario"), "roundtrip");
    c.check(
        "new process identity",
        fact("second process", &second[0]["proc"]).expect(NotEqual(proc)),
    );
    c.assert_certify();
}
