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
                "MemorySinkConfig",
                "MemoryStats",
                "CaptureConfig",
                "capture_observations",
                "CapturedRun",
                "run.certify",
                "exclusive",
                "unfinished_contexts",
                "compact_fraction",
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
                "CorrelationExpectation",
                "LinearFitExpectation",
                "DifferenceExpectation",
                "check_fact_expect",
                "comparison_fact",
                "expect_valid",
                "close_to",
                "HardViolation",
                "StatisticalViolation",
                "EvaluationError",
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
            c.check("manifest", fact("front matter", valid).expect(true));
            for marker in markers {
                c.check(
                    marker,
                    fact(
                        "skill category",
                        text.to_lowercase().contains(&marker.to_lowercase()),
                    )
                    .expect(true),
                );
            }
            for obsolete in ["measured(", ".compare(", ".coverage(", "fn execute(engine:"] {
                c.check_fact_expect(
                    format!("no obsolete API: {obsolete}"),
                    text.contains(obsolete),
                    false,
                );
            }
            let headings: Vec<_> = text.lines().filter(|l| l.starts_with("## ")).collect();
            let unique: BTreeSet<_> = headings.iter().collect();
            c.check(
                "no duplicate major sections",
                fact("headings", headings.len()).expect(unique.len()),
            )
            .check(
                "no nulls",
                fact("content", text.contains(char::from(0))).expect(false),
            )
            .check(
                "no obsolete crate",
                fact(
                    "content",
                    text.contains(&["sun", "observations", "analysis"].join("-")),
                )
                .expect(false),
            )
            .check(
                "no required external Markdown reading",
                fact("content", text.contains("required reading")).expect(false),
            );
        });
    }
    c.check(
        "no obsolete root skill",
        fact("location", root().join("SKILL.md").exists()).expect(false),
    )
    .assert_certify();
}
#[test]
fn canonical_skill_rust_blocks_and_readme_capture_compile_and_execute() {
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
                    "#[test] fn run_documented_workflow(){capture().unwrap();}
",
                );
            }
            if block.contains("fn ordinary()") {
                source.push_str(
                    "#[test] fn run_documented_workflow(){ordinary();}
",
                );
            }
            if block.contains("fn inspect()") {
                source.push_str(
                    "#[test] fn run_documented_workflow(){inspect().unwrap();}
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
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let capture_blocks: Vec<_> = readme
        .split("```rust,ignore\n")
        .skip(1)
        .map(|block| block.split("```").next().unwrap())
        .collect();
    for (index, block) in capture_blocks.iter().enumerate() {
        source.push_str(&format!(
            "#[test] fn readme_capture_{index}() -> Result<(), Box<dyn std::error::Error>> {{\n{block}\nOk(())\n}}\n"
        ));
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
    certify("skill and README APIs compile as documented")
        .limits(DiagnosticLimits {
            failures: 1,
            characters_per_failure: 16_384,
        })
        .check_fact_expect(
            "README capture workflow present",
            capture_blocks.len(),
            1usize,
        )
        .check(
            "all Rust blocks",
            fact("downstream compile/run", output.status.success())
                .details(String::from_utf8_lossy(&output.stderr))
                .expect(true),
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
    c.check("canonical count", data.count().expect(64))
        .check(
            "no transient persistence",
            data.matching_count(|r| {
                r.get("render_text").is_some()
                    || r.get("tags").is_some()
                    || r.get("channel").is_some()
            })
            .expect(0),
        )
        .check(
            "production namespace",
            data.matching_count(|r| r["namespace"] != "pipeline::prediction")
                .expect(0),
        )
        .assert_certify();
}
#[test]
fn scoped_capture_benchmark_medians_preserve_every_raw_scale_and_sample() {
    let raw = fs::read_to_string(root().join("scripts/capture_metrics.csv")).unwrap();
    let docs = fs::read_to_string(root().join("docs/CAPTURE_BENCHMARKS.md")).unwrap();
    let mut samples: BTreeMap<(&str, usize), Vec<(usize, f64)>> = BTreeMap::new();
    for line in raw.lines().skip(1) {
        let p: Vec<_> = line.split(',').collect();
        samples
            .entry((p[0], p[1].parse().unwrap()))
            .or_default()
            .push((p[2].parse().unwrap(), p[3].parse().unwrap()));
    }
    let mut c = certify("capture benchmark evidence integrity");
    c.check_fact_expect("complete matrix", samples.len(), 13 * 4);
    for ((name, n), mut values) in samples {
        c.check_fact_expect(
            "all samples",
            values.iter().map(|v| v.0).collect::<Vec<_>>(),
            vec![0usize, 1, 2],
        );
        values.sort_by(|a, b| a.1.total_cmp(&b.1));
        let column = [1000, 10000, 100000, 1000000]
            .iter()
            .position(|v| *v == n)
            .unwrap();
        let line = docs
            .lines()
            .find(|v| v.starts_with(&format!("| {name} |")))
            .unwrap();
        let reported: f64 = line
            .split('|')
            .nth(column + 2)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        c.check_fact_expect(
            format!("{name} {n}"),
            reported,
            close_to(values[1].1).absolute(0.0000005).relative(0.),
        );
    }
    c.assert_certify();
}
#[test]
fn historical_benchmark_medians_preserve_every_raw_scale_and_sample() {
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
        fact("samples", samples.len()).expect(29 * 4),
    );
    for ((name, n), mut values) in samples {
        c.check(
            "three raw samples",
            fact(name, values.iter().map(|v| v.0).collect::<Vec<_>>()).expect(vec![0, 1, 2]),
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
            fact("median", reported).expect(Criterion::Range {
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
                fact("local link", file.parent().unwrap().join(path).exists()).expect(true),
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
        fact("README", headings).expect(vec!["## Overview", "## Benchmarks"]),
    )
    .assert_certify();
}
