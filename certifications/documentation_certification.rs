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
fn certification_api_coverage(text: &str) -> CertificationReport {
    let mut c = certify("agent-facing method-level API");
    for (heading, methods) in [
        (
            "Fact and CheckResult",
            vec![
                "fact(",
                "try_fact(",
                "details(",
                "expect(",
                "expect_valid(",
                "slope(",
                "intercept(",
                "r_squared(",
                "is_pass(",
                "assess(",
                "describe(",
                "class(",
            ],
        ),
        (
            "Expectations",
            vec!["criterion(", "close_to(", "absolute(", "relative("],
        ),
        (
            "PreparedSource",
            vec![
                "source(",
                "source_slice(",
                "collection(",
                "name(",
                "values(",
                "entry(",
                "iter(",
                "with_provenance(",
                "select(",
                "project(",
                "numeric(",
                "count(",
                "matching_count(",
                "unique(",
                "contiguous(",
                "keys(",
                "compare_with(",
                "group_by(",
                "lag_relation_by(",
            ],
        ),
        ("PreparedValues", vec!["values(", "source("]),
        (
            "PreparedNumeric",
            vec![
                "values(",
                "series(",
                "summary(",
                "robust_summary(",
                "quantile(",
                "check(",
                "outliers(",
                "jumps(",
            ],
        ),
        ("PreparedKeys", vec!["indices(", "compare_with("]),
        ("GroupedView", vec!["iter(", "numeric("]),
        (
            "Group",
            vec!["len(", "is_empty(", "values(", "iter(", "numeric("],
        ),
        (
            "GroupedNumeric",
            vec!["summaries(", "summaries_with_robust("],
        ),
        ("ComparisonBuilder", vec!["on(", "on_u64(", "on_sorted("]),
        (
            "PreparedComparison",
            vec![
                "name(",
                "comparison(",
                "pairs(",
                "comparison_fact(",
                "equal(",
                "check_pairs(",
                "numeric(",
                "lag(",
            ],
        ),
        (
            "PreparedNumericComparison",
            vec![
                "values(",
                "correlation(",
                "linear_fit(",
                "relation(",
                "difference(",
                "linear(",
                "pearson(",
                "spearman(",
                "largest(",
            ],
        ),
        (
            "Statistical expectation builders and measures",
            vec![
                "positive(",
                "negative(",
                "default(",
                "bound(",
                "pearson(",
                "spearman(",
                "slope(",
                "intercept(",
                "r_squared(",
                "residual_mean_absolute(",
                "residual_max_absolute(",
                "mean_absolute(",
                "p95_absolute(",
                "max_absolute(",
                "MetricConstraint<M>",
                "measure:M",
                "min:Option<f64>",
                "max:Option<f64>",
                "RelationConstraint = MetricConstraint<RelationMeasure>",
            ],
        ),
        (
            "Direct analysis and report/error reference",
            vec![
                "new(",
                "iter(",
                "finite_count(",
                "summary(",
                "quantile(",
                "robust_summary(",
                "check(",
                "outliers(",
                "jumps(",
                "check_count(",
                "check_unique(",
                "check_contiguous(",
                "check_same_keys(",
                "is_valid(",
                "is_complete(",
                "join_by(",
                "join_u64_by(",
                "join_sorted_by(",
                "comparison(",
                "pairs(",
                "require_complete(",
                "set_source_names(",
                "difference(",
                "numeric_relation(",
                "linear_metrics(",
                "pearson(",
                "spearman(",
                "correlation(",
                "linear_fit(",
                "difference_summary(",
                "value(",
                "lag_relation_by(",
                "RelationExpectation::evaluate(",
                "LagExpectation::evaluate(",
            ],
        ),
        (
            "Hierarchy, report and final assertion",
            vec![
                "certify(",
                "source(",
                "collection(",
                "check(",
                "check_fact_expect(",
                "section(",
                "limits(",
                "is_pass(",
                "Certification::assert_certify(",
                "CertificationReport::assert_certify(",
                "bounded_debug(",
            ],
        ),
        (
            "Ordinary + Observation-backed composition",
            vec!["run.certify(", "typed::<Prediction>", "source("],
        ),
    ] {
        let section = text
            .lines()
            .skip_while(|line| line.trim_start_matches('#').trim() != heading)
            .skip(1)
            .take_while(|line| !line.starts_with("## ") && !line.starts_with("### "))
            .collect::<Vec<_>>()
            .join("\n");
        c.section(heading, |c| {
            c.check_fact_expect("owner section", !section.is_empty(), true);
            for method in methods {
                c.check_fact_expect(method, section.contains(method), true);
            }
        });
    }
    for (owner, measures) in [
        (
            "CorrelationExpectation",
            vec![
                "positive(",
                "negative(",
                "pearson(",
                "spearman(",
                "bound(",
                "CorrelationMeasure",
                "Pearson",
                "Spearman",
                "AbsolutePearson",
                "AbsoluteSpearman",
            ],
        ),
        (
            "LinearFitExpectation",
            vec![
                "slope(",
                "intercept(",
                "r_squared(",
                "residual_mean_absolute(",
                "residual_max_absolute(",
                "bound(",
                "LinearFitMeasure",
                "Slope",
                "Intercept",
                "RSquared",
                "ResidualMeanAbsolute",
                "ResidualMaxAbsolute",
            ],
        ),
        (
            "DifferenceExpectation",
            vec![
                "mean_absolute(",
                "p95_absolute(",
                "max_absolute(",
                "bound(",
                "DifferenceMeasure",
                "MeanAbsolute",
                "P95Absolute",
                "MaxAbsolute",
            ],
        ),
        (
            "RelationExpectation",
            vec![
                "positive(",
                "negative(",
                "bound(",
                "RelationMeasure",
                "Pearson",
                "Spearman",
                "AbsolutePearson",
                "AbsoluteSpearman",
                "Slope",
                "Intercept",
                "RSquared",
                "MeanAbsoluteDifference",
                "P95AbsoluteDifference",
                "MaxAbsoluteDifference",
                "ResidualMeanAbsolute",
                "ResidualMaxAbsolute",
            ],
        ),
    ] {
        let row = text
            .lines()
            .find(|line| line.starts_with(&format!("| {owner} |")))
            .unwrap_or("");
        c.section(owner, |c| {
            for measure in measures {
                let present = row
                    .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                    .any(|token| token == measure.trim_end_matches('('));
                c.check_fact_expect(measure, present, true);
            }
        });
    }
    c.certify()
}

#[test]
fn certification_skill_documents_methods_in_their_owning_api_families() {
    certification_api_coverage(&skill("certifications")).assert_certify();
}

#[test]
fn type_names_cannot_mask_missing_prepared_numeric_methods() {
    let text = skill("certifications");
    let (before, numeric) = text.split_once("### PreparedNumericComparison").unwrap();
    let (numeric, after) = numeric.split_once("### Statistical").unwrap();
    for method in ["linear()", "pearson()", "spearman()"] {
        let incomplete = format!(
            "{before}### PreparedNumericComparison{}### Statistical{after}",
            numeric.replace(method, "omitted")
        );
        let report = certification_api_coverage(&incomplete);
        assert_eq!(report.failed, 1, "{method}: {report}");
        assert_eq!(report.failures[0].section, "PreparedNumericComparison/");
        assert!(std::panic::catch_unwind(|| report.assert_certify()).is_err());
    }
}

#[test]
fn absolute_measure_names_cannot_mask_missing_signed_variants() {
    let text = skill("certifications");
    for (prefix, owner) in [
        (
            "CorrelationMeasure::{Pearson,Spearman",
            "CorrelationExpectation/",
        ),
        ("RelationMeasure::{Pearson,Spearman", "RelationExpectation/"),
    ] {
        let incomplete = text.replace(prefix, &prefix.replace("Pearson,", ""));
        let report = certification_api_coverage(&incomplete);
        assert_eq!(report.failed, 1, "{report}");
        assert_eq!(report.failures[0].section, owner);
        assert!(std::panic::catch_unwind(|| report.assert_certify()).is_err());
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}
fn canonical_skill_path(name: &str) -> PathBuf {
    let owner = match name {
        "certifications" => "sun-certifications",
        "sun-observations" => "sun-observations",
        _ => panic!("unknown skill {name}"),
    };
    root()
        .join("crates")
        .join(owner)
        .join("agent-skills")
        .join(name)
        .join("SKILL.md")
}

fn skill(name: &str) -> String {
    fs::read_to_string(canonical_skill_path(name)).unwrap()
}

fn skill_mirror_check(canonical: Vec<u8>, mirror: Vec<u8>) -> CheckResult<Vec<u8>> {
    fact("synchronized skill bytes", mirror).expect(Equal(canonical))
}

#[test]
fn stale_mirror_is_a_hard_violation_even_with_valid_skill_metadata() {
    let canonical = b"---\nname: certifications\ndescription: Valid synchronized skill fixture\n---\ncanonical API".to_vec();
    let stale =
        b"---\nname: certifications\ndescription: Valid synchronized skill fixture\n---\nstale API"
            .to_vec();
    certify("mirror drift rejection")
        .check_fact_expect(
            "identical mirror",
            skill_mirror_check(canonical.clone(), canonical.clone()).is_pass(),
            true,
        )
        .check_fact_expect(
            "stale mirror",
            skill_mirror_check(canonical, stale).class,
            CheckClass::HardViolation,
        )
        .assert_certify();
}
fn blocks(text: &str) -> impl Iterator<Item = String> {
    let mut result = Vec::new();
    let mut fence = None;
    let mut rust = false;
    let mut body = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if trimmed == marker {
                if rust {
                    result.push(std::mem::take(&mut body));
                }
                fence = None;
            } else if rust {
                body.push_str(line);
                body.push('\n');
            }
        } else if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            fence = Some(marker);
            rust = trimmed[marker.len()..].split(',').next() == Some("rust");
        }
    }
    result.into_iter()
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
                "ScopedCapture",
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
                "Comparison",
                "CapturedRun",
                "run.certify",
                "PreparedNumericComparison",
                "CheckResult",
                "CheckClass",
                "Fact",
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
            c.check(
                "root mirror matches canonical package skill",
                skill_mirror_check(
                    text.as_bytes().to_vec(),
                    fs::read(root().join(format!(".agents/skills/{name}/SKILL.md"))).unwrap(),
                ),
            );
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
            for obsolete in [
                "measured(",
                ".compare(",
                ".coverage(",
                "fn execute(engine:",
                "Measurement",
                "CompareResult",
                "Alignment<",
                ".alignment(",
                "aligned pairs",
            ] {
                c.check_fact_expect(
                    format!("no obsolete API: {obsolete}"),
                    text.contains(obsolete),
                    false,
                );
            }
            c.check_fact_expect(
                "no obsolete generic correspondence type",
                text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                    .any(|token| token == "Alignment"),
                false,
            );
            if name == "sun-observations" {
                for generic_api in [
                    "use sun_certifications::*",
                    "check_fact_expect",
                    "expect_valid",
                    "CheckClass",
                    "assert_certify",
                    "c.check(",
                    "c.section(",
                ] {
                    c.check_fact_expect(
                        format!("generic proof stays with certifications: {generic_api}"),
                        text.contains(generic_api),
                        false,
                    );
                }
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
fn canonical_skill_and_focused_reference_rust_blocks_compile_and_execute() {
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
    let mut reference_count = 0usize;
    for path in [
        "docs/CAPTURE.md",
        "docs/CERTIFICATIONS_API.md",
        "docs/OPERATIONS.md",
    ] {
        for block in blocks(&fs::read_to_string(root().join(path)).unwrap()) {
            let body = block
                .lines()
                .filter(|line| !line.starts_with("# "))
                .collect::<Vec<_>>()
                .join("\n");
            source.push_str(&format!(
                "#[test] fn reference_{reference_count}() -> Result<(), Box<dyn std::error::Error>> {{\n{body}\nOk(())\n}}\n"
            ));
            reference_count += 1;
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
    certify("skill and focused reference APIs compile as documented")
        .limits(DiagnosticLimits {
            failures: 1,
            characters_per_failure: 16_384,
        })
        .check_fact_expect(
            "focused reference examples present",
            reference_count,
            CountRule::Minimum(5),
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
    let docs = fs::read_to_string(root().join("docs/history/CERTIFICATION_BENCHMARKS.md")).unwrap();
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

fn markdown_anchors(text: &str) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();
    let mut seen = BTreeMap::<String, usize>::new();
    let mut fence = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if trimmed == marker {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            fence = Some(marker);
            continue;
        }
        let title = trimmed.trim_start_matches('#');
        if title.len() == trimmed.len() || !title.starts_with(' ') {
            continue;
        }
        let slug: String = title
            .trim()
            .trim_end_matches('#')
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
            .map(|c| if c.is_whitespace() { '-' } else { c })
            .collect();
        let count = seen.entry(slug.clone()).or_default();
        anchors.insert(if *count == 0 {
            slug
        } else {
            format!("{slug}-{count}")
        });
        *count += 1;
    }
    anchors
}

fn local_markdown_link_valid(file: &Path, target: &str) -> bool {
    let (path, anchor) = target
        .split_once('#')
        .map_or((target, None), |(p, a)| (p, Some(a)));
    let destination = if path.is_empty() {
        file.to_owned()
    } else {
        file.parent().unwrap().join(path)
    };
    if !destination.exists() {
        return false;
    }
    match anchor {
        Some(anchor) if !anchor.is_empty() => fs::read_to_string(destination)
            .is_ok_and(|text| markdown_anchors(&text).contains(anchor)),
        _ => true,
    }
}

#[test]
fn markdown_navigation_rejects_missing_paths_and_anchors_and_ignores_fenced_headings() {
    let headings = markdown_anchors("# Title\n## `Typed` & meaningful!\n```rust\n# Hidden\n```\n~~~text\n## Also hidden\n~~~\n## Repeat\n## Repeat\n");
    let readme = root().join("README.md");
    certify("Markdown navigation validation")
        .check(
            "heading identity",
            fact("slugs", headings).expect(BTreeSet::from([
                "title".into(),
                "typed--meaningful".into(),
                "repeat".into(),
                "repeat-1".into(),
            ])),
        )
        .check_fact_expect(
            "valid self anchor",
            local_markdown_link_valid(&readme, "#certifications"),
            true,
        )
        .check_fact_expect(
            "valid cross-file anchor",
            local_markdown_link_valid(&readme, "docs/FORMAT.md#envelope"),
            true,
        )
        .check_fact_expect(
            "missing self anchor",
            local_markdown_link_valid(&readme, "#nonexistent-heading"),
            false,
        )
        .check_fact_expect(
            "missing cross-file anchor",
            local_markdown_link_valid(&readme, "docs/FORMAT.md#nonexistent-heading"),
            false,
        )
        .check_fact_expect(
            "missing file",
            local_markdown_link_valid(&readme, "docs/nonexistent.md"),
            false,
        )
        .assert_certify();
}
#[test]
fn documentation_links_and_architecture_overview_remain_coherent() {
    let mut files = vec![root().join("README.md")];
    markdown(&root().join("docs"), &mut files);
    markdown(&root().join(".agents/skills"), &mut files);
    files.extend([
        canonical_skill_path("certifications"),
        canonical_skill_path("sun-observations"),
    ]);
    let mut c = certify("documentation architecture and navigation");
    let map = fs::read_to_string(root().join("docs/README.md")).unwrap();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for target in text
            .split("](")
            .skip(1)
            .map(|s| s.split(')').next().unwrap())
        {
            if target.contains("://") {
                continue;
            }
            c.check(
                format!("{} -> {target}", file.display()),
                fact(
                    "local path and anchor",
                    local_markdown_link_valid(&file, target),
                )
                .expect(true),
            );
        }
        if file.starts_with(root().join("docs")) && file.file_name().unwrap() != "README.md" {
            c.check(
                format!("indexed {}", file.display()),
                fact(
                    "documentation map",
                    map.contains(file.file_name().unwrap().to_str().unwrap()),
                )
                .expect(true),
            );
        }
        if file.starts_with(root().join("docs/history")) {
            c.check(
                format!("historical {}", file.display()),
                fact(
                    "historical status",
                    text.lines().next().unwrap().starts_with("# Historical"),
                )
                .expect(true),
            );
        } else {
            for obsolete in [
                "measured(",
                ".compare(",
                ".coverage(",
                "`Measurement`",
                "Measurement<",
                "CompareResult",
                "Alignment<",
                ".alignment(",
                "aligned pairs",
            ] {
                c.check_fact_expect(
                    format!("{}: no {obsolete}", file.display()),
                    text.contains(obsolete),
                    false,
                );
            }
            c.check_fact_expect(
                format!("{}: no obsolete correspondence type", file.display()),
                text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                    .any(|v| v == "Alignment"),
                false,
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
        "workflows before internals and benchmarks",
        fact("README", headings).expect(vec![
            "## Five-minute tour",
            "## How the framework fits together",
        ]),
    );
    c.check(
        "ordinary before Observation-backed proof",
        fact(
            "tour order",
            readme
                .find("### Certify ordinary Rust values")
                .zip(readme.find("### Observe and certify real production behavior"))
                .is_some_and(|(ordinary, capture)| ordinary < capture),
        )
        .expect(true),
    );
    c.check_fact_expect(
        "no unvalidated ignored README examples",
        readme.contains("rust,ignore"),
        false,
    );
    let release = fs::read_to_string(root().join("docs/RELEASE_VALIDATION.md")).unwrap();
    c.check_fact_expect(
        "release gate is not an application tutorial",
        release.contains("not the application-facing"),
        true,
    );
    c.assert_certify();
}
