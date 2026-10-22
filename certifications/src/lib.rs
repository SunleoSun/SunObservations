use serde_json::Value;
use std::{collections::BTreeMap, path::Path, process::Command};

/// Independent v1 consumer used by process parents, not runtime deserialization.
pub fn validate(value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("object")?;
    for key in ["ts", "proc", "namespace", "level"] {
        if !value[key].is_string() {
            return Err(key.into());
        }
    }
    if value["proc_ms"].as_u64().is_none() || !object.contains_key("data") {
        return Err("required".into());
    }
    let ts = value["ts"].as_str().unwrap();
    if ts.len() != 24
        || !ts.ends_with('Z')
        || ts.as_bytes()[19] != b'.'
        || !ts.as_bytes()[20..23].iter().all(u8::is_ascii_digit)
        || chrono::DateTime::parse_from_rfc3339(ts).is_err()
    {
        return Err("timestamp".into());
    }
    let namespace = value["namespace"].as_str().unwrap();
    if !namespace.split("::").all(|s| {
        s.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    }) {
        return Err("namespace".into());
    }
    if !matches!(value["level"].as_str(), Some("INFO" | "WARN" | "ERROR")) {
        return Err("level".into());
    }
    for key in ["scope", "message", "phase", "span"] {
        if let Some(v) = object.get(key) {
            if !v.is_string() {
                return Err(key.into());
            }
        }
    }
    for key in ["tags", "channel", "channels"] {
        if object.contains_key(key) {
            return Err(key.into());
        }
    }
    match (object.get("phase"), object.get("span")) {
        (None, None) => (),
        (Some(phase), Some(_)) if phase == "start" => (),
        (Some(phase), Some(_))
            if phase == "end" && value["data"].is_null() && !object.contains_key("message") => {}
        _ => return Err("lifecycle".into()),
    }
    Ok(())
}
/// Read complete physical lines and reject malformed framing/schema.
pub fn read(path: &Path) -> Vec<Value> {
    let bytes = std::fs::read(path).unwrap();
    assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));
    assert!(bytes.ends_with(b"\n"));
    let text = std::str::from_utf8(&bytes).unwrap();
    text.split_terminator('\n')
        .map(|line| {
            assert!(!line.is_empty());
            assert!(!line.ends_with('\r'));
            let value: Value = serde_json::from_str(line).unwrap();
            validate(&value).unwrap();
            value
        })
        .collect()
}
/// Pair interleaved lifecycle records independently of their adjacency.
pub fn pair(records: &[Value], expected: usize) {
    let mut pairs = BTreeMap::new();
    for r in records.iter().filter(|r| r.get("span").is_some()) {
        pairs
            .entry((
                r["proc"].as_str().unwrap().to_owned(),
                r.get("scope")
                    .map(|scope| scope.as_str().unwrap().to_owned()),
                r["namespace"].as_str().unwrap().to_owned(),
                r["span"].as_str().unwrap().to_owned(),
            ))
            .or_insert_with(Vec::new)
            .push(r);
    }
    let mut c = sun_observations::certifications::certification("persisted span lifecycle");
    c.check_fact_expect("pair count", pairs.len(), expected);
    for (identity, pair) in &pairs {
        c.section(format!("{identity:?}"), |c| {
            c.check_fact_expect("two lifecycle halves", pair.len(), 2usize);
            if let [a, b] = pair.as_slice() {
                c.check_fact_expect("start phase", a["phase"].as_str(), Some("start"));
                c.check_fact_expect("end phase", b["phase"].as_str(), Some("end"));
                for key in ["proc", "scope", "namespace", "level", "span"] {
                    c.check_fact_expect(key, &a[key], &b[key]);
                }
                c.check_fact_expect("end has no payload", b["data"].is_null(), true);
                c.check_fact_expect("end has no message", b.get("message").is_none(), true);
                c.check(
                    "ordered time",
                    sun_observations::certifications::fact(
                        "end milliseconds",
                        b["proc_ms"].as_u64().unwrap(),
                    )
                    .expect(
                        sun_observations::certifications::Criterion::at_least(
                            a["proc_ms"].as_u64().unwrap(),
                        )
                        .unwrap(),
                    ),
                );
            }
        });
    }
    c.assert_certify();
}
/// Run each global scenario in a new OS process and compare independent persisted views.
pub fn run_scenario(executable: &str, scenario: &str) -> Vec<Value> {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(executable)
        .arg(scenario)
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{scenario}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let records = read(&directory.path().join("events.jsonl"));
    let memory: Vec<Value> =
        serde_json::from_slice(&std::fs::read(directory.path().join("memory.json")).unwrap())
            .unwrap();
    let canonical = |values: &[Value]| {
        let mut v: Vec<_> = values
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect();
        v.sort();
        v
    };
    sun_observations::certifications::certification("persisted and memory views")
        .check_fact_expect("canonical records", canonical(&records), canonical(&memory))
        .assert_certify();
    records
}
