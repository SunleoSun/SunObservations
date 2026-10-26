---
name: certifications-api
description: Write client-side semantic certifications with sun_observations::certifications. Use when building facts, expectations, planned production execution, prepared sources, comparisons, statistics, anomaly checks, lag checks, or final assertions.
---

# Certifications client language

Use `sun_observations::certifications::*`. Read certifications-methodology for proof design, domain-api for generated inputs, and observations-api for capture/query/decode.

## DSL

Authoring structure, not literal Rust; uppercase words are parameters or nodes defined here. Rust closures supply projections, predicates, executors, and section bodies.

~~~text
CERTIFICATION :=
    c = certification(NAME)
    [ SOURCE_BINDING ]*
    [ EAGER_CHECK | SECTION ]*
    ( ASSERT_CERTIFY
    | runner = c.executor(EXECUTOR); PLANNED_BODY
    | runner = c.executor_with_seed(SEEDED_EXECUTOR); PLANNED_BODY )
SOURCE_BINDING :=
    value = source(NAME, &VALUE) | rows = collection(NAME, &VALUES)
EAGER_CHECK :=
c.check_fact_expect(NAME, OBSERVED, EXPECTATION)
| c.check(NAME, FACT.expect(EXPECTATION))
| c.check(NAME, PREPARED_SOURCE_CHECK | NUMERIC_CHECK | GROUP_CHECK
| CORRESPONDENCE_CHECK | NUMERIC_COMPARISON_CHECK)
SECTION :=
    c.section(NAME, |s| { [ EAGER_CHECK | SECTION ]* })
  | runner.section(NAME, |s| { [ PLANNED_CHECK | SECTION ]* })
PLANNED_BODY :=
    [ runner.data_source(INPUTS) ] [ runner.domain(DOMAIN) ]
    [ runner.cases_budget(N) ] [ runner.failure_budget(N) ]
    [ runner.seed(N) ] [ runner.report_mode(BOOL) ]
    [ runner.require_pairwise_coverage() ]
    CASE [ PLANNED_CHECK | SECTION ]* ASSERT_CERTIFY
CASE := case = runner.case()
PLANNED_CHECK :=
    runner.check(fact!(CASE_EXPRESSION).expect(EXPECTATION | fact!(CASE_EXPECTATION))
                 [ .description(TEXT) ])
  | runner.check(EAGER_CHECK_RESULT)
FACT := fact(SOURCE, VALUE) | try_fact(SOURCE, RESULT) | fact!(EXPRESSION)
EXPECTATION :=
    VALUE | NotEqual(VALUE) | Criterion::at_least(BOUND) | criterion(TEXT, CLASS, PREDICATE)
  | close_to(VALUE) | CountRule::NonEmpty | NumericRule::Finite
  | AtMostAnomalies(N) | LagExpectation { best_lag: OFFSET }
  | GroupExpectation::new(MIN_COUNT, MIN_MEAN, MAX_MEAN)
  | CorrelationExpectation::with_bound(MEASURE, MIN, MAX)
  | LinearFitExpectation::with_bound(MEASURE, MIN, MAX)
  | DifferenceExpectation::with_bound(MEASURE, MIN, MAX)
  | RelationExpectation::with_bound(MEASURE, MIN, MAX)
PREPARED_SOURCE_CHECK :=
    rows.count().expect(EXPECTATION) | value.single_map(PROJECT).expect(EXPECTATION)
  | rows.matching_count(PREDICATE).expect(EXPECTATION)
  | rows.unique(KEY).expect_valid() | rows.contiguous(KEY, START, END).expect_valid()
NUMERIC_CHECK :=
    rows.numeric(PROJECT).expect(NUMERIC_RULE)
  | rows.numeric(PROJECT).(summary() | robust_summary() | quantile(Q)).expect(EXPECTATION)
  | rows.numeric(PROJECT).(outliers(CONFIG) | jumps(CONFIG)).expect(AtMostAnomalies(N))
  | rows.(lag_self(PROJECT, CONFIG) | lag_with(&OTHER, LEFT, RIGHT, CONFIG))
        .expect(LagExpectation { best_lag: OFFSET })
GROUP_CHECK :=
    fact(NAME, rows.group_by(KEY).numeric(PROJECT)
        .(summaries() | summaries_with_robust())).expect(GROUP_EXPECTATION)
CORRESPONDENCE_CHECK :=
    paired = left.compare_with(&right).on(LEFT_KEY, RIGHT_KEY)
    paired.paired_comparison_fact().expect_valid()
  | paired.equal(LEFT, RIGHT, TOP).expect_valid()
  | paired.check_pairs(PREDICATE, TOP).expect_valid()
NUMERIC_COMPARISON_CHECK :=
    paired.numeric(LEFT, RIGHT)
        .(correlation() | linear_fit() | difference() | relation()).expect(EXPECTATION)
ASSERT_CERTIFY := c.assert_certify() | runner.assert_certify()
~~~

## CERTIFICATION

Start one proof with certification(NAME). Register prerequisites and prepare borrowed evidence before optionally consuming c with executor(EXECUTOR) or executor_with_seed(SEEDED_EXECUTOR). Do not create two independent proofs for those phases.

~~~rust
use sun_observations::certifications::*;
#[test]
fn export_contract() {
    let exported = [10u64, 11, 12];
    let mut c = certification("export has stable identity");
    c.check("ids", collection("export", &exported).unique(|id| *id).expect_valid());
    c.assert_certify();
}
~~~

## SOURCE_BINDING

Use source(name, &value) for one value and collection(name, &slice) for many. Names are diagnostics, not join keys. Bindings borrow evidence, not the certification; they survive the transition to planned execution.

Inspect with name(), values(), entry(index), iter(); attach custom provenance with with_provenance(&provider). A custom provider implements SourceProvenance::describe(index, out) and writes bounded diagnostic coordinates only. Cache a typed projection with project(project), then use .values() or .source().

~~~rust
use sun_observations::certifications::*;
#[test]
fn prepared_before_execution() {
    let expected = ["alpha", "beta"];
    let c = certification("normalized catalog");
    let catalog = collection("catalog", &expected);
    let mut runner = c.executor(|s: &String| Ok::<_, ()>(s.trim().to_lowercase()));
    runner.data_source([" ALPHA ".to_owned()]);
    runner.check(catalog.count().expect(2usize));
    let case = runner.case();
    runner.check(fact!(case.result().clone()).expect("alpha".to_owned()));
    runner.assert_certify();
}
~~~

## EAGER_CHECK

Use c.check_fact_expect(name, observed, expectation) for a direct observed/expected check. Use c.check(name, fact.expect(expectation)) when the Fact needs its own source/details/description, and c.check(name, prepared_check) for prepared analysis. These are authoring roles, not competing acceptance engines.

~~~rust
use sun_observations::certifications::*;
#[test]
fn bounded_page() {
 let page = [21, 22, 23];
 let mut c = certification("page capacity");
 c.check_fact_expect("fits", page.len() <= 100, true);
 c.check("not empty", fact!(page.len()).expect(CountRule::NonEmpty));
 c.assert_certify();
}
~~~

## SECTION

Call section(name, |s| { ... }) on either phase; nest sections for meaningful semantic stages. Use the closure's s for registrations. Planned paths are captured at registration, not fact construction. An eager result registered with s.check(result) belongs to that planned section too. Paths remain separate from descriptions in reports; a section does not execute the planned cases.

~~~rust
use sun_observations::certifications::*;
#[test]
fn nested_stages() {
    let mut runner = certification("squares stay nonnegative")
        .executor(|n: &i32| Ok::<_, ()>(i64::from(*n).pow(2)));
    runner.data_source([-3, 0, 4]);
    let case = runner.case();
    runner.section("transform", |s| {
        s.section("range", |s| {
            s.check(fact!(*case.result()).expect(Criterion::at_least(0i64).unwrap()));
        });
    });
    runner.assert_certify();
}
~~~

## PLANNED_BODY

The executor runs real production: |input| -> Result<Output, Error>, or |input, seed| for seeded execution. CapturedRun<Result<Output, Error>> also works; capture belongs to observations-api.

- data_source(inputs): meaningful concrete cases, executed first; no DomainInput required.
- domain(domain): one generated input contract; syntax belongs to domain-api.
- cases_budget(n): total exploration capacity including real-case prefill; real inputs run first even beyond this bound. A registered Domain needs remaining generated capacity.
- failure_budget(n): positive bound on retained representative case failures; stops exploration when full.
- seed(n): reproducible framework/Domain seed; seeded executors additionally receive a deterministic per-case seed.
- report_mode(bool): coverage and registered-check summary.
- require_pairwise_coverage(): require feasible enum values/pairs within the existing budget, not extra generation capacity.

Positive executor errors, accepted generated-negative cases, and executor panics fail. Expected negative rejection passes and skips positive-only checks. Missing inputs, invalid Domain/configuration, incomplete capture, or undefined required analysis fail closed.

~~~rust
use sun_observations::certifications::*;
#[test]
fn magnitude_cases() {
    let mut runner = certification("absolute magnitude")
        .executor(|n: &i32| Ok::<_, ()>(i64::from(*n).abs()));
    runner.data_source([i32::MIN, 0, i32::MAX]);
    runner.seed(71).failure_budget(3).report_mode(true);
    let case = runner.case();
    runner.check(fact!(*case.result() >= 0).expect(true));
    runner.assert_certify();
}
~~~

## CASE

Bind let case = runner.case() once. Inside fact!(...), use case.input(), case.result(), case.observations::<T>() for captured payloads, or case.spans::<T>() for paired profiling evidence. result() is the successful positive output, not a Result. The binding does not borrow the runner. All deferred checks reevaluate for each positive case.

~~~rust
use sun_observations::certifications::*;
#[test]
fn sort_preserves_length() {
    let mut runner = certification("sorted batch cardinality").executor(|rows: &Vec<u64>| {
        let mut sorted = rows.clone();
        sorted.sort_unstable();
        Ok::<_, ()>(sorted)
    });
    runner.data_source([vec![3, 1, 2]]);
    let case = runner.case();
    runner.check(fact!(case.result().len()).expect(fact!(case.input().len())));
    runner.assert_certify();
}
~~~

## PLANNED_CHECK

runner.check(fact!(expression).expect(expectation)) accepts static expectations or fact!(case-dependent expectation). .description(text) gives semantic diagnostic meaning. runner.check(eager_result) registers a one-time prerequisite, not a per-case check. Independent checks continue after check evaluation errors/panics.

~~~rust
use sun_observations::certifications::*;
#[test]
fn rounded_capacity() {
    let mut runner = certification("capacity rounds upward to a multiple of eight")
        .executor(|n: &usize| Ok::<_, ()>(n.div_ceil(8) * 8));
    runner.data_source([0, 1, 8, 9]);
    let case = runner.case();
    runner.check(fact!(*case.result() % 8).expect(0).description("aligned"));
    runner.check(fact!(*case.result() >= *case.input()).expect(true));
    runner.check(fact!(*case.result() - *case.input() < 8).expect(true));
    runner.assert_certify();
}
~~~

## FACT

Use fact!(expression) for ordinary expressions. Use fact(source_name, value) for a prepared report/value with an explicit diagnostic source, and try_fact(source_name, result) for fallible evidence so analysis errors remain EvaluationError. Add .details(text) for context. Apply .expect(expectation) before registering; facts never assert.

~~~rust
use sun_observations::certifications::*;
#[test]
fn parse_persisted_count() {
    let mut c = certification("persisted count is usable");
    c.check("count", try_fact("header count", "24".parse::<u64>())
        .details("after reload").expect(24));
    c.assert_certify();
}
~~~

## EXPECTATION

Choose by evidence semantics:

- VALUE: exact PartialEq equality; NotEqual(value): inequality.
- Criterion::greater_than(bound), Criterion::at_least(bound), Criterion::less_than(bound), Criterion::at_most(bound), Criterion::between(min, max): ordered scalar bounds; constructors validate and return Result.
- criterion(text, ViolationClass::Hard | ViolationClass::Statistical, predicate): custom typed predicate. Implement Expectation<T> only for reusable policy: define type Violation: Debug, assess(observed), describe(), and optionally class(); return AnalysisError when evaluation is impossible.
- close_to(expected)?.absolute_tolerance(abs)?.relative_tolerance(rel)?: validated f64 comparison.
- CountRule::NonEmpty: nonempty counts; use VALUE for exact counts or Criterion::at_least(bound) for a minimum.
- .expect_valid(): integrity, correspondence, and pair-predicate reports only, not arbitrary object validation.
- NumericRule::Finite, NumericRule::Positive, NumericRule::NonNegative, NumericRule::between(min, max)?: use rows.numeric(project).expect(rule), which returns a ready check, not a Fact.
- AtMostAnomalies(n): outlier or jump count. LagExpectation { best_lag: offset }: keyed lag acceptance.
- GroupExpectation::new(min_count, min_mean, max_mean)?: each group's count and mean.
- CorrelationExpectation::with_bound(measure, min, max)?, LinearFitExpectation::with_bound(measure, min, max)?, DifferenceExpectation::with_bound(measure, min, max)?, RelationExpectation::with_bound(measure, min, max)?: statistical acceptance. Each min/max is Option<f64>; chain .bound(measure, min, max)? only to add another metric constraint.

Measures: CorrelationMeasure::{Pearson, Spearman, AbsolutePearson, AbsoluteSpearman}; LinearFitMeasure::{Slope, Intercept, RSquared, ResidualMeanAbsolute, ResidualMaxAbsolute}; DifferenceMeasure::{MeanAbsolute, P95Absolute, MaxAbsolute}; RelationMeasure combines these, naming direct errors MeanAbsoluteDifference, P95AbsoluteDifference, MaxAbsoluteDifference. Bounds validate finite/order/domain constraints. A required undefined metric is EvaluationError.

~~~rust
use sun_observations::certifications::*;
#[test]
fn tolerance_and_identity() {
    let mut c = certification("normalized reading");
    c.check("unit magnitude", fact("gain", 1.0f64)
        .expect(close_to(1.0).unwrap().absolute_tolerance(1e-6).unwrap()));
    c.check("assigned id", fact("id", 42u64).expect(NotEqual(0)));
    c.assert_certify();
}
~~~

## PREPARED_SOURCE_CHECK

Use count().expect(count_rule), matching_count(predicate).expect(count_rule), single_map(project).expect(expectation), unique(key).expect_valid(), contiguous(key, start, end).expect_valid(). single_map requires exactly one value. contiguous uses inclusive u64 endpoints. unique/contiguous end with expect_valid(); names never substitute for typed keys.

~~~rust
use sun_observations::certifications::*;
#[test]
fn exported_sequence() {
    let ids = [42u64, 43, 44];
    let mut c = certification("complete export range");
    let rows = collection("export", &ids);
    c.check("unique", rows.unique(|id| *id).expect_valid());
    c.check("range", rows.contiguous(|id| *id, 42, 44).expect_valid());
    c.assert_certify();
}
~~~

## NUMERIC_CHECK
Profiling bridge: obtain SpanMeasurements from observations-api and use spans.source(name) as an ordinary PreparedSource. Project duration_ns() as f64 for generic numeric analysis or duration() for exact typed bounds; then reuse group_by, compare_with, relation, outliers, and jumps. summary() is optional; preserve execution identity when generated inputs repeat.

Use numeric(project) once when reused. expect(numeric_rule) registers integrity directly; summary(), robust_summary(), quantile(q), outliers(config), jumps(config) take ordinary .expect(expectation). q is in [0,1]. For custom summary acceptance use criterion over the report.

- OutlierConfig::new(threshold, zero_mad_policy)?: finite nonnegative modified-z threshold; default 3.5 and ExactMedian. Use ZeroMadPolicy::absolute_tolerance(value)?, using original units when MAD is zero.
- JumpConfig::new(Some(abs), Some(rel), floor): at least one finite nonnegative threshold and positive finite denominator floor; either strict threshold flags a transition.
- lag_self(project, config) or lag_with(&other, left, right, config): projections return (i64 key, f64 value). LagConfig { range: LagRange::new(start,end)?, measure: LagMeasure::Pearson | Spearman }. Positive lag d pairs left k with right k+d. Inspect zero_lag() and alignment.has_interior_gaps() when interpreting boundary loss.

~~~rust
use sun_observations::certifications::*;
#[test]
fn stable_latency() {
    let milliseconds = [10.0, 10.0, 10.0, 10.0];
    let mut c = certification("latency integrity");
    let numeric = collection("latency ms", &milliseconds).numeric(|v| *v);
    c.check("finite", numeric.expect(NumericRule::Finite));
    c.check("outliers", numeric.outliers(OutlierConfig::default()).expect(AtMostAnomalies(0)));
    c.assert_certify();
}
~~~

## GROUP_CHECK

Group by semantic KEY, prepare numeric PROJECT, then wrap summaries() or summaries_with_robust() with fact(name, report) before GroupExpectation. Certify required group identities separately; summaries cannot detect an entirely absent group. For custom checks use groups.iter(), then group.len(), group.is_empty(), group.values(), group.iter(), group.numeric(project). The last returns a NumericSeries: wrap its fallible analyses with try_fact(name, result) before .expect(expectation).

~~~rust
use sun_observations::certifications::*;
#[test]
fn per_region_latency() {
    let samples = [("east", 10.0), ("east", 12.0), ("west", 9.0), ("west", 11.0)];
    let mut c = certification("regional latency");
    let rows = collection("requests", &samples);
    let groups = rows.group_by(|r| r.0);
    c.check("regions", fact("region count", groups.iter().count()).expect(2usize));
    c.check("latency", fact("regional ms", groups.numeric(|r| r.1).summaries())
        .expect(GroupExpectation::new(2, 8.0, 13.0).unwrap()));
    c.assert_certify();
}
~~~

## CORRESPONDENCE_CHECK

Bind paired = left.compare_with(&right).on(left_key, right_key). Both keys must have the same ordered type. Always certify paired_comparison_fact().expect_valid() before interpreting matched subsets. equal(left,right,top) and check_pairs(predicate,top) fail closed on incomplete correspondence; top bounds examples (capped at 32).

Inspect comparison(), matched_pairs(), name() for diagnostics.

~~~rust
use sun_observations::certifications::*;
#[test]
fn persistence_preserves_rows() {
    let sent = [(1u64, "alpha"), (2, "beta")];
    let loaded = [(2u64, "beta"), (1, "alpha")];
    let mut c = certification("persistence by identity");
    let paired = collection("sent", &sent).compare_with(&collection("loaded", &loaded))
        .on(|r| r.0, |r| r.0);
    c.check("complete", paired.paired_comparison_fact().expect_valid());
    c.check("labels", paired.equal(|r| r.1, |r| r.1, 5).expect_valid());
    c.assert_certify();
}
~~~

## NUMERIC_COMPARISON_CHECK

Use paired.numeric(left_value,right_value) once. Chain correlation(), linear_fit(), difference(), relation() with the matching expectation family. difference is right minus left, not regression residual; high correlation alone does not prove agreement. Inspect largest(top) for bounded diagnostic examples.

~~~rust
use sun_observations::certifications::*;
#[test]
fn calibrated_sensor() {
    let reference = [(1u64, 1.0), (2, 2.0), (3, 3.0)];
    let measured = [(3u64, 3.001), (1, 1.001), (2, 2.001)];
    let mut c = certification("sensor calibration");
    let paired = collection("reference", &reference).compare_with(&collection("sensor", &measured))
        .on(|r| r.0, |r| r.0);
    c.check("identity", paired.paired_comparison_fact().expect_valid());
    let numeric = paired.numeric(|r| r.1, |r| r.1);
    c.check("agreement", numeric.difference().expect(DifferenceExpectation::with_bound(DifferenceMeasure::MaxAbsolute, None, Some(0.01)).unwrap()));
    c.check("association", numeric.correlation().expect(CorrelationExpectation::with_bound(CorrelationMeasure::Pearson, Some(0.99), None).unwrap()));
    c.assert_certify();
}
~~~

## ASSERT_CERTIFY

End normal proofs with c.assert_certify() or runner.assert_certify(). It aggregates hard/statistical violations and evaluation errors. No checks is an eager failure; no executed cases is a planned failure. report() is for programmatic diagnostics, not a replacement for assertion in normal tests.

~~~rust
use sun_observations::certifications::*;
#[test]
fn no_forbidden_ids() {
    let ids = [1u64, 2, 3];
    let mut c = certification("ids are assigned");
    c.check("unassigned", collection("ids", &ids).matching_count(|id| *id == 0).expect(0usize));
    c.assert_certify();
}
~~~
