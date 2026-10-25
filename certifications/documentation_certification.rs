use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use sun_observations::certifications::*;
#[path = "../crates/sun-observations/examples/skill_workflows.rs"]
mod workflows;
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

const SKILLS: [&str; 4] = [
    "certifications-methodology",
    "certifications-api",
    "domain-api",
    "observations-api",
];

fn canonical_skill_path(name: &str) -> PathBuf {
    assert!(SKILLS.contains(&name), "unknown skill {name}");
    root()
        .join("crates/sun-observations/agent-skills")
        .join(name)
        .join("SKILL.md")
}

fn skill(name: &str) -> String {
    fs::read_to_string(canonical_skill_path(name)).unwrap()
}

fn skill_mirror_check(canonical: Vec<u8>, mirror: Vec<u8>) -> CheckResult<Vec<u8>> {
    fact("synchronized skill bytes", mirror).expect(canonical)
}

#[test]
fn stale_mirror_is_a_hard_violation_even_with_valid_skill_metadata() {
    let canonical = b"---\nname: certifications-api\ndescription: Valid synchronized skill fixture\n---\ncanonical API".to_vec();
    let stale =
        b"---\nname: certifications-api\ndescription: Valid synchronized skill fixture\n---\nstale API"
            .to_vec();
    certification("mirror drift rejection")
        .check_fact_expect(
            "identical mirror",
            skill_mirror_check(canonical.clone(), canonical.clone()).is_pass(),
            true,
        )
        .check_fact_expect(
            "stale mirror",
            skill_mirror_check(canonical, stale).class(),
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

fn require_markers(name: &str, markers: &[&str], forbidden: &[&str]) {
    let text = skill(name);
    let mut c = certification(format!("{name} client contract"));
    for marker in markers {
        c.check_fact_expect(format!("documents {marker}"), text.contains(marker), true);
    }
    for marker in forbidden {
        c.check_fact_expect(
            format!("omits internal or stale {marker}"),
            text.contains(marker),
            false,
        );
    }
    c.assert_certify();
}

#[test]
fn canonical_skills_are_atomic_valid_and_synchronized() {
    let mut c = certification("canonical skill manifests and mirrors");
    for name in SKILLS {
        let canonical = canonical_skill_path(name);
        let text = fs::read_to_string(&canonical).unwrap();
        c.check_fact_expect(
            format!("{name} front matter"),
            text.starts_with(&format!("---\nname: {name}\n")),
            true,
        );
        c.check_fact_expect(
            format!("{name} description"),
            text.lines().any(|line| line.starts_with("description: ")),
            true,
        );
        let mirror = root().join(".agents/skills").join(name).join("SKILL.md");
        c.check(
            format!("{name} synchronized mirror"),
            try_fact(
                format!("{name} mirror bytes"),
                fs::read(&mirror).map_err(|error| format!("{}: {error}", mirror.display())),
            )
            .expect(fs::read(&canonical).unwrap()),
        );
    }
    c.assert_certify();
}

#[test]
fn methodology_skill_covers_proof_design_domain_and_migration_without_api_duplication() {
    require_markers(
        "certifications-methodology",
        &[
            "semantic owner",
            "Certify semantic behavior, not implementation shape.",
            "Evidence priority",
            "certification only",
            "certification + Observations",
            "Boundary Observation",
            "Decision Observation",
            "Diagnostic Observation",
            "Domain as input-proof strategy",
            "family of inputs",
            "validation or rejection",
            "Concrete case + Domain",
            "Domain only",
            "Owner and composition proof",
            "must happen",
            "must never happen",
            "domain identity",
            "causality",
            "statistical expectation",
            "evaluation error",
            "independent oracle",
            "metamorphic",
            "null/control scenario",
            "wall-clock thresholds",
            "fails closed",
        ],
        &[
            "DomainRegion",
            "CaseDomainCoordinateSlot",
            "diagnostic_bytes",
        ],
    );
}

#[test]
fn certifications_api_skill_covers_client_workflows_and_omits_framework_internals() {
    require_markers(
        "certifications-api",
        &[
            "certification NAME",
            "executor FUNCTION",
            "executor_with_seed FUNCTION",
            "data_source INPUTS",
            "domain DOMAIN",
            "cases_budget N",
            "failure_budget N",
            "seed N",
            "report_mode BOOL",
            "require_pairwise_coverage",
            "assert_certify",
            "section(name",
            "source(name",
            "collection(name",
            "fact(",
            "try_fact(",
            "fact!(expression)",
            "case.input()",
            "case.result()",
            "case.observations<T>()",
            "Criterion::between",
            "close_to",
            "CountRule",
            "NumericRule",
            "Expectation<T>",
            "PreparedSource",
            "PreparedKeys",
            "PreparedValues",
            "PreparedNumeric",
            "single_map",
            "compare_with",
            "group_by",
            "GroupedView",
            "GroupedNumeric",
            "on_presorted",
            "PairedComparison",
            "paired_comparison_fact",
            "PreparedNumericComparison",
            "CorrelationExpectation",
            "LinearFitExpectation",
            "DifferenceExpectation",
            "RelationExpectation",
            "OutlierConfig",
            "JumpConfig",
            "LagRange",
            "LagConfig",
            "lag_relation_by",
            "require_complete()",
        ],
        &[
            "diagnostic_bytes",
            "case.execution()",
            "CertificationExecution",
            "DomainRegion",
            "DomainTercile",
            "DomainNegativeDistance",
            "CaseDomainCoordinateSlot",
            ".compile()",
            ".classify(",
        ],
    );
}

#[test]
fn domain_api_skill_covers_the_client_dsl_without_solver_representation() {
    require_markers(
        "domain-api",
        &[
            "DomainInput",
            "domain TYPE",
            "FIELD.CONSTRUCTIVE",
            "CONSTRUCTIVE :=",
            "fixed_value(VALUE)",
            "selected_values(VALUES)",
            "in_range(RANGE)",
            "one_of_ranges(RANGES)",
            "all_enum_values()",
            "selected_enum_values(VALUES)",
            "FIELD {",
            "EnumType::Variant {",
            "ENUM_SELECTION :=",
            "length().LENGTH_SPACE",
            "each(ELEMENT_SPACE)",
            "each {",
            "when(RELATION)",
            "FIELD.equal(OPERAND)",
            "FIELD.not_equal(OPERAND)",
            "FIELD.less_than(OPERAND)",
            "FIELD.less_equal(OPERAND)",
            "FIELD.greater_than(OPERAND)",
            "FIELD.greater_equal(OPERAND)",
            "payload enums",
            "data_source",
            "pairwise",
            "no shrinking",
        ],
        &[
            "DomainRegion",
            "DomainTercile",
            "DomainNegativeDistance",
            "CaseDomainCoordinateSlot",
            "CertificationExecution",
            ".compile()",
            ".classify(",
        ],
    );
}

#[test]
fn observations_api_skill_covers_runtime_evidence_client_surface() {
    require_markers(
        "observations-api",
        &[
            "Observation",
            "observe!",
            "observe_if_active",
            "Selection",
            "ScopeConfig",
            "Policy",
            "Context",
            "context::thread::spawn",
            "ContextThreadPool",
            "add_channel_sink",
            "register_namespace_channel",
            "MemorySink",
            "JsonlSink",
            "ConsoleSink",
            "Sink",
            "observe_span!",
            "CaptureConfig",
            "capture_observations",
            "CapturedRun",
            "run.certification(name)",
            "ObservationDataset",
            "ObservationsQuery",
            "TypedView",
            "SourceProvenance",
            "FORMAT v1",
        ],
        &["CertificationExecution", "prerequisite()", "outcome()"],
    );
}

#[test]
fn canonical_skill_rust_blocks_compile_and_execute() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let mut source = String::from("#![allow(dead_code,unused_imports)]\n");
    let mut index = 0;
    for name in SKILLS {
        for block in blocks(&skill(name)) {
            source.push_str(&format!("mod snippet_{index} {{\n{block}\n"));
            if block.contains("fn capture()") {
                source.push_str("#[test] fn run_documented_workflow(){capture().unwrap();}\n");
            }
            if block.contains("fn certify_output()") {
                source.push_str("#[test] fn run_documented_workflow(){certify_output();}\n");
            }
            if block.contains("fn certify_execution()") {
                source.push_str("#[test] fn run_documented_workflow(){certify_execution();}\n");
            }
            source.push_str("}\n");
            index += 1;
        }
    }
    let path = root()
        .join("crates/sun-observations")
        .to_string_lossy()
        .replace(char::from(92), "/");
    let manifest = format!(
        r#"[package]
name="skill-contracts"
version="0.0.0"
edition="2021"
[workspace]
[dependencies]
sun-observations={{path={path:?}}}
serde={{version="1",features=["derive"]}}
serde_json="1"
"#
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
    certification("skill Rust blocks compile as documented")
        .limits(DiagnosticLimits {
            failures: 1,
            characters_per_failure: 16_384,
        })
        .check_fact_expect("Rust blocks present", index, CountRule::Minimum(3))
        .check(
            "downstream compile/run",
            fact("cargo test", output.status.success())
                .details(format!(
                    "stdout: {} stderr: {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ))
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
    workflows::planned_workflow();
    let records: Vec<serde_json::Value> = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mut c = certification("persisted mixed evidence workflow");
    let data = collection("artifact", &records);
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
fn csv_data_rows(path: &str) -> usize {
    fs::read_to_string(root().join(path))
        .unwrap()
        .lines()
        .skip(1)
        .count()
}
#[test]
fn tracked_benchmark_datasets_are_retained() {
    let expected = [
        ("scripts/benchmark_metrics.csv", 62usize),
        ("scripts/capture_metrics.csv", 156),
        ("scripts/generative_certification_metrics.csv", 70),
        ("scripts/certification_metrics.csv", 348),
        ("scripts/analysis_metrics.csv", 240),
        ("scripts/analysis_extension_metrics.csv", 336),
        ("scripts/human_output.csv", 80),
    ];
    let mut c = certification("tracked benchmark evidence");
    for (path, rows) in expected {
        c.check_fact_expect(path, csv_data_rows(path), rows);
    }
    c.assert_certify();
}
#[test]
fn generated_benchmark_charts_match_tracked_evidence() {
    let python = std::env::var_os("PYTHON").unwrap_or_else(|| "python".into());
    let output = Command::new(python)
        .current_dir(root())
        .args(["scripts/render_benchmark_charts.py", "--check"])
        .output()
        .unwrap();
    certification("generated benchmark chart freshness")
        .check(
            "renderer check",
            fact("chart check", output.status.success())
                .details(format!(
                    "stdout: {} stderr: {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ))
                .expect(true),
        )
        .assert_certify();
}
const README_CERTIFICATION_API: &[&str] = &[
    "certification(name)",
    "check(name",
    "check_fact_expect",
    "section(name",
    "source(name",
    "collection(name",
    "fact(source",
    "try_fact",
    "Fact::details",
    "Fact::expect",
    "fact!(expression)",
    "CheckPlan::description",
    "assert_certify()",
    "NotEqual",
    "Criterion::greater_than",
    "Criterion::at_least",
    "Criterion::less_than",
    "Criterion::at_most",
    "Criterion::between",
    "criterion(description",
    "close_to",
    "absolute_tolerance",
    "relative_tolerance",
    "CountRule::Exact",
    "CountRule::Minimum",
    "CountRule::NonEmpty",
    "NumericRule::Finite",
    "NumericRule::Positive",
    "NumericRule::NonNegative",
    "NumericRule::between",
    "expect_valid()",
    "AtMostAnomalies",
    "GroupExpectation::new",
    "LagExpectation",
    "Expectation<T>",
    "executor(f)",
    "executor_with_seed(f)",
    "data_source(inputs)",
    "domain(domain)",
    "cases_budget(n)",
    "failure_budget(n)",
    "seed(n)",
    "report_mode(bool)",
    "require_pairwise_coverage()",
    "case()",
    "case.input()",
    "case.result()",
    "case.observations::<T>()",
];

const README_DOMAIN_API: &[&str] = &[
    "DomainInput",
    "domain!(Type",
    "fixed_value(value)",
    "selected_values(values)",
    "in_range(range)",
    "one_of_ranges(ranges)",
    "all_enum_values()",
    "selected_enum_values(values)",
    "field {",
    "Enum::Variant",
    "length()",
    "each(rule)",
    "each {",
    "when(relation)",
    "equal",
    "not_equal",
    "less_than",
    "less_equal",
    "greater_than",
    "greater_equal",
];

const README_PREPARED_API: &[&str] = &[
    "`name()` | Returns semantic source name",
    "with_provenance",
    "single_map",
    "matching_count",
    "PreparedKeys",
    "PreparedNumeric",
    "GroupedView",
    "PairedComparison::name()",
    "PreparedNumericComparison",
];

const README_OBSERVATION_API: &[&str] = &[
    "derive(Observation)",
    "NAMESPACE",
    "LEVEL",
    "TAGS",
    "message()",
    "render_text()",
    "observe!(value)",
    "observe_if_active",
    "ObservationEngine::isolated()",
    "process_id()",
    "sink_error_count()",
    "set_selection",
    "flush_channels()",
    "Selection::new()",
    "activate_all()",
    "activate_namespace",
    "exclude_namespace",
    "activate_tags",
    "exclude_tags",
    "activates(namespace, tags)",
    "excludes(namespace, tags)",
    "admits(namespace, tags)",
    "namespace::is_normalized_segment",
    "namespace::matches_subtree",
    "namespace::validate_namespace",
    "ScopeConfig::new",
    "selection(selection)",
    "override_policy",
    "Policy::first_n",
    "Policy::every_n",
    "Context::capture",
    "enter()",
    "run(operation)",
    "scope_name()",
    "context::thread::spawn",
    "ContextThreadPool",
    "with_threads",
    "install(operation)",
];

const README_EVIDENCE_API: &[&str] = &[
    "add_channel_sink",
    "register_namespace_channel",
    "clear_channel_sinks",
    "MemorySink",
    "with_config",
    "shared_records",
    "drain_shared",
    "records()",
    "drain()",
    "MemorySinkConfig",
    "max_items()",
    "compact_fraction()",
    "MemoryStats",
    "JsonlSink",
    "open_append",
    "from_writer",
    "writer_with_config",
    "JsonlConfig",
    "flush_interval_ms",
    "max_file_size_mb",
    "max_file_size_bytes",
    "trim_fraction",
    "is_started()",
    "last_error()",
    "ConsoleSink",
    "stdout()",
    "stderr()",
    "Sink",
    "write_shared",
    "wants_render_text",
    "write_delivery",
    "ObservationDelivery",
    "shared_record()",
    "shared_render_text()",
    "observe_span!",
    "span_if_active",
    "SpanGuard",
    "is_active()",
    "finish()",
    "CaptureConfig",
    "retention",
    "activate_namespaces",
    "activate_tag",
    "exclude_namespaces",
    "exclude_tag",
    "capture_observations",
    "CapturedRun",
    "status().is_complete()",
    "to_typed",
    "certification(name)",
    "ObservationDataset",
    "try_from_records",
    "try_from_memory",
    "ObservationsQuery",
    "namespace_subtree",
    "without_scope",
    "points",
    "span_starts",
    "span_ends",
    "process",
    "RecordView",
    "TypedView",
    "sources()",
    "TypedEntry",
    "SourceProvenance",
    "DecodeError",
    "SourceContext",
];

const README_NUMERIC_API: &[&str] = &[
    "CorrelationExpectation",
    "pearson_at_least",
    "spearman_at_least",
    "spearman_at_most",
    "with_bound",
    "and_pearson_bounds",
    "and_spearman_bounds",
    "LinearFitExpectation",
    "slope_between",
    "intercept_between",
    "r_squared_at_least",
    "residual_mean_absolute_at_most",
    "residual_max_absolute_at_most",
    "and_slope_between",
    "and_intercept_between",
    "and_r_squared_at_least",
    "and_residual_mean_absolute_at_most",
    "and_residual_max_absolute_at_most",
    "DifferenceExpectation",
    "mean_absolute_at_most",
    "p95_absolute_at_most",
    "max_absolute_at_most",
    "and_mean_absolute_at_most",
    "and_p95_absolute_at_most",
    "and_max_absolute_at_most",
    "RelationExpectation",
    "RelationMeasure",
    "OutlierConfig::new",
    "ZeroMadPolicy::absolute_tolerance",
    "JumpConfig::new",
    "LagRange::new",
    "LagConfig",
    "LagReport::zero_lag",
    "LagAlignment::has_interior_gaps",
    "NumericSeries::new",
    "finite_count()",
    "integrity(rule)",
    "check_count",
    "check_unique",
    "check_contiguous",
    "check_same_keys",
    "join_by",
    "join_presorted_by",
    "require_complete()",
    "linear_metrics",
    "numeric_relation",
    "difference_summary",
    "lag_relation_by",
];

#[test]
fn readme_covers_the_supported_client_api() {
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let api = readme
        .split_once("## Complete supported client API")
        .unwrap()
        .1
        .split_once("## Cookbook")
        .unwrap()
        .0;
    let lines: Vec<_> = api.lines().collect();
    let mut c = certification("README supported client API contract");
    for markers in [
        README_CERTIFICATION_API,
        README_DOMAIN_API,
        README_PREPARED_API,
        README_NUMERIC_API,
        README_OBSERVATION_API,
        README_EVIDENCE_API,
    ] {
        for marker in markers {
            let documented = lines.iter().enumerate().any(|(index, line)| {
                if !line.contains(marker) {
                    return false;
                }
                let start = index.saturating_sub(1);
                let end = (index + 3).min(lines.len().saturating_sub(1));
                lines[start..=end]
                    .iter()
                    .any(|line| line.as_bytes().iter().filter(|byte| **byte == 96).count() >= 2)
            });
            c.check_fact_expect(
                format!("documents and exemplifies {marker}"),
                documented,
                true,
            );
        }
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
    certification("Markdown navigation validation")
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
    for name in SKILLS {
        files.push(canonical_skill_path(name));
    }
    let mut c = certification("documentation architecture and navigation");
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for target in text
            .split("](")
            .skip(1)
            .map(|s| s.split(')').next().unwrap())
        {
            if !target.contains("://") {
                c.check_fact_expect(
                    format!("{} -> {target}", file.display()),
                    local_markdown_link_valid(&file, target),
                    true,
                );
            }
        }
    }
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let headings: Vec<_> = readme
        .lines()
        .filter(|v| v.starts_with("## "))
        .take(5)
        .collect();
    c.check(
        "handbook section order",
        fact("README", headings).expect(vec![
            "## Table of contents",
            "## Overview",
            "## Five-minute tour",
            "## Performance",
            "## How the framework fits together",
        ]),
    );
    c.check_fact_expect(
        "no ignored README Rust examples",
        readme.contains("rust,ignore"),
        false,
    );
    c.assert_certify();
}
