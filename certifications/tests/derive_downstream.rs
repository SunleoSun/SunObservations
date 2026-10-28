use std::{fs, process::Command};
#[test]
fn single_renamed_dependency_exposes_runtime_evidence_certifications_and_rayon_and_rejects_invalid_metadata(
) {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("crates/sun-observations");
    let manifest = format!("[package]\nname = \"renamed-consumer\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n[dependencies]\nsdk = {{ package = \"sun-observations\", path = {:?} }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\n", runtime.to_string_lossy().replace('\\', "/"));
    fs::write(directory.path().join("Cargo.toml"), manifest).unwrap();
    let pass = r#"
use sdk::{Observation, Selection, MemorySink};
use serde::{Deserialize, Serialize};
use sdk::certifications::*;
use sdk::evidence::{capture_observations, CaptureConfig};
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "downstream::stable", level = INFO, tags("detail"))]
struct Renamed<T> { value: T }
#[derive(Serialize, Observation)]
#[observation(namespace = "downstream::warning", level = WARN)]
struct Warning;
#[derive(Serialize, Observation)]
#[observation(namespace = "downstream::error", level = ERROR)]
struct Error;
fn main() {
    let memory = std::sync::Arc::new(MemorySink::new());
    sdk::global().set_selection(Selection::new().activate_all());
    sdk::global().register_namespace_channel("downstream", "memory").unwrap();
    sdk::global().add_channel_sink("memory", memory.clone());
    assert!(sdk::observe!(Renamed { value: vec![1u32,2] }).unwrap());
    sdk::observe!(Warning).unwrap(); sdk::observe!(Error).unwrap();
    sdk::global().flush_channels().unwrap();
    assert_eq!(memory.len(), 3);
    assert_eq!(memory.records()[0].namespace, "downstream::stable");
    let ordinary = [1u32, 2];
    let mut proof = certification("ordinary values");
    proof.check("count", collection("ordinary", &ordinary).count().expect(2usize));
    proof.assert_certify();
    let run = capture_observations(CaptureConfig::new().activate_namespace("downstream").unwrap(), || {
        let pool = sdk::context::rayon::ContextThreadPool::with_threads(std::num::NonZeroUsize::new(2).unwrap()).unwrap();
        pool.install(|| {
            sdk::observe!(Renamed { value: 1u32 }).unwrap();
            sdk::observe!(Renamed { value: 2u32 }).unwrap();
        });
        [2u32, 1]
    });
    let typed = run.to_typed::<Renamed<u32>>().unwrap();
    let mut proof = run.certification("captured Rayon transfer");
    let left = typed.source("emitted");
    let right = proof.collection("returned", run.result());
    let pair = left.compare_with(&right).on(|row| row.value, |row| *row);
    proof.check("keys", pair.paired_comparison_fact().expect_valid());
    proof.check("values", pair.equal(|row| row.value, |row| *row, 2).expect_valid());
    let numbers = pair.numeric(|row| row.value as f64, |row| *row as f64);
    proof.check("correlation", numbers.correlation().expect(CorrelationExpectation::spearman_at_least(0.99).unwrap()));
    proof.check("difference", numbers.difference().expect(DifferenceExpectation::max_absolute_at_most(0.0).unwrap()));
    proof.assert_certify();
}
"#;
    let source = directory.path().join("src/main.rs");
    fs::write(&source, pass).unwrap();
    let invoke = |subcommand: &str| {
        Command::new(env!("CARGO"))
            .arg(subcommand)
            .arg("--offline")
            .arg("--manifest-path")
            .arg(directory.path().join("Cargo.toml"))
            .arg("--target-dir")
            .arg(
                std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .parent()
                    .unwrap(),
            )
            .arg("--message-format=json")
            .output()
            .unwrap()
    };
    let output = invoke("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let invalid = [
        ("", "missing required observation namespace"),
        ("namespace = \"bad::\"", "namespace must contain"),
        ("namespace = \"Upper\"", "namespace must contain"),
        (
            "namespace = \"valid\", tags(\"bad-tag\")",
            "tags must match",
        ),
        (
            "namespace = \"valid\", namespace = \"other\"",
            "duplicate namespace",
        ),
        (
            "namespace = \"valid\", level = INFO, level = WARN",
            "duplicate level",
        ),
        ("namespace = \"valid\", tags(), tags()", "duplicate tags"),
        (
            "namespace = \"valid\", unknown = true",
            "unknown observation attribute",
        ),
        (
            "namespace = \"valid\", level = DEBUG",
            "level must be INFO, WARN, or ERROR",
        ),
        (
            "namespace = \"valid\", level = TRACE",
            "level must be INFO, WARN, or ERROR",
        ),
    ];
    let mut invalid_source = "use serde::Serialize;\n".to_owned();
    let mut expected = vec![];
    for (index, (metadata, diagnostic)) in invalid.iter().enumerate() {
        let first_line = invalid_source.lines().count() + 1;
        invalid_source.push_str(&format!("#[derive(Serialize, sdk::Observation)]\n#[observation({metadata})]\nstruct Invalid{index};\n"));
        expected.push((first_line, first_line + 2, diagnostic));
    }
    invalid_source.push_str("fn main() {}\n");
    fs::write(&source, invalid_source).unwrap();
    let output = invoke("check");
    assert!(!output.status.success());
    let messages: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for (first, last, diagnostic) in expected {
        assert!(
            messages.iter().any(|event| {
                let message = &event["message"];
                message["level"] == "error"
                    && message["message"]
                        .as_str()
                        .is_some_and(|text| text.contains(diagnostic))
                    && message["spans"].as_array().is_some_and(|spans| {
                        spans.iter().any(|span| {
                            span["is_primary"] == true
                                && span["line_start"]
                                    .as_u64()
                                    .is_some_and(|line| line >= first as u64)
                                && span["line_end"]
                                    .as_u64()
                                    .is_some_and(|line| line <= last as u64)
                        })
                    })
            }),
            "missing rejection at lines {first}..={last}: {diagnostic}; {messages:?}"
        );
    }
}
