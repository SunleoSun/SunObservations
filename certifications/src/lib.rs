use serde_json::Value;
use std::{path::Path, process::Command};
use sun_observations::Observation;

/// Production work-span schema shared by the child process and persisted consumers.
#[derive(Debug, serde::Serialize, serde::Deserialize, Observation)]
#[observation(namespace = "cert::work")]
pub struct Work {
    pub parent: usize,
    pub child: usize,
}

/// Rich production payload used to prove JSONL and memory envelope correspondence.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Rich {
    pub nested: Value,
    pub message: Option<String>,
}
impl Observation for Rich {
    const NAMESPACE: &'static str = "cert::rich";
    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// Independent v1 consumer used by process parents, not runtime deserialization.
pub fn validate(value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("object")?;
    for key in ["ts", "proc", "namespace", "level"] {
        if !value[key].is_string() {
            return Err(key.into());
        }
    }
    if value["proc_ns"].as_u64().is_none() || !object.contains_key("data") {
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
/// Certify persisted span completeness through the canonical evidence interpreter.
pub fn certify_spans<T: sun_observations::Observation + serde::de::DeserializeOwned>(
    records: &[Value],
    expected: usize,
) {
    use sun_observations::certifications::*;
    use sun_observations::evidence::ObservationDataset;
    let records = records
        .iter()
        .map(|record| {
            std::sync::Arc::new(
                serde_json::from_value::<sun_observations::ObservationRecord>(record.clone())
                    .unwrap(),
            )
        })
        .collect();
    let dataset = ObservationDataset::try_from_records(records).unwrap();
    let spans = dataset.to_spans::<T>().unwrap();
    let mut c = certification("persisted span lifecycle");
    c.check(
        "pair count",
        spans.source("completed spans").count().expect(expected),
    );
    c.check(
        "all lifecycle records accounted for",
        collection("records", dataset.records())
            .matching_count(|record| record.phase.is_some())
            .expect(expected * 2),
    );
    c.check(
        "start delivered before end",
        collection("span endpoints", spans.sources())
            .matching_count(|source| source.start_index < source.end_index)
            .expect(expected),
    );
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
