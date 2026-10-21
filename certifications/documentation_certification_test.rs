extern crate analysis as sun_observations_data_source;
extern crate obs as sun_observations;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use sun_certifications::*;
#[path = "../crates/sun-observations-data-source/examples/skill_workflows.rs"]
mod workflows;
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}
fn skill(name: &str) -> String {
    fs::read_to_string(root().join(format!(".agents/skills/{name}/SKILL.md"))).unwrap()
}
fn blocks(text: &str) -> impl Iterator<Item = &str> {
    text.split(
        "```rust
",
    )
    .skip(1)
    .map(|s| s.split("```").next().unwrap())
}
#[test]
fn canonical_self_contained_skills_have_valid_manifests_complete_categories_and_no_stale_api() {
    let mut c = certify("canonical agent skills");
    for (name, markers) in [
        (
            "sun-observations",
            vec![
                "Observation",
                "NAMESPACE",
                "LEVEL",
                "TAGS",
                "message()",
                "render_text()",
                "observe!",
                "Selection",
                "ScopeConfig",
                "Policy",
                "FirstN",
                "EveryN",
                "MaxPerScope",
                "Context",
                "ContextThreadPool",
                "Channel",
                "Custom Sink",
                "MemorySink",
                "JsonlSink",
                "JsonlConfig",
                "ConsoleSink",
                "ObservationDelivery",
                "flush_channels",
                "span_if_active",
                "FORMAT v1",
                "ObservationDataset",
                "EnvelopeQuery",
                "TypedView",
                "TypedEntry",
                "SourceContext",
                "SourceProvenance",
                "Boundary",
                "Decision",
                "Diagnostic",
                "which certification/consumer uses it",
            ],
        ),
        (
            "certifications",
            vec![
                "Certify semantic behavior, not implementation shape.",
                "evidence",
                "negative paths",
                "causality",
                "metamorphic",
                "oracle",
                "collect",
                "PreparedSource",
                "PreparedKeys",
                "PreparedComparison",
                "PreparedNumericComparison",
                "CompareResult",
                "CheckClass",
                "Measurement",
                "Expectation<T>",
                "Criterion",
                "AllValid",
                "RelationExpectation",
                "GroupExpectation",
                "AtMostAnomalies",
                "LagExpectation",
                "group_by",
                "Spearman",
                "covariance",
                "residual",
                "largest",
                "section",
                "assert_certify",
                "producer",
                "forbidden",
                "bounded",
                "Error",
                "Evidence plan",
                "deterministic fixtures",
            ],
        ),
    ] {
        let text = skill(name);
        let front = text.splitn(3, "---").collect::<Vec<_>>();
        let valid = front.len() == 3
            && front[0].is_empty()
            && front[1].lines().any(|l| l == format!("name: {name}"))
            && front[1]
                .lines()
                .any(|l| l.starts_with("description: ") && l.len() > 20);
        c.section(name, |c| {
            c.check(
                "manifest",
                measured("front matter", valid).compare(Equal(true)),
            );
            for marker in markers {
                c.check(
                    marker,
                    measured(
                        "skill category",
                        text.to_lowercase().contains(&marker.to_lowercase()),
                    )
                    .compare(Equal(true)),
                );
            }
            let headings: Vec<_> = text.lines().filter(|l| l.starts_with("## ")).collect();
            let unique: BTreeSet<_> = headings.iter().collect();
            c.check(
                "no duplicate major sections",
                measured("headings", headings.len()).compare(Equal(unique.len())),
            )
            .check(
                "no nulls",
                measured("content", text.contains(char::from(0))).compare(Equal(false)),
            )
            .check(
                "no obsolete crate",
                measured(
                    "content",
                    text.contains(&["sun", "observations", "analysis"].join("-")),
                )
                .compare(Equal(false)),
            )
            .check(
                "no required external Markdown reading",
                measured("content", text.contains("required reading")).compare(Equal(false)),
            );
        });
    }
    c.check(
        "no obsolete root skill",
        measured("location", root().join("SKILL.md").exists()).compare(Equal(false)),
    )
    .assert_certify();
}
#[test]
fn every_canonical_skill_rust_block_compiles_and_scalar_capture_workflows_execute() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let mut source = String::from("#![allow(dead_code,unused_imports)]\n");
    let mut index = 0;
    for name in ["sun-observations", "certifications"] {
        for block in blocks(&skill(name)) {
            source.push_str(&format!(
                "mod snippet_{index} {{
{block}
"
            ));
            if block.contains("fn capture()") {
                source.push_str(
                    "#[test] fn execute(){capture().unwrap();}
",
                );
            }
            if block.contains("fn ordinary()") {
                source.push_str(
                    "#[test] fn execute(){ordinary();}
",
                );
            }
            if block.contains("fn inspect()") {
                source.push_str(
                    "#[test] fn execute(){inspect().unwrap();}
",
                );
            }
            source.push_str(
                "}
",
            );
            index += 1;
        }
    }
    let path = |name: &str| {
        root()
            .join(format!("crates/{name}"))
            .to_string_lossy()
            .replace(char::from(92), "/")
    };
    let manifest = format!(
        r#"[package]
name="skill-contracts"
version="0.0.0"
edition="2021"
[workspace]
[dependencies]
sun-certifications={{path={:?}}}
sun-observations={{path={:?},features=["rayon"]}}
sun-observations-data-source={{path={:?}}}
serde={{version="1",features=["derive"]}}
serde_json="1"
"#,
        path("sun-certifications"),
        path("sun-observations"),
        path("sun-observations-data-source")
    );
    fs::write(dir.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(dir.path().join("src/lib.rs"), source).unwrap();
    let output = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(dir.path().join("Cargo.toml"))
        .arg("--target-dir")
        .arg(dir.path().join("target"))
        .output()
        .unwrap();
    certify("skill API compiles as documented")
        .check(
            "all Rust blocks",
            measured("downstream compile/run", output.status.success())
                .details(String::from_utf8_lossy(&output.stderr))
                .compare(Equal(true)),
        )
        .assert_certify();
}
#[test]
fn mixed_evidence_workflow_proves_internal_transfer_and_negative_forbidden_absence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence.jsonl");
    workflows::workflows(&path).unwrap();
    workflows::negative_workflow();
    let records: Vec<serde_json::Value> = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mut c = certify("persisted mixed evidence workflow");
    let data = source_slice("artifact", &records);
    c.check("canonical count", data.count().compare(Equal(64)))
        .check(
            "no transient persistence",
            data.matching_count(|r| {
                r.get("render_text").is_some()
                    || r.get("tags").is_some()
                    || r.get("channel").is_some()
            })
            .compare(Equal(0)),
        )
        .check(
            "production namespace",
            data.matching_count(|r| r["namespace"] != "pipeline::prediction")
                .compare(Equal(0)),
        )
        .assert_certify();
}
#[test]
fn current_benchmark_medians_preserve_every_raw_scale_and_sample() {
    let raw = fs::read_to_string(root().join("scripts/certification_metrics.csv")).unwrap();
    let docs = fs::read_to_string(root().join("docs/CERTIFICATION_BENCHMARKS.md")).unwrap();
    let mut samples: BTreeMap<(&str, usize), Vec<(usize, f64)>> = BTreeMap::new();
    for line in raw.lines().skip(1) {
        let p: Vec<_> = line.split(',').collect();
        samples
            .entry((p[0], p[1].parse().unwrap()))
            .or_default()
            .push((p[2].parse().unwrap(), p[3].parse().unwrap()));
    }
    let mut c = certify("benchmark evidence integrity");
    c.check(
        "complete matrix",
        measured("samples", samples.len()).compare(Equal(29 * 4)),
    );
    for ((name, n), mut values) in samples {
        c.check(
            "three raw samples",
            measured(name, values.iter().map(|v| v.0).collect::<Vec<_>>())
                .compare(Equal(vec![0, 1, 2])),
        );
        values.sort_by(|a, b| a.1.total_cmp(&b.1));
        let index = [1000, 10000, 100000, 1000000]
            .iter()
            .position(|v| *v == n)
            .unwrap();
        let line = docs
            .lines()
            .find(|line| line.starts_with(&format!("| {name} |")))
            .unwrap();
        let reported: f64 = line
            .split('|')
            .nth(index + 2)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        c.check(
            format!("{name} {n}"),
            measured("median", reported).compare(Criterion::Range {
                min: values[1].1 - 0.0000005,
                max: values[1].1 + 0.0000005,
            }),
        );
    }
    c.assert_certify();
}
fn markdown(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown(&path, files);
        } else if path.extension().is_some_and(|v| v == "md") {
            files.push(path);
        }
    }
}
#[test]
fn documentation_links_and_architecture_overview_remain_coherent() {
    let mut files = vec![root().join("README.md")];
    markdown(&root().join("docs"), &mut files);
    markdown(&root().join(".agents/skills"), &mut files);
    let mut c = certify("documentation architecture and navigation");
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for target in text
            .split("](")
            .skip(1)
            .map(|s| s.split(')').next().unwrap())
        {
            if target.contains("://") || target.starts_with('#') {
                continue;
            }
            let path = target.split('#').next().unwrap();
            c.check(
                format!("{} -> {target}", file.display()),
                measured("local link", file.parent().unwrap().join(path).exists())
                    .compare(Equal(true)),
            );
        }
    }
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let headings: Vec<_> = readme
        .lines()
        .filter(|v| v.starts_with("## "))
        .take(2)
        .collect();
    c.check(
        "overview then benchmarks",
        measured("README", headings).compare(Equal(vec!["## Overview", "## Benchmarks"])),
    )
    .assert_certify();
}
