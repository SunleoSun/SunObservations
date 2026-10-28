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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExampleExecution {
    Run,
    CompileOnly,
}

struct RustExample {
    body: String,
    execution: ExampleExecution,
}

fn rust_examples(text: &str) -> impl Iterator<Item = RustExample> {
    let mut result = Vec::new();
    let mut fence = None;
    let mut execution = None;
    let mut body = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if trimmed == marker {
                if let Some(execution) = execution {
                    result.push(RustExample {
                        body: std::mem::take(&mut body),
                        execution,
                    });
                }
                fence = None;
            } else if execution.is_some() {
                body.push_str(line);
                body.push('\n');
            }
        } else if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            fence = Some(marker);
            let mut options = trimmed[marker.len()..].split(',');
            execution = if options.next() == Some("rust") {
                let mut execution = ExampleExecution::Run;
                for option in options {
                    assert_eq!(option, "no_run", "unsupported Rust example mode");
                    execution = ExampleExecution::CompileOnly;
                }
                Some(execution)
            } else {
                None
            };
        }
    }
    result.into_iter()
}

fn blocks(text: &str) -> impl Iterator<Item = String> {
    rust_examples(text).map(|example| example.body)
}

struct Language {
    skill: &'static str,
    nodes: &'static [&'static str],
}
const LANGUAGES: &[Language] = &[
    Language {
        skill: "certifications-api",
        nodes: &[
            "CERTIFICATION",
            "SOURCE_BINDING",
            "EAGER_CHECK",
            "SECTION",
            "PLANNED_BODY",
            "CASE",
            "CASE_AGGREGATE",
            "PLANNED_CHECK",
            "AGGREGATE_CHECK",
            "FACT",
            "EXPECTATION",
            "PREPARED_SOURCE_CHECK",
            "NUMERIC_CHECK",
            "GROUP_CHECK",
            "CORRESPONDENCE_CHECK",
            "NUMERIC_COMPARISON_CHECK",
            "ASSERT_CERTIFY",
        ],
    },
    Language {
        skill: "domain-api",
        nodes: &[
            "DOMAIN",
            "RULE",
            "CONSTRUCTIVE",
            "ENUM_RULE",
            "COLLECTION_RULE",
            "WHEN",
            "RELATION",
        ],
    },
    Language {
        skill: "observations-api",
        nodes: &[
            "DECLARATION",
            "EMISSION",
            "SELECTION",
            "SCOPE",
            "ROUTING",
            "SINK",
            "SPAN",
            "CAPTURE",
            "QUERY",
            "TYPED_EVIDENCE",
            "SPAN_EVIDENCE",
        ],
    },
];

fn headings(text: &str) -> Vec<(usize, &str, usize, usize)> {
    let mut headings = Vec::new();
    let mut offset = 0;
    let mut fence = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if trimmed == marker {
                fence = None;
            }
        } else if let Some(marker) = ["~~~", "```"].into_iter().find(|m| trimmed.starts_with(m)) {
            fence = Some(marker);
        } else {
            let level = trimmed.bytes().take_while(|c| *c == b'#').count();
            if level > 0 && trimmed.as_bytes().get(level) == Some(&b' ') {
                headings.push((level, trimmed[level..].trim(), offset, offset + line.len()));
            }
        }
        offset += line.len();
    }
    headings
}

fn sections(text: &str) -> BTreeMap<&str, Vec<&str>> {
    let headings = headings(text);
    let mut result = BTreeMap::<_, Vec<_>>::new();
    for (index, (level, title, _, start)) in headings.iter().enumerate() {
        let end = headings[index + 1..]
            .iter()
            .find(|h| h.0 <= *level)
            .map_or(text.len(), |h| h.2);
        result.entry(*title).or_default().push(&text[*start..end]);
    }
    result
}

fn grammar(text: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::<String, String>::new();
    let mut owner = None;
    let mut active = false;
    for line in text.lines() {
        if line == "~~~text" {
            active = true;
            owner = None;
            continue;
        }
        if line == "~~~" {
            active = false;
            continue;
        }
        if !active {
            continue;
        }
        if let Some((node, body)) = line.split_once(":=") {
            let node = node.trim().to_owned();
            assert!(!result.contains_key(&node), "duplicate grammar node {node}");
            result.insert(node.clone(), body.split_whitespace().collect());
            owner = Some(node);
        } else if let Some(node) = &owner {
            result
                .get_mut(node)
                .unwrap()
                .extend(line.split_whitespace());
        }
    }
    result
}

fn identifiers(text: &str) -> BTreeSet<&str> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .collect()
}

fn grammar_calls(production: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for (index, _) in production.match_indices('(') {
        let mut before = production[..index].trim_end_matches('!');
        if before.ends_with('>') {
            if let Some(start) = before.rfind("::<") {
                before = &before[..start];
            }
        }
        let name = before
            .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap();
        if !name.is_empty() && name.chars().any(char::is_lowercase) && name != "Variant" {
            result.insert(name.to_owned());
        }
    }
    result
}

fn language_errors(language: &Language, text: &str, reference: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let skill_sections = sections(text);
    let readme_sections = sections(reference);
    let productions = grammar(
        skill_sections
            .get("DSL")
            .and_then(|s| s.first())
            .copied()
            .unwrap_or(""),
    );
    let reference_productions = grammar(
        readme_sections
            .get("DSL")
            .and_then(|s| s.first())
            .copied()
            .unwrap_or(""),
    );
    let expected: BTreeSet<_> = language.nodes.iter().copied().collect();
    if productions
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected
    {
        errors.push(format!("{} grammar node ownership", language.skill));
    }
    for node in language.nodes {
        for (document, index, source, level) in [
            ("skill", &skill_sections, text, 2),
            ("README", &readme_sections, reference, 4),
        ] {
            if headings(source)
                .iter()
                .filter(|h| h.1 == *node && h.0 == level)
                .count()
                != 1
            {
                errors.push(format!("{document} {node} must be a peer DSL section"));
            }
            match index.get(node) {
                Some(bodies) if bodies.len() == 1 => {
                    let body = bodies[0];
                    if blocks(body).next().is_none() {
                        errors.push(format!("{document} {node} missing Rust example"));
                    }
                    if let Some(production) = productions.get(*node) {
                        let owned = identifiers(body);
                        for call in grammar_calls(production) {
                            if !owned.contains(call.as_str()) {
                                errors.push(format!(
                                    "{document} {node} missing owned alternative {call}"
                                ));
                            }
                        }
                    }
                }
                _ => errors.push(format!("{document} {node} missing or duplicate section")),
            }
        }
        if !productions.contains_key(*node)
            || productions.get(*node) != reference_productions.get(*node)
        {
            errors.push(format!("{node} grammar differs between skill and README"));
        }
    }
    for other in LANGUAGES
        .iter()
        .filter(|other| other.skill != language.skill)
    {
        for node in other.nodes {
            if skill_sections.contains_key(node) {
                errors.push(format!("{} owns foreign node {node}", language.skill));
            }
        }
    }
    for forbidden in [
        "CertificationExecution",
        "RegisterCheck",
        "PlanExpectation",
        "CaseDomainCoordinate",
        "CaseDomainCoordinateSlot",
        "DomainRegion",
        "DomainTercile",
        "DomainNegativeDistance",
        "case.execution()",
        ".compile()",
        ".classify(",
        "runner.source(",
        "runner.collection(",
    ] {
        if text.contains(forbidden) {
            errors.push(format!("{} leaks {forbidden}", language.skill));
        }
    }
    errors
}

fn reference(readme: &str) -> &str {
    sections(readme)["Complete client DSL and API reference"][0]
}

fn prose_identifiers(text: &str) -> BTreeSet<&str> {
    let mut result = BTreeSet::new();
    let mut in_grammar = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "~~~text" {
            in_grammar = true;
            continue;
        }
        if in_grammar && trimmed == "~~~" {
            in_grammar = false;
            continue;
        }
        if !in_grammar {
            result.extend(identifiers(line));
        }
    }
    result
}

#[test]
fn portable_dsl_parameters_are_explained_outside_the_grammar() {
    for language in LANGUAGES {
        let text = skill(language.skill);
        let productions = grammar(&text);
        let nodes: BTreeSet<_> = productions.keys().map(String::as_str).collect();
        let prose = prose_identifiers(&text);
        let mut missing = BTreeSet::new();
        for production in productions.values() {
            for token in identifiers(production) {
                if nodes.contains(token) {
                    continue;
                }
                let symbolic = token.len() > 1
                    && token
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    && token.chars().any(|c| c.is_ascii_uppercase());
                if symbolic && !prose.contains(token) {
                    missing.insert(token);
                }
            }
        }
        assert!(
            missing.is_empty(),
            "{} has unexplained DSL parameters: {missing:?}",
            language.skill
        );
    }
}

#[test]
fn client_languages_have_owned_sections_alternatives_and_executable_examples() {
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    for language in LANGUAGES {
        let errors = language_errors(language, &skill(language.skill), reference(&readme));
        assert!(errors.is_empty(), "{}: {errors:#?}", language.skill);
    }
    assert!(grammar(&skill("certifications-methodology")).is_empty());
}

#[test]
fn documentation_structure_rejects_missing_misplaced_and_cross_owner_contracts() {
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let original = skill("certifications-api");
    let language = &LANGUAGES[0];
    let body = sections(&original)["PLANNED_BODY"][0];
    let mutations = [
        original.replace("CASE := case = runner.case()", "case = runner.case()"),
        original.replace("## SECTION", "## Stages"),
        original.replace("## SECTION", "### SECTION"),
        original.replace(body, "No examples here.\n\n"),
        original.replace(
            body,
            &body.replace("require_pairwise_coverage", "missing_control"),
        ),
        format!("{original}\n## DOMAIN\nforeign ownership\n"),
        format!("{original}\nRegisterCheck\n"),
        format!("{original}\nrunner.source(unsupported, &value)\n"),
    ];
    for mutated in mutations {
        assert!(!language_errors(language, &mutated, reference(&readme)).is_empty());
    }
    let moved = original
        .replace(
            body,
            &body.replace("require_pairwise_coverage", "missing_control"),
        )
        .replace("## CASE\n", "## CASE\n\nrequire_pairwise_coverage()\n");
    assert!(language_errors(language, &moved, reference(&readme))
        .iter()
        .any(|e| e.contains("PLANNED_BODY missing owned alternative")));
    let missing_readme = reference(&readme).replace("#### SECTION", "#### Stages");
    assert!(!language_errors(language, &original, &missing_readme).is_empty());
    let fake = "## SECTION\n~~~text\n## CASE\n~~~\n";
    assert!(!sections(fake).contains_key("CASE"));
}

#[test]
fn handbook_and_distinct_end_to_end_evidence_workflows_are_retained() {
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let sections = sections(&readme);
    for chapter in [
        "Overview",
        "Five-minute tour",
        "Performance",
        "How the framework fits together",
        "Certifications",
        "Domain generation",
        "Observations",
        "Capture production evidence",
        "Certification methodology",
        "How Domain generation works internally",
        "Cookbook",
        "Production configuration and guarantees",
        "Agent integration",
        "Development and validation",
    ] {
        assert!(
            sections.contains_key(chapter),
            "missing handbook chapter {chapter}"
        );
    }
    let scenarios = sections["End-to-end scenarios"][0];
    let examples: Vec<_> = blocks(scenarios).collect();
    assert!(examples.len() >= 5);
    let mut used = BTreeSet::new();
    for family in [
        &["domain", "data_source", "less_than", "Err"][..],
        &["domain", "data_source", "pow", "section"][..],
        &["keys", "compare_with", "serde_json"][..],
        &["capture_observations", "observations", "observe"][..],
        &["group_by", "summaries", "linear_fit"][..],
    ] {
        let index = examples
            .iter()
            .enumerate()
            .find_map(|(index, block)| {
                let symbols = identifiers(block);
                (!used.contains(&index)
                    && family.iter().all(|name| symbols.contains(name))
                    && symbols.contains("assert_certify")
                    && symbols.contains("fn"))
                .then_some(index)
            })
            .unwrap_or_else(|| panic!("missing standalone executable scenario {family:?}"));
        used.insert(index);
    }
}

fn hidden(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attr| {
        attr.path().is_ident("doc")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|id| id == "hidden")
    })
}

fn method_node(owner: &str, method: &str) -> Option<&'static str> {
    Some(match owner {
        "Certification" => match method {
            "source" | "collection" => "SOURCE_BINDING",
            "check" | "check_fact_expect" => "EAGER_CHECK",
            "section" => "SECTION",
            "executor" | "executor_with_seed" => "CERTIFICATION",
            "assert_certify" => "ASSERT_CERTIFY",
            _ => "CERTIFICATION",
        },
        "PlannedCertification" => match method {
            "case" => "CASE",
            "case_aggregate" => "CASE_AGGREGATE",
            "check_aggregate" => "AGGREGATE_CHECK",
            "check" => "PLANNED_CHECK",
            "section" => "SECTION",
            "report" | "assert_certify" => "ASSERT_CERTIFY",
            _ => "PLANNED_BODY",
        },
        "CertificationReport" => "CERTIFICATION",
        "PlannedCertificationReport" => "ASSERT_CERTIFY",
        "CaseContext" | "CaseAggregateContext" => return None,
        "AggregateCase" | "CaseAggregate" | "AggregateEntry" | "AggregateEvidence" => {
            "CASE_AGGREGATE"
        }
        "AggregateFact" | "AggregateCheckPlan" => "AGGREGATE_CHECK",
        "TestCase" => match method {
            "input" | "result" | "observations" | "spans" => "CASE",
            _ => return None,
        },
        "CheckPlan" => "PLANNED_CHECK",
        "Fact" => match method {
            "slope" | "intercept" | "r_squared" => "NUMERIC_COMPARISON_CHECK",
            "expect_valid" => "EXPECTATION",
            _ => "FACT",
        },
        "CheckResult" => {
            if method == "description" {
                "EAGER_CHECK"
            } else {
                "FACT"
            }
        }
        "Criterion"
        | "CloseTo"
        | "GroupExpectation"
        | "NumericRule"
        | "NumericRange"
        | "MetricConstraint"
        | "CorrelationExpectation"
        | "LinearFitExpectation"
        | "DifferenceExpectation"
        | "RelationExpectation" => "EXPECTATION",
        "PreparedSource" => match method {
            "name" | "values" | "entry" | "iter" | "with_provenance" | "project" | "keys" => {
                "SOURCE_BINDING"
            }
            "numeric" | "lag_self" | "lag_with" => "NUMERIC_CHECK",
            "group_by" => "GROUP_CHECK",
            "compare_with" => "CORRESPONDENCE_CHECK",
            _ => "PREPARED_SOURCE_CHECK",
        },
        "PreparedValues" | "PreparedKeys" => "SOURCE_BINDING",
        "PreparedNumeric" | "NumericSeries" | "OutlierConfig" | "ZeroMadPolicy"
        | "NonNegativeFinite" | "JumpConfig" | "LagRange" | "LagAlignment" | "LagReport" => {
            "NUMERIC_CHECK"
        }
        "IntegrityReport" => "PREPARED_SOURCE_CHECK",
        "GroupedView" | "GroupedNumeric" | "Group" => "GROUP_CHECK",
        "ComparisonBuilder" | "Comparison" | "Join" => "CORRESPONDENCE_CHECK",
        "PairedComparison" => {
            if method == "numeric" {
                "NUMERIC_COMPARISON_CHECK"
            } else {
                "CORRESPONDENCE_CHECK"
            }
        }
        "PreparedNumericComparison" | "Metric" => "NUMERIC_COMPARISON_CHECK",
        "DomainField" => match method {
            "fields" => "RULE",
            "length" | "each" => "COLLECTION_RULE",
            "all_enum_values" | "selected_enum_values" => "ENUM_RULE",
            "equal" | "not_equal" | "less_than" | "less_equal" | "greater_than"
            | "greater_equal" => "RELATION",
            _ => "CONSTRUCTIVE",
        },
        "Selection" => "SELECTION",
        "ScopeConfig" | "Context" | "ContextThreadPool" | "Policy" => "SCOPE",
        "ObservationEngine" => match method {
            "add_channel_sink"
            | "clear_channel_sinks"
            | "register_namespace_channel"
            | "flush_channels" => "ROUTING",
            "span_if_active" => "SPAN",
            _ => "EMISSION",
        },
        "MemorySink"
        | "MemorySinkConfig"
        | "JsonlSink"
        | "JsonlConfig"
        | "ConsoleSink"
        | "ObservationDelivery" => "SINK",
        "SpanGuard" => "SPAN",
        "CapturedRun" if method == "to_spans" => "SPAN_EVIDENCE",
        "CaptureConfig" | "CapturedRun" | "CaptureStatus" => "CAPTURE",
        "ObservationDataset" => {
            if method == "to_spans" {
                "SPAN_EVIDENCE"
            } else if method == "to_typed" {
                "TYPED_EVIDENCE"
            } else {
                "QUERY"
            }
        }
        "ObservationsQuery" => "QUERY",
        "RecordView" => {
            if method == "to_spans" {
                "SPAN_EVIDENCE"
            } else if method == "to_typed" {
                "TYPED_EVIDENCE"
            } else {
                "QUERY"
            }
        }
        "TypedView" | "DecodeError" => "TYPED_EVIDENCE",
        "SpanMeasurements" | "SpanMeasurement" | "SpanIdentity" => "SPAN_EVIDENCE",
        _ => return None,
    })
}

fn trait_node(owner: &str) -> Option<&'static str> {
    Some(match owner {
        "Observation" => "DECLARATION",
        "Sink" => "SINK",
        "Expectation" => "EXPECTATION",
        "SourceProvenance" => "SOURCE_BINDING",
        _ => return None,
    })
}

fn enum_node(owner: &str) -> Option<&'static str> {
    Some(match owner {
        "CheckClass" => "FACT",
        "ViolationClass" | "CountRule" | "NumericRule" | "CorrelationMeasure"
        | "LinearFitMeasure" | "DifferenceMeasure" | "RelationMeasure" => "EXPECTATION",
        "ZeroMadPolicy" | "LagMeasure" => "NUMERIC_CHECK",
        "Policy" => "SCOPE",
        "ObservationLevel" => "DECLARATION",
        _ => return None,
    })
}

fn free_node(name: &str) -> Option<&'static str> {
    Some(match name {
        "certification" => "CERTIFICATION",
        "source" | "collection" => "SOURCE_BINDING",
        "fact" | "try_fact" => "FACT",
        "criterion" | "close_to" => "EXPECTATION",
        "check_count" | "check_unique" | "check_contiguous" | "check_same_keys" => {
            "PREPARED_SOURCE_CHECK"
        }
        "join_by" | "join_presorted_by" => "CORRESPONDENCE_CHECK",
        "linear_metrics" | "pearson" | "spearman" | "correlation" | "linear_fit"
        | "numeric_relation" | "difference_summary" => "NUMERIC_COMPARISON_CHECK",
        "lag_relation_by" => "NUMERIC_CHECK",
        "global" | "observe_if_active" => "EMISSION",
        "start_scope" | "spawn" => "SCOPE",
        "is_normalized_segment" | "validate_tag" | "matches_subtree" | "validate_namespace" => {
            "SELECTION"
        }
        "span_if_active" => "SPAN",
        "capture_observations" => "CAPTURE",
        _ => return None,
    })
}

#[derive(Default)]
struct ClientApi {
    methods: BTreeMap<String, BTreeSet<String>>,
    nodes: BTreeMap<&'static str, BTreeSet<String>>,
}
impl ClientApi {
    fn add_source(&mut self, source: &str) {
        for (owner, method, node) in source_methods(source) {
            self.methods
                .entry(owner)
                .or_default()
                .insert(method.clone());
            self.nodes.entry(node).or_default().insert(method);
        }
        for item in syn::parse_file(source).unwrap().items {
            if let syn::Item::Impl(implementation) = &item {
                if implementation
                    .trait_
                    .as_ref()
                    .is_some_and(|(_, path, _)| path.is_ident("Default"))
                {
                    if let syn::Type::Path(path) = &*implementation.self_ty {
                        let owner = path.path.segments.last().unwrap().ident.to_string();
                        if let Some(node) = method_node(&owner, "default") {
                            self.methods
                                .entry(owner)
                                .or_default()
                                .insert("default".into());
                            self.nodes.entry(node).or_default().insert("default".into());
                        }
                    }
                }
            }
            if let syn::Item::Enum(enumeration) = &item {
                if matches!(enumeration.vis, syn::Visibility::Public(_)) {
                    self.methods
                        .entry(enumeration.ident.to_string())
                        .or_default()
                        .extend(enumeration.variants.iter().map(|v| v.ident.to_string()));
                }
            }
            if let syn::Item::Trait(contract) = &item {
                if let Some(node) = trait_node(&contract.ident.to_string()) {
                    for item in &contract.items {
                        if let syn::TraitItem::Fn(method) = item {
                            self.nodes
                                .entry(node)
                                .or_default()
                                .insert(method.sig.ident.to_string());
                        }
                    }
                }
            }
            if let syn::Item::Fn(function) = item {
                if matches!(function.vis, syn::Visibility::Public(_)) && !hidden(&function.attrs) {
                    let name = function.sig.ident.to_string();
                    if let Some(node) = free_node(&name) {
                        self.nodes.entry(node).or_default().insert(name);
                    }
                }
            }
        }
        for (node, constructors) in [
            (
                "EXPECTATION",
                &["NotEqual", "Equal", "AllValid", "AtMostAnomalies"][..],
            ),
            ("DECLARATION", &["derive", "observation", "tags"][..]),
            ("DOMAIN", &["domain", "derive"][..]),
            ("WHEN", &["when"][..]),
            ("EMISSION", &["observe"][..]),
            ("SPAN", &["observe_span"][..]),
        ] {
            self.nodes
                .entry(node)
                .or_default()
                .extend(constructors.iter().map(|s| (*s).to_owned()));
        }
    }

    fn permits_call(&self, node: &str, call: &str) -> bool {
        let local = self
            .nodes
            .get(node)
            .is_some_and(|names| names.contains(call));
        let shared = match call {
            "fact" | "expect" | "expect_valid" | "AtMostAnomalies" => {
                LANGUAGES[0].nodes.contains(&node)
                    && ["FACT", "EXPECTATION"].iter().any(|owner| {
                        self.nodes
                            .get(owner)
                            .is_some_and(|names| names.contains(call))
                    })
            }
            "executor" | "executor_with_seed" => {
                node == "PLANNED_BODY"
                    && self
                        .nodes
                        .get("CERTIFICATION")
                        .is_some_and(|names| names.contains(call))
            }
            "all_enum_values" | "selected_enum_values" => {
                ["CONSTRUCTIVE", "COLLECTION_RULE"].contains(&node)
                    && self
                        .nodes
                        .get("ENUM_RULE")
                        .is_some_and(|names| names.contains(call))
            }
            "fixed_value" | "selected_values" | "in_range" | "one_of_ranges" => {
                node == "COLLECTION_RULE"
                    && self
                        .nodes
                        .get("CONSTRUCTIVE")
                        .is_some_and(|names| names.contains(call))
            }
            "flush_channels" => {
                node == "EMISSION"
                    && self
                        .nodes
                        .get("ROUTING")
                        .is_some_and(|names| names.contains(call))
            }
            _ => false,
        };
        local || shared
    }

    fn documentation_errors(&self, text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        let sections = sections(text);
        for language in LANGUAGES {
            for node in language.nodes {
                let Some(bodies) = sections.get(node) else {
                    continue;
                };
                let mut fence = None;
                for line in bodies[0].lines() {
                    if let Some(marker) = fence {
                        if line.trim() == marker {
                            fence = None;
                        }
                        continue;
                    }
                    if let Some(marker) = ["~~~", "```"]
                        .into_iter()
                        .find(|m| line.trim().starts_with(m))
                    {
                        fence = Some(marker);
                        continue;
                    }
                    for inline in line.split('`').skip(1).step_by(2) {
                        for path in
                            inline.split(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
                        {
                            if let Some((owner, method)) = path.split_once("::") {
                                if (method_node(owner, method).is_some()
                                    || enum_node(owner).is_some())
                                    && method != "default"
                                    && !self
                                        .methods
                                        .get(owner)
                                        .is_some_and(|methods| methods.contains(method))
                                {
                                    errors.push(format!(
                                        "{node} references unsupported {owner}::{method}"
                                    ));
                                }
                            }
                        }
                        for call in grammar_calls(inline) {
                            if call == "default"
                                || call.chars().next().is_some_and(char::is_uppercase)
                            {
                                continue;
                            }
                            if !self.permits_call(node, &call) {
                                errors.push(format!(
                                    "{node} inline call has no public owning API: {call}"
                                ));
                            }
                        }
                    }
                }
            }
        }
        errors
    }
}

#[test]
fn client_reference_does_not_invent_or_retain_removed_calls() {
    let mut paths = Vec::new();
    rust_sources(&root().join("crates/sun-observations/src"), &mut paths);
    let mut api = ClientApi::default();
    for path in paths {
        api.add_source(&fs::read_to_string(path).unwrap());
    }
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let mut errors = api.documentation_errors(reference(&readme));
    for language in LANGUAGES {
        let text = skill(language.skill);
        for (node, production) in grammar(&text) {
            for call in grammar_calls(&production) {
                if !api.permits_call(&node, &call) {
                    errors.push(format!("{node} grammar has no public owning API: {call}"));
                }
            }
        }
    }
    assert!(errors.is_empty(), "{errors:#?}");
    let mut fixture = ClientApi::default();
    fixture.add_source("impl PreparedSource { pub fn count(&self) {} }");
    let documented = "#### PREPARED_SOURCE_CHECK\n`count()`\n";
    assert!(fixture.documentation_errors(documented).is_empty());
    let mut removed = ClientApi::default();
    removed.add_source("impl PreparedSource { pub fn cardinality(&self) {} }");
    assert!(!removed.documentation_errors(documented).is_empty());
    assert!(!fixture
        .documentation_errors("#### GROUP_CHECK\n`count()`\n")
        .is_empty());
}

fn source_methods(source: &str) -> Vec<(String, String, &'static str)> {
    let mut methods = Vec::new();
    for item in syn::parse_file(source).unwrap().items {
        if let syn::Item::Impl(item) = item {
            if item.trait_.is_some() {
                continue;
            }
            let syn::Type::Path(path) = &*item.self_ty else {
                continue;
            };
            let owner = path.path.segments.last().unwrap().ident.to_string();
            for item in item.items {
                if let syn::ImplItem::Fn(method) = item {
                    if !matches!(method.vis, syn::Visibility::Public(_)) || hidden(&method.attrs) {
                        continue;
                    }
                    let name = method.sig.ident.to_string();
                    if let Some(node) = method_node(&owner, &name) {
                        methods.push((owner.clone(), name, node));
                    }
                }
            }
        }
    }
    methods
}

fn source_contract_items(source: &str) -> Vec<(String, String, &'static str)> {
    let mut items = source_methods(source);
    for item in syn::parse_file(source).unwrap().items {
        match item {
            syn::Item::Fn(function)
                if matches!(function.vis, syn::Visibility::Public(_))
                    && !hidden(&function.attrs) =>
            {
                let name = function.sig.ident.to_string();
                if let Some(node) = free_node(&name) {
                    items.push(("free function".into(), name, node));
                }
            }
            syn::Item::Trait(contract)
                if matches!(contract.vis, syn::Visibility::Public(_))
                    && !hidden(&contract.attrs) =>
            {
                let owner = contract.ident.to_string();
                if let Some(node) = trait_node(&owner) {
                    for item in contract.items {
                        let name = match item {
                            syn::TraitItem::Fn(method) => method.sig.ident.to_string(),
                            syn::TraitItem::Type(associated) => associated.ident.to_string(),
                            syn::TraitItem::Const(constant) => constant.ident.to_string(),
                            _ => continue,
                        };
                        items.push((owner.clone(), name, node));
                    }
                }
            }
            syn::Item::Enum(enumeration)
                if matches!(enumeration.vis, syn::Visibility::Public(_))
                    && !hidden(&enumeration.attrs) =>
            {
                let owner = enumeration.ident.to_string();
                if let Some(node) = enum_node(&owner) {
                    items.extend(
                        enumeration
                            .variants
                            .into_iter()
                            .map(|variant| (owner.clone(), variant.ident.to_string(), node)),
                    );
                }
            }
            _ => {}
        }
    }
    items
}

fn source_parameterized_calls(source: &str) -> Vec<(String, String, &'static str, usize)> {
    let mut calls = Vec::new();
    for item in syn::parse_file(source).unwrap().items {
        match item {
            syn::Item::Impl(implementation) if implementation.trait_.is_none() => {
                let syn::Type::Path(path) = &*implementation.self_ty else {
                    continue;
                };
                let owner = path.path.segments.last().unwrap().ident.to_string();
                for item in implementation.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };
                    if !matches!(method.vis, syn::Visibility::Public(_)) || hidden(&method.attrs) {
                        continue;
                    }
                    if !method
                        .sig
                        .inputs
                        .iter()
                        .any(|arg| matches!(arg, syn::FnArg::Typed(_)))
                    {
                        continue;
                    }
                    let name = method.sig.ident.to_string();
                    if let Some(node) = method_node(&owner, &name) {
                        calls.push((
                            owner.clone(),
                            name,
                            node,
                            method
                                .sig
                                .inputs
                                .iter()
                                .filter(|arg| matches!(arg, syn::FnArg::Typed(_)))
                                .count(),
                        ));
                    }
                }
            }
            syn::Item::Fn(function)
                if matches!(function.vis, syn::Visibility::Public(_))
                    && !hidden(&function.attrs)
                    && !function.sig.inputs.is_empty() =>
            {
                let name = function.sig.ident.to_string();
                if let Some(node) = free_node(&name) {
                    calls.push((
                        "free function".into(),
                        name,
                        node,
                        function.sig.inputs.len(),
                    ));
                }
            }
            syn::Item::Trait(contract)
                if matches!(contract.vis, syn::Visibility::Public(_))
                    && !hidden(&contract.attrs) =>
            {
                let owner = contract.ident.to_string();
                let Some(node) = trait_node(&owner) else {
                    continue;
                };
                for item in contract.items {
                    let syn::TraitItem::Fn(method) = item else {
                        continue;
                    };
                    if method
                        .sig
                        .inputs
                        .iter()
                        .any(|arg| matches!(arg, syn::FnArg::Typed(_)))
                    {
                        calls.push((
                            owner.clone(),
                            method.sig.ident.to_string(),
                            node,
                            method
                                .sig
                                .inputs
                                .iter()
                                .filter(|arg| matches!(arg, syn::FnArg::Typed(_)))
                                .count(),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    calls
}

fn source_coverage_errors(source: &str, reference: &str) -> Vec<String> {
    let sections = sections(reference);
    source_contract_items(source)
        .into_iter()
        .filter_map(|(owner, name, node)| {
            let body = sections
                .get(node)
                .and_then(|s| s.first())
                .copied()
                .unwrap_or("");
            (!identifiers(body).contains(name.as_str()))
                .then(|| format!("{node} must document {owner}::{name}"))
        })
        .collect()
}

fn rust_sources(directory: &Path, result: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, result);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && !path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .ends_with("_certification")
            && path.file_stem().unwrap() != "test_support"
        {
            result.push(path);
        }
    }
}

#[test]
fn reference_tracks_public_methods_at_their_semantic_owner() {
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    let reference = reference(&readme);
    let mut paths = Vec::new();
    rust_sources(&root().join("crates/sun-observations/src"), &mut paths);
    let mut errors = Vec::new();
    let mut count = 0;
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        count += source_contract_items(&source).len();
        errors.extend(source_coverage_errors(&source, reference));
    }
    assert!(
        count > 150,
        "public client surface was not scanned: {count}"
    );
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn portable_skills_track_public_methods_at_their_semantic_owner() {
    let mut paths = Vec::new();
    rust_sources(&root().join("crates/sun-observations/src"), &mut paths);
    let mut errors = Vec::new();
    let mut count = 0;
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        for (owner, name, node) in source_contract_items(&source) {
            let Some(language) = LANGUAGES
                .iter()
                .find(|language| language.nodes.contains(&node))
            else {
                continue;
            };
            count += 1;
            let skill = skill(language.skill);
            let skill_sections = sections(&skill);
            let body = skill_sections
                .get(node)
                .and_then(|sections| sections.first())
                .copied()
                .unwrap_or("");
            if !identifiers(body).contains(name.as_str()) {
                errors.push(format!(
                    "{} {node} must document {owner}::{name}",
                    language.skill
                ));
            }
        }
    }
    assert!(
        count > 150,
        "public client surface was not scanned: {count}"
    );
    assert!(errors.is_empty(), "{errors:#?}");
}

fn documented_call_arities(text: &str, name: &str) -> Vec<usize> {
    let mut result = Vec::new();
    for (start, _) in text.match_indices(name) {
        let before = text[..start].chars().next_back();
        let after_name = start + name.len();
        let after = text[after_name..].chars().next();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            || after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let rest = &text[after_name..];
        let Some(open_rel) = rest.find('(') else {
            continue;
        };
        let prefix = &rest[..open_rel];
        if prefix.contains('\n') || (!prefix.trim().is_empty() && !prefix.trim().starts_with("::<"))
        {
            continue;
        }
        let args = &rest[open_rel + 1..];
        let has_receiver =
            args.trim_start().starts_with("&self,") || args.trim_start().starts_with("&self)");
        let mut paren = 1usize;
        let mut bracket = 0usize;
        let mut brace = 0usize;
        let mut angle = 0usize;
        let mut commas = 0usize;
        let mut non_ws = false;
        for ch in args.chars() {
            match ch {
                '(' => paren += 1,
                ')' => {
                    paren -= 1;
                    if paren == 0 {
                        let count = if non_ws { commas + 1 } else { 0 };
                        result.push(count - usize::from(has_receiver));
                        break;
                    }
                }
                '[' => bracket += 1,
                ']' => bracket = bracket.saturating_sub(1),
                '{' => brace += 1,
                '}' => brace = brace.saturating_sub(1),
                '<' => angle += 1,
                '>' => angle = angle.saturating_sub(1),
                ',' if paren == 1 && bracket == 0 && brace == 0 && angle == 0 => commas += 1,
                _ => {}
            }
            if !ch.is_whitespace() && !(ch == ')' && paren == 0) {
                non_ws = true;
            }
        }
    }
    result
}

#[test]
fn implementer_receivers_do_not_count_as_client_call_arguments() {
    assert_eq!(
        documented_call_arities("fn assess(&self, observed: &T)", "assess"),
        vec![1]
    );
    assert_eq!(
        documented_call_arities("fn describe(&self)", "describe"),
        vec![0]
    );
    assert_eq!(
        documented_call_arities(
            "fn describe(&self, index: usize, out: &mut dyn fmt::Write)",
            "describe"
        ),
        vec![2]
    );
    assert_eq!(
        documented_call_arities("policy.assess(&observed)", "assess"),
        vec![1]
    );
}

#[test]
fn portable_skills_show_call_syntax_for_parameterized_public_methods() {
    let mut paths = Vec::new();
    rust_sources(&root().join("crates/sun-observations/src"), &mut paths);
    let mut errors = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        for (owner, name, node, arity) in source_parameterized_calls(&source) {
            let Some(language) = LANGUAGES
                .iter()
                .find(|language| language.nodes.contains(&node))
            else {
                continue;
            };
            let skill = skill(language.skill);
            let skill_sections = sections(&skill);
            let body = skill_sections
                .get(node)
                .and_then(|sections| sections.first())
                .copied()
                .unwrap_or("");
            let arities = documented_call_arities(body, &name);
            if !arities.contains(&arity) {
                errors.push(format!(
                    "{} {node} must show {owner}::{name} with {arity} parameter(s); documented arities: {arities:?}",
                    language.skill
                ));
            }
        }
    }
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn source_coverage_rejects_renames_additions_and_misplaced_method_documentation() {
    let source = "impl PreparedSource { pub fn count(&self) {} pub fn unique(&self) {} }";
    let valid = "#### PREPARED_SOURCE_CHECK
count() unique()
#### SOURCE_BINDING
values()
";
    assert!(source_coverage_errors(source, valid).is_empty());
    for changed in [
        source.replace("count", "cardinality"),
        source.replace("unique", "distinct"),
    ] {
        assert!(!source_coverage_errors(&changed, valid).is_empty());
    }
    assert!(!source_coverage_errors(
        source,
        "#### PREPARED_SOURCE_CHECK
unique()
#### SOURCE_BINDING
count()
"
    )
    .is_empty());

    let free = "pub fn check_count() {}";
    assert!(source_coverage_errors(
        free,
        "#### PREPARED_SOURCE_CHECK
check_count()
"
    )
    .is_empty());
    assert!(!source_coverage_errors(
        free,
        "#### PREPARED_SOURCE_CHECK
"
    )
    .is_empty());

    let expectation =
        "pub trait Expectation<T> { type Violation; fn assess(&self); fn describe(&self); fn class(&self); }";
    assert!(source_coverage_errors(
        expectation,
        "#### EXPECTATION
Violation assess describe class
"
    )
    .is_empty());
    assert!(!source_coverage_errors(
        expectation,
        "#### EXPECTATION
assess describe class
"
    )
    .is_empty());

    let observation = "pub trait Observation { const NAMESPACE: &'static str; fn message(&self); }";
    assert!(source_coverage_errors(
        observation,
        "#### DECLARATION
NAMESPACE message
"
    )
    .is_empty());
    assert!(!source_coverage_errors(
        observation,
        "#### DECLARATION
message
"
    )
    .is_empty());

    let count_rules = "pub enum CountRule { Exact(usize), Minimum(usize), NonEmpty }";
    assert!(source_coverage_errors(
        count_rules,
        "#### EXPECTATION
Exact Minimum NonEmpty
"
    )
    .is_empty());
    assert!(!source_coverage_errors(
        count_rules,
        "#### EXPECTATION
Exact NonEmpty
"
    )
    .is_empty());
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
            "Adversarially validate the proof boundary itself",
            "evidence set and proof obligations themselves",
            "false PASS and false FAIL",
            "domain identity",
            "causality",
            "statistical expectation",
            "evaluation error",
            "independent oracle",
            "metamorphic",
            "null/control scenario",
            "wall-clock thresholds",
            "### Per-case and aggregate checks",
            "not only profiling",
            "### Profiling",
            "known regression risk",
            "runner.check(...)",
            "runner.check_aggregate(...)",
            "once after exploration",
            "PreparedSource",
            "evidence set that actually contains comparable samples",
            "nanosecond hardware resolution",
            "overhead subtraction",
            "fails closed",
        ],
        &[
            "DomainRegion",
            "CaseDomainCoordinateSlot",
            "diagnostic_bytes",
        ],
    );
}

fn numeric_authoring_errors(text: &str) -> Vec<String> {
    let mut errors = Vec::new();
    for (parameter, constructors) in [
        (
            "CORRELATION_EXPECTATION",
            &[
                "pearson_at_least(min)",
                "spearman_at_least(min)",
                "spearman_at_most(max)",
                "and_pearson_bounds(min,max)",
                "and_spearman_bounds(min,max)",
            ][..],
        ),
        (
            "LINEAR_FIT_EXPECTATION",
            &[
                "slope_between(min,max)",
                "intercept_between(min,max)",
                "r_squared_at_least(min)",
                "residual_mean_absolute_at_most(max)",
                "residual_max_absolute_at_most(max)",
                "and_slope_between(min,max)",
                "and_intercept_between(min,max)",
                "and_r_squared_at_least(min)",
                "and_residual_mean_absolute_at_most(max)",
                "and_residual_max_absolute_at_most(max)",
            ][..],
        ),
        (
            "DIFFERENCE_EXPECTATION",
            &[
                "mean_absolute_at_most(max)",
                "p95_absolute_at_most(max)",
                "max_absolute_at_most(max)",
                "and_mean_absolute_at_most(max)",
                "and_p95_absolute_at_most(max)",
                "and_max_absolute_at_most(max)",
            ][..],
        ),
        (
            "RELATION_EXPECTATION",
            &[
                "spearman_at_least(min)",
                "spearman_at_most(max)",
                "bound(measure,min,max)",
            ][..],
        ),
    ] {
        let row = text
            .lines()
            .find(|line| line.starts_with(&format!("| {parameter} |")))
            .unwrap_or("");
        for constructor in constructors {
            if !row.contains(constructor) {
                errors.push(format!("{parameter} missing {constructor}"));
            }
        }
    }
    for path in ["numerical.rs", "relation.rs"] {
        let source = fs::read_to_string(
            root()
                .join("crates/sun-observations/src/certifications")
                .join(path),
        )
        .unwrap();
        for item in syn::parse_file(&source).unwrap().items {
            if let syn::Item::Enum(item) = item {
                let owner = item.ident.to_string();
                if ![
                    "CorrelationMeasure",
                    "LinearFitMeasure",
                    "DifferenceMeasure",
                    "RelationMeasure",
                ]
                .contains(&owner.as_str())
                {
                    continue;
                }
                let row = text
                    .lines()
                    .find(|line| line.starts_with(&format!("- {owner}::")))
                    .unwrap_or("");
                let names = identifiers(row);
                for variant in item.variants {
                    let name = variant.ident.to_string();
                    if !names.contains(name.as_str()) {
                        errors.push(format!("missing {owner}::{name}"));
                    }
                }
                let family = owner.replace("Measure", "Expectation");
                if !text.contains(&format!("{family}::with_bound(measure, min, max)?")) {
                    errors.push(format!("missing {family} generic bounds"));
                }
            }
        }
    }
    let productions = grammar(text);
    let comparison = productions
        .get("NUMERIC_COMPARISON_CHECK")
        .map(String::as_str)
        .unwrap_or("");
    for (method, expectation) in [
        ("correlation", "CORRELATION"),
        ("linear_fit", "LINEAR_FIT"),
        ("difference", "DIFFERENCE"),
        ("relation", "RELATION"),
    ] {
        if !comparison.contains(&format!("{method}().expect({expectation}_EXPECTATION)")) {
            errors.push(format!("missing {method} expectation mapping"));
        }
    }
    errors
}

const AUTHORING_FIXTURES: &[(&str, &str, &[&str])] = &[
    (
        "certifications-api",
        "impl Expectation<u32> for Nonzero",
        &[
            "type Violation = &'static str;",
            "fn assess(&self, observed: &u32) -> Result<Vec<Self::Violation>, AnalysisError>",
            "fn describe(&self) -> String",
        ],
    ),
    (
        "certifications-api",
        "impl SourceProvenance for RowCoordinates",
        &["fn describe(&self, index: usize, out: &mut dyn std::fmt::Write) -> std::fmt::Result"],
    ),
    (
        "observations-api",
        "impl Sink for AuditSink",
        &[
            "fn write(&self, record: &ObservationRecord) -> io::Result<()>",
            "fn flush(&self) -> io::Result<()>",
            "fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()>",
            "fn wants_render_text(&self) -> bool",
            "fn write_delivery(&self, delivery: &ObservationDelivery) -> io::Result<()>",
        ],
    ),
    (
        "observations-api",
        "impl Observation for Accepted",
        &[
            "const NAMESPACE: &'static str",
            "const LEVEL: ObservationLevel",
            "const TAGS: &'static [&'static str]",
            "fn message(&self) -> Option<&str>",
            "fn render_text(&self) -> Option<String>",
        ],
    ),
    (
        "domain-api",
        "fn tuple_payload_positions()",
        &[
            "enum Item { Number(i32), Pair(i32, bool) }",
            "Item::Number { _0.in_range(1..=3) }",
            "Item::Pair { _0.in_range(1..=3), _1.fixed_value(true) }",
        ],
    ),
];

fn authoring_fixture_errors(name: &str, text: &str) -> Vec<String> {
    let mut errors = vec![];
    for &(_, anchor, signatures) in AUTHORING_FIXTURES
        .iter()
        .filter(|(owner, _, _)| *owner == name)
    {
        let Some(example) = blocks(text).find(|block| block.contains(anchor)) else {
            errors.push(format!("missing executable {anchor}"));
            continue;
        };
        let compact: String = example.split_whitespace().collect();
        for signature in signatures {
            let expected: String = signature.split_whitespace().collect();
            if !compact.contains(&expected) {
                errors.push(format!("{anchor} missing {signature}"));
            }
        }
    }
    errors
}

#[test]
fn portable_implementer_and_tuple_fixtures_reject_missing_or_mistyped_contracts() {
    for &(name, anchor, signatures) in AUTHORING_FIXTURES {
        let original = skill(name);
        assert!(authoring_fixture_errors(name, &original).is_empty());
        let example = blocks(&original)
            .find(|block| block.contains(anchor))
            .unwrap();
        assert!(!authoring_fixture_errors(name, &original.replace(&example, "")).is_empty());
        for signature in signatures {
            let altered = original.replace(signature, "omitted contract");
            assert_ne!(original, altered);
            assert!(
                !authoring_fixture_errors(name, &altered).is_empty(),
                "{signature}"
            );
        }
    }
}

fn invalid_extension_examples() -> Vec<String> {
    [
        (
            "certifications-api",
            "impl Expectation<u32>",
            "-> Result<Vec<Self::Violation>, AnalysisError>",
            "-> Vec<Self::Violation>",
        ),
        (
            "certifications-api",
            "impl SourceProvenance",
            "out: &mut dyn std::fmt::Write",
            "out: &mut String",
        ),
        (
            "certifications-api",
            "impl SourceProvenance",
            "-> std::fmt::Result",
            "-> String",
        ),
        (
            "observations-api",
            "impl Sink",
            "record: &ObservationRecord",
            "record: Arc<ObservationRecord>",
        ),
        (
            "observations-api",
            "impl Sink",
            "record: Arc<ObservationRecord>",
            "record: &Arc<ObservationRecord>",
        ),
        (
            "observations-api",
            "impl Sink",
            "delivery: &ObservationDelivery",
            "delivery: ObservationDelivery",
        ),
        (
            "observations-api",
            "impl Observation for Accepted",
            "-> Option<&str>",
            "-> Option<String>",
        ),
        (
            "observations-api",
            "impl Observation for Accepted",
            "-> Option<String>",
            "-> Option<&str>",
        ),
    ]
    .into_iter()
    .map(|(name, anchor, valid, invalid)| {
        let example = blocks(&skill(name))
            .find(|block| block.contains(anchor))
            .unwrap();
        assert!(example.contains(valid));
        example.replace(valid, invalid)
    })
    .collect()
}

fn authoring_errors(name: &str, text: &str) -> Vec<String> {
    let (required, forbidden): (&[&str], &[&str]) = match name {
        "observations-api" => (
            &[
                "EMISSION := observe!(VALUE)",
                "SPAN := span = observe_span!(START_PAYLOAD)",
                "CaptureConfig::new()",
                ".activate_namespace(NS)?",
                ".activate_tags(TAGS)?",
                "to_spans::<PAYLOAD>()?",
                "SpanMeasurement",
                "CHANNEL_NAME",
                "CUSTOM_SINK_IMPL",
                "Sink: Send + Sync + 'static",
                "only write(&self, record: &ObservationRecord)",
                "Manual Observation: serde::Serialize + 'static",
                "require const NAMESPACE: &'static str",
                "both default to None",
                "const LEVEL: ObservationLevel = ObservationLevel::INFO",
                "const TAGS: &'static [&'static str] = &[]",
                "ConsoleSink::new(writer)",
                "ConsoleSink::is_started()",
                "ConsoleSink::last_error()",
                "not an Arc<dyn Sink>",
                "MEMORY_CONFIG means MemorySinkConfig::new(MAX_ITEMS, COMPACT_FRACTION)?",
                "JSONL_CONFIG means JsonlConfig::new()",
                "OPERATION means arbitrary real production code",
                "finish() records completion/end timing and carries no additional payload",
                "SpanIdentity",
                "proc_ns",
                "SpanError",
                "start_index/end_index",
                "certifications-api",
                "nanosecond",
                "exactly one",
            ],
            &[
                "| observe_if_active",
                "| engine.observe_if_active",
                "| span_if_active",
                "| engine.span_if_active",
                "or observe_if_active",
                "or lazy span_if_active",
                "RETENTION",
                "PATH, CONFIG",
                "sink(CHANNEL,",
                "finish_with",
            ],
        ),
        "certifications-api" => (
            &[
                "source(NAME, &VALUE)",
                "collection(NAME, &VALUES)",
                "check_fact_expect",
                "type Violation must implement std::fmt::Debug",
                "T has no additional trait bounds",
                "empty means pass",
                "Optional class(&self) -> ViolationClass defaults to Hard",
                "Criterion::greater_than(bound)",
                "Criterion::at_least(bound)",
                "Criterion::less_than(bound)",
                "Criterion::at_most(bound)",
                "Criterion::between(min, max)",
                "with_bound(measure, min, max)",
                "case.spans::<T>()",
                "spans.source(name)",
                "summary() is optional",
                "outliers(config)",
                "CASE_AGGREGATE := cases = runner.case_aggregate()",
                "AGGREGATE_CHECK :=",
                "runner.check_aggregate(fact!(AGGREGATE_EXPRESSION)",
                "once after exploration",
                "for each successful positive case",
                "cases.observations::<T>()?",
                "cases.spans::<T>()?",
                "OutlierConfig::default()",
                "OutlierConfig::new(THRESHOLD, ZeroMadPolicy::ExactMedian",
                "ZeroMadPolicy::absolute_tolerance(ABSOLUTE_TOLERANCE)?",
                "JumpConfig::new(ABSOLUTE_THRESHOLD, RELATIVE_THRESHOLD, DENOMINATOR_FLOOR)?",
                "thresholds are Option<f64>",
                "LagRange::new(LAG_START, LAG_END)?",
                "LAG_START/LAG_END/OFFSET are i64",
                "SEQUENCE_START/SEQUENCE_END are u64",
                "rows.contiguous(KEY, SEQUENCE_START, SEQUENCE_END)",
                ".linear() returns Fact<LinearMetrics>",
                ".linear_fit() returns Fact<LinearFitMetrics>",
                "LagMeasure::Pearson | LagMeasure::Spearman",
                "ComparisonBuilder::on_presorted(left_key,right_key)",
            ],
            &[
                "c.source(",
                "c.collection(",
                "Direct alternatives:",
                "Direct one-off alternatives:",
                "One-off analysis:",
                "Criterion::BOUND(...)",
                "AllValid",
                "outliers(CONFIG)",
                "jumps(CONFIG)",
                "PROJECT, CONFIG",
                "ZERO_MAD_POLICY",
                "LAG_CONFIG",
                "aggregate_check(",
                "case_local(",
            ],
        ),
        "domain-api" => (
            &[
                "domain!(i32 { in_range(1..=3) })",
                "`_0`, `_1`, ... in declaration order",
                "equal(operand)",
                "less_than(operand)",
                "greater_equal(operand)",
            ],
            &[],
        ),
        _ => return Vec::new(),
    };
    let mut errors: Vec<_> = required
        .iter()
        .filter(|marker| !text.to_lowercase().contains(&marker.to_lowercase()))
        .map(|marker| format!("missing {marker}"))
        .chain(
            forbidden
                .iter()
                .filter(|marker| text.contains(**marker))
                .map(|marker| format!("ambiguous {marker}")),
        )
        .collect();
    errors.extend(authoring_fixture_errors(name, text));
    if name == "certifications-api" {
        errors.extend(numeric_authoring_errors(text));
    }
    if name == "observations-api" {
        let productions = grammar(text);
        for node in ["CAPTURE", "SCOPE"] {
            let production = productions.get(node).map(String::as_str).unwrap_or("");
            for builder in [
                ".activate_all()",
                ".activate_namespace(NS)?",
                ".activate_tags(TAGS)?",
                ".exclude_namespace(NS)?",
                ".exclude_tags(TAGS)?",
                ".selection(SELECTION)",
            ] {
                if !production.contains(builder) {
                    errors.push(format!("{node} missing {builder}"));
                }
            }
        }
    }
    errors
}

#[test]
fn portable_authoring_is_canonical_parameter_complete_and_profiles_through_generic_analysis() {
    for name in SKILLS {
        let text = skill(name);
        let errors = authoring_errors(name, &text);
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
    let original = skill("certifications-api");
    for altered in [
        original
            .replace("Criterion::greater_than(bound)", "Criterion::greater_than")
            .replace("Criterion::greater_than(BOUND)", "Criterion::greater_than"),
        original
            .replace("Criterion::less_than(bound)", "Criterion::less_than()")
            .replace("Criterion::less_than(BOUND)", "Criterion::less_than()"),
        original.replace("source(NAME, &VALUE)", "c.source(NAME, &VALUE)"),
        original.replace("ZeroMadPolicy::ExactMedian", "CONFIG"),
        original.replace(
            "JumpConfig::new(ABSOLUTE_THRESHOLD, RELATIVE_THRESHOLD, DENOMINATOR_FLOOR)?",
            "CONFIG",
        ),
        original.replace("runner.case_aggregate()", "runner.case()"),
        original.replace("runner.check_aggregate(", "runner.aggregate_check("),
        original.replace("check_fact_expect", "check"),
        original.replace("and_slope_between(min,max)", "and_slope_between"),
        original.replace("and_pearson_bounds(min,max)", "and_pearson_bounds"),
        original.replace("AbsoluteSpearman", "OmittedMeasure"),
        original.replace(
            "SEQUENCE_START/SEQUENCE_END are u64",
            "SEQUENCE_START/SEQUENCE_END are i64",
        ),
        original.replace(
            ".linear_fit() returns Fact<LinearFitMetrics>",
            ".linear_fit() returns Fact<LinearMetrics>",
        ),
        original.replace(
            "correlation().expect(CORRELATION_EXPECTATION)",
            "correlation().expect(EXPECTATION)",
        ),
    ] {
        assert!(!authoring_errors("certifications-api", &altered).is_empty());
    }
    let domain = skill("domain-api");
    assert!(!authoring_errors(
        "domain-api",
        &domain.replace("domain!(i32 { in_range(1..=3) })", "scalar root")
    )
    .is_empty());
    let original = skill("observations-api");
    for method in [
        "ConsoleSink::new(writer)",
        "ConsoleSink::is_started()",
        "ConsoleSink::last_error()",
    ] {
        assert!(!authoring_errors(
            "observations-api",
            &original
                .replace(method, "other sink method")
                .replace("ConsoleSink::new(WRITER)", "other sink constructor")
        )
        .is_empty());
    }
    let altered = original.replace(
        "EMISSION := observe!(VALUE)",
        "EMISSION := observe!(VALUE) | observe_if_active(|| VALUE)",
    );
    assert!(!authoring_errors("observations-api", &altered).is_empty());
    for altered in [
        original.replace("START_PAYLOAD", "PAYLOAD"),
        original.replace("CHANNEL_NAME", "CHANNEL"),
        original.replace("MEMORY_CONFIG", "RETENTION"),
        original.replace("CUSTOM_SINK_IMPL", "CUSTOM_SINK"),
        original.replace(".activate_namespace(NS)?", ""),
        original.replace(".activate_tags(TAGS)?", ""),
    ] {
        assert!(!authoring_errors("observations-api", &altered).is_empty());
    }
}

fn readme_example_source(example: RustExample, index: usize) -> (String, usize) {
    let body = example
        .body
        .lines()
        .map(|line| {
            let content = line.trim_start();
            let indent = &line[..line.len() - content.len()];
            if content == "#" {
                String::new()
            } else if let Some(hidden) = content.strip_prefix("# ") {
                format!("{indent}{hidden}")
            } else if let Some(escaped) = content.strip_prefix("##") {
                format!("{indent}#{escaped}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut tests = vec![];
    let file = syn::parse_file(&body).ok().filter(|file| {
        !file
            .items
            .iter()
            .any(|item| matches!(item, syn::Item::Macro(item) if item.ident.is_none()))
    });
    let content = if let Some(file) = file {
        for item in &file.items {
            if let syn::Item::Fn(function) = item {
                if function
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("test"))
                {
                    tests.push(function.sig.ident.to_string());
                }
            }
        }
        assert_eq!(body.matches("#[test]").count(), tests.len());
        let mut content = body.replace("#[test]", "#[test]\n#[ignore]");
        if tests.is_empty() {
            let call = if file
                .items
                .iter()
                .any(|item| matches!(item, syn::Item::Fn(f) if f.sig.ident == "main"))
            {
                "super::ExampleOutcome::assert_success(main());"
            } else {
                ""
            };
            content.push_str(&format!("\n#[test] #[ignore] fn execute() {{ {call} }}"));
            tests.push("execute".to_owned());
        }
        content
    } else {
        tests.push("execute".to_owned());
        format!("#[test] #[ignore] fn execute() {{ super::ExampleOutcome::assert_success((|| {{\n{body}\n}})()); }}")
    };
    let mut source = format!("mod readme_{index} {{\n{content}\n");
    for test in &tests {
        let action = if example.execution == ExampleExecution::CompileOnly {
            format!("let _ = {test};")
        } else {
            format!("super::isolated_example(\"readme_{index}::{test}\");")
        };
        source.push_str(&format!("#[test] fn isolated_{test}() {{ {action} }}\n"));
    }
    source.push_str("}\n");
    (source, tests.len())
}

#[test]
fn canonical_skill_and_readme_rust_blocks_compile_and_execute() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let mut source = String::from(
        r#"#![allow(dead_code,unused_imports)]
trait ExampleOutcome { fn assert_success(self); }
impl ExampleOutcome for () { fn assert_success(self) {} }
impl<E: std::fmt::Debug> ExampleOutcome for Result<(), E> {
    fn assert_success(self) { self.unwrap(); }
}
fn isolated_example(name: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--ignored", "--test-threads=1"]).output().unwrap();
    assert!(output.status.success(), "{name}: {}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
}
#[test] fn failed_results_fail() {
    assert!(std::panic::catch_unwind(|| ExampleOutcome::assert_success(Err::<(), _>("negative"))).is_err());
}
#[test] #[ignore] fn rejected_child() { panic!("child failure control"); }
#[test] fn child_failures_fail() {
    assert!(std::panic::catch_unwind(|| isolated_example("rejected_child")).is_err());
}
#[test] fn missing_child_fails() {
    assert!(std::panic::catch_unwind(|| isolated_example("absent_child")).is_err());
}
"#,
    );
    let mut index = 0usize;
    for name in SKILLS {
        for block in blocks(&skill(name)) {
            assert!(block.contains("#[test]"), "{name} example must execute");
            source.push_str(&format!("mod snippet_{index} {{\n{block}\n}}\n"));
            index += 1;
        }
    }
    assert!(index >= LANGUAGES.iter().map(|language| language.nodes.len()).sum());
    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    for (example_index, example) in rust_examples(&readme).enumerate() {
        let (module, tests) = readme_example_source(example, example_index);
        source.push_str(&module);
        index += tests;
    }
    let (compile_only, tests) = readme_example_source(
        RustExample {
            body: "let fail: fn() = || panic!(\"no_run example executed\"); fail();".to_owned(),
            execution: ExampleExecution::CompileOnly,
        },
        usize::MAX,
    );
    source.push_str(&compile_only);
    index += tests + 3;
    let invalid_examples = invalid_extension_examples();
    for (invalid_index, example) in invalid_examples.iter().enumerate() {
        let compile_failure = format!("```compile_fail,E0053\n{example}\n```\n");
        source.push_str(&format!(
            "#[doc = {compile_failure:?}] pub struct InvalidExtension{invalid_index};\n"
        ));
    }
    let path = root()
        .join("crates/sun-observations")
        .to_string_lossy()
        .replace(char::from(92), "/");
    let manifest = format!(
        r#"[package]
name="client-documentation-contracts"
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
    let run = |doc_only| {
        let mut command = Command::new(env!("CARGO"));
        command
            .args(["test", "--offline", "--manifest-path"])
            .arg(dir.path().join("Cargo.toml"))
            .arg("--target-dir")
            .arg(
                std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .parent()
                    .unwrap(),
            );
        command.arg(if doc_only { "--doc" } else { "--lib" });
        command.args(["--", "--test-threads=1"]).output().unwrap()
    };
    let invalid_count = invalid_examples.len();
    let output = run(false);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("{index} passed; 0 failed")),
        "{stdout}"
    );
    let output = run(true);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout)
        .contains(&format!("{invalid_count} passed; 0 failed")));
    fs::write(
        dir.path().join("src/lib.rs"),
        r#"#![doc = "
```rust
use sun_observations::certifications::*;
let mut runner = certification(\"unsupported binding\").executor(|n: &u64| Ok::<_, ()>(*n));
runner.source(\"unsupported\", &1u64);
```
"]
"#,
    )
    .unwrap();
    let broken = run(true);
    assert!(!broken.status.success());
    assert!(String::from_utf8_lossy(&broken.stdout).contains("source"));
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
