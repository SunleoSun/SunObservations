use std::{fs, process::Command};
#[test]
fn renamed_temporary_downstream_dependency_derives_emits_and_rejects_invalid_metadata() {
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
use serde::Serialize;
#[derive(Serialize, Observation)]
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
            .arg(directory.path().join("target"))
            .output()
            .unwrap()
    };
    let output = invoke("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (metadata, diagnostic) in [
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
    ] {
        fs::write(&source, format!("use serde::Serialize;\n#[derive(Serialize, sdk::Observation)]\n#[observation({metadata})]\nstruct Invalid;\nfn main() {{}}\n")).unwrap();
        let output = invoke("check");
        assert!(!output.status.success(), "{metadata}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
