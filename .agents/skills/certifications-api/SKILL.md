---
name: certifications-api
description: Write client-side semantic certifications with sun_observations::certifications. Use when building facts, expectations, planned production execution, prepared sources, comparisons, statistics, anomaly checks, lag checks, or final assertions.
---

# Certifications client API

Use sun_observations::certifications::*. Read certifications-methodology whenever creating, changing, reviewing, or migrating a certification. Read domain-api for generated input contracts and observations-api for capture or typed Observation evidence.

This skill documents the API a client repository should normally call. Framework planner coordinates, internal execution adapters, and solver representation types are intentionally omitted.

## Certification workflow grammar

Structure only; not copyable Rust. The grammar separates eager checks from per-execution checks because their registration APIs differ.

~~~text
SOURCE_BINDING :=
    source NAME &VALUE
  | collection NAME &VALUES

FACT :=
    fact SOURCE OBSERVED
  | try_fact SOURCE RESULT
  | prepared API call documented below that returns Fact<...>

EXPECTATION :=
    exact VALUE
  | NotEqual VALUE
  | Criterion ...
  | criterion ...
  | close_to ...
  | CountRule ...
  | AtMostAnomalies
  | LagExpectation
  | GroupExpectation
  | CorrelationExpectation
  | LinearFitExpectation
  | DifferenceExpectation
  | RelationExpectation

CHECK_RESULT :=
    FACT expect EXPECTATION
    [ description TEXT ]

EAGER_CHECK :=
    check NAME CHECK_RESULT
  | check_fact_expect NAME OBSERVED EXPECTATION

SECTION :=
    section NAME {
        [ EAGER_CHECK | SECTION ]*
    }

EAGER_CERTIFICATION :=
    certification NAME
    [ SOURCE_BINDING ]*
    [ EAGER_CHECK | SECTION ]*
    assert_certify

PLANNED_CERTIFICATION :=
    runner = certification NAME
    ( executor FUNCTION | executor_with_seed FUNCTION )
    [ data_source INPUTS ]
    [ domain DOMAIN ]
    [ cases_budget N ]
    [ failure_budget N ]
    [ seed N ]
    [ report_mode BOOL ]
    [ require_pairwise_coverage ]

    let case = runner.case()

    [ check PLANNED_CHECK ]*
    assert_certify

PLANNED_CHECK :=
    PLANNED_FACT expect EXPECTATION
    [ description TEXT ]

PLANNED_FACT :=
    fact!(expression using case.input())
  | fact!(expression using case.result())
  | fact!(expression using case.observations<T>())
~~~

Normal certification tests end with assert_certify(); it is the proof boundary and aggregates independent failures.

section(...) belongs to eager Certification. PlannedCertification has no section method. If sectioned eager prerequisites are needed together with planned execution, add them to the base Certification before calling executor(...).

## Eager certification

Start with certification(name).

- check(name, CheckResult) registers one eager check.
- check_fact_expect(name, observed, expectation) is shorthand for check(name, fact(name, observed).expect(expectation)); the expectation need not be equality.
- section(name, |c| ...) groups eager checks under a semantic path and can nest.
- source(name, &value) and collection(name, &values) create reusable prepared evidence.
- assert_certify() is the final test assertion and aggregates independent failures.

Minimal example:

~~~rust
use sun_observations::certifications::*;

fn certify_output() {
    let returned = 3usize;
    let rows = [1u64, 2, 3];

    let mut c = certification("canonical output");
    let result = c.source("result", &returned);
    let data = c.collection("rows", &rows);

    c.section("shape", |s| {
        s.check("result", result.single_map(|v| *v).expect(3usize));
        s.check("unique", data.unique(|v| *v).expect_valid());
    });
    c.check(
        "forbidden future",
        data.matching_count(|v| *v > 3).expect(0usize),
    );
    c.assert_certify();
}
~~~

## Facts, checks, expectations

### Fact to check flow

- fact(source, observed) creates eager typed evidence.
- try_fact(source, Result) turns analysis failure into EvaluationError instead of aborting unrelated proof work.
- Fact::details(text) adds bounded diagnostic context.
- Fact::expect(expectation) turns a Fact into a CheckResult.
- CheckResult::description(text) overrides the human check description.
- fact!(expression) names an eager expression from source text. Expressions using case.input(), case.result(), or case.observations() become deferred per-case facts.
- Fact<LinearFitMetrics>::slope(), intercept(), and r_squared() project required scalar metrics; an undefined required metric becomes EvaluationError.

expect_valid() is not generic object validation. It is shorthand for AllValid on three framework report types:
- IntegrityReport: valid when issues is empty; issues can describe count, duplicate, missing/unexpected key, or numeric-integrity failures.
- &Comparison: valid when correspondence is complete and unique, with no unmatched or duplicate keys.
- PairCheck: valid when the pair predicate has zero failures.

### Expectation DSL and where it applies

Most expectations are passed to FACT.expect(EXPECTATION):

~~~rust
use sun_observations::certifications::*;

fn expectation_examples() {
    let rows = [1u64, 2, 3];
    let version = 1u64;
    let ratio = 1.0f64;
    let batch = 4usize;
    let mut c = certification("expectation examples");

    c.check("exact", fact("rows", rows.len()).expect(3usize));
    c.check("different", fact("version", version).expect(NotEqual(0u64)));
    c.check(
        "minimum",
        fact("rows", rows.len()).expect(Criterion::at_least(1usize).unwrap()),
    );
    c.check(
        "ratio",
        fact("ratio", ratio).expect(
            close_to(1.0)
                .unwrap()
                .absolute_tolerance(1e-6)
                .unwrap(),
        ),
    );
    c.check(
        "domain predicate",
        fact("batch", batch).expect(criterion(
            "batch is even",
            ViolationClass::Hard,
            |value: &usize| value % 2 == 0,
        )),
    );
    c.assert_certify();
}
~~~

Use the expectation that matches the Fact type:
- plain PartialEq value -> exact expectation by passing the value itself.
- NotEqual(value) -> exact inequality.
- Criterion::greater_than / Criterion::at_least / Criterion::less_than / Criterion::at_most / Criterion::between -> ordered scalar Fact; constructors validate comparability and return Result.
- criterion(description, ViolationClass, predicate) -> custom hard/statistical predicate.
- close_to(expected) -> f64 Fact; optional absolute_tolerance and relative_tolerance; all builders validate and return Result.
- CountRule::Exact / Minimum / NonEmpty -> Fact<usize>.
- AtMostAnomalies(n) -> Fact<OutlierReport>.
- LagExpectation { best_lag } -> Fact<LagReport>.
- GroupExpectation::new(...) -> Fact<GroupedSummaryReport<_>>; constructor returns Result.
- specialized CorrelationExpectation, LinearFitExpectation, DifferenceExpectation, and RelationExpectation apply to the corresponding prepared numeric Facts described below.

NumericRule is slightly different: PreparedNumeric::expect(NumericRule) performs numeric integrity analysis and returns CheckResult directly, so register it with c.check(name, numeric.expect(rule)). NumericRule::between validates its bounds.

Expectation<T> is the extension trait for custom reusable expectations. Implement assess(observed), describe(), and optionally class(); inability to evaluate should return AnalysisError rather than inventing pass/fail.

## Planned production execution

Convert an eager Certification into a planned runner with:

- executor(|input| production(input))
- executor_with_seed(|input, seed| production(input, seed))

The executor must run the real production path. Result<Output, Error> works directly. CapturedRun<Result<Output, Error>> is supported through the evidence adapter; read observations-api when using capture.

### Inputs and controls

- data_source(inputs) registers concrete real inputs. They execute first through the same executor. DomainInput is not required for data-source-only execution.
- domain(domain!(...)) adds generated input exploration; read domain-api.
- cases_budget(n) sets generated exploration capacity.
- failure_budget(n) bounds retained representative case failures and stops further generated execution when full; zero is invalid.
- seed(n) sets the deterministic certification and Domain seed. It is useful even with ordinary executor.
- executor_with_seed additionally exposes the deterministic derived seed for each materialized case to production fixture logic.
- report_mode(bool) requests a compact coverage/check summary.
- require_pairwise_coverage() makes feasible enum values and pairwise enum combinations mandatory within the existing case budget. It does not increase the budget.

Real data_source inputs can prefill Domain coverage. Positive executor errors, accepted generated-negative inputs, and executor panics fail. Expected negative rejection passes without evaluating positive-only checks.

### Deferred case checks

Bind once when checks depend on execution:

~~~text
let case = runner.case()

case.input()          materialized input
case.result()         successful positive output
case.observations<T>() typed captured evidence; capture only
~~~

In normal client proofs, use only input(), result(), and observations<T>() from the case binding; execution adaptation is framework-owned.

Register deferred checks with planned.check(...). CheckPlan::description(text) adds human diagnostic context.

Minimal example:

~~~rust
use sun_observations::certifications::*;

#[derive(Clone)]
struct Input { count: usize }

fn execute(input: &Input) -> Result<usize, &'static str> {
    if input.count == 0 { Err("zero") } else { Ok(input.count * 2) }
}

fn certify_execution() {
    let mut c = certification("count doubles").executor(execute);
    c.data_source([Input { count: 2 }, Input { count: 4 }]);

    let case = c.case();
    c.check(
        fact!(*case.result())
            .expect(fact!(case.input().count * 2))
            .description("production result follows input count"),
    );

    c.assert_certify();
}
~~~

When the contract is family-wide, add a Domain instead of manually enumerating arbitrary cases. Domain syntax belongs to domain-api.

## Prepared evidence workflow

Prepared evidence projects once and keeps semantic source identity. Names are diagnostics, not join keys.

~~~text
PREPARED_SOURCE
  -> inspect: name | values | entry | iter
  -> annotate: with_provenance
  -> FACT: count | single_map | matching_count | unique | contiguous | lag_self | lag_with
  -> PreparedValues: project
  -> PreparedNumeric: numeric
  -> PreparedKeys: keys
  -> GroupedView: group_by
  -> ComparisonBuilder: compare_with

PreparedValues -> values | source -> PREPARED_SOURCE

PreparedNumeric
  -> values | series
  -> FACT: summary | robust_summary | quantile | outliers | jumps
  -> CHECK_RESULT: expect NumericRule

PreparedKeys -> indices | compare_with PreparedKeys -> PairedComparison
GroupedView -> iter -> Group
GroupedView -> numeric -> GroupedNumeric
GroupedNumeric -> summaries | summaries_with_robust -> GroupedSummaryReport
~~~

The arrows show what each call produces; follow the resulting type instead of treating the method list as unrelated operations.

### PreparedSource

Create with source(name, &value), collection(name, &slice), or the corresponding Certification methods.

Inspection and provenance:
- name(), values(), entry(index), iter()
- with_provenance(&SourceProvenance)

Direct Facts:
- count()
- single_map(project); exactly one value is required, otherwise the Fact carries EvaluationError.
- matching_count(predicate)
- unique(key) and contiguous(key, start, end) -> Fact<IntegrityReport<_>>; normally use expect_valid().
- lag_self(project, config) and lag_with(&other, left_project, right_project, config) -> Fact<LagReport>.

Derived views:
- project(project) -> PreparedValues.
- numeric(project) -> PreparedNumeric.
- keys(key) -> PreparedKeys.
- group_by(key) -> GroupedView.
- compare_with(&other) -> ComparisonBuilder.

PreparedKeys caches a typed sorted key projection:
- indices() inspects cached key/index pairs.
- compare_with(&other) -> PairedComparison directly when both sides already use the same key type.
### PreparedValues and PreparedNumeric

PreparedValues owns one projected vector:
- values() borrows the projection.
- source() re-enters the PreparedSource workflow without recomputing it.

PreparedNumeric owns one f64 projection:
- values() and series() expose prepared numbers.
- summary(), robust_summary(), quantile(q), outliers(config), and jumps(config) return Facts.
- expect(NumericRule) performs numeric integrity analysis and returns CheckResult directly.

Register numeric integrity directly with c.check(name, numeric.expect(rule)). For anomaly acceptance, use numeric.outliers(config).expect(AtMostAnomalies(n)).

### Grouped evidence

PreparedSource::group_by(key) creates GroupedView.

GroupedView:
- iter()
- numeric(project)

Group:
- len()
- is_empty()
- values()
- iter()
- numeric(project)

GroupedNumeric:
- summaries()
- summaries_with_robust()

GroupedNumeric summaries return GroupedSummaryReport rather than Fact. Wrap that report with fact(...) before applying GroupExpectation.

## Typed correspondence and pair analysis

PreparedSource::compare_with(right) creates ComparisonBuilder. on(left_key, right_key) creates canonical typed correspondence; on_presorted(...) is only for already-sorted keys and fails closed when that promise is false.

~~~text
PreparedSource compare_with PreparedSource
  -> on | on_presorted
  -> PairedComparison

PairedComparison
  -> paired_comparison_fact -> FACT<&Comparison> -> expect_valid
  -> equal | check_pairs -> FACT<PairCheck> -> expect_valid
  -> numeric -> PreparedNumericComparison

PreparedNumericComparison
  -> correlation -> FACT<CorrelationMetrics>
  -> linear_fit -> FACT<LinearFitMetrics>
  -> relation -> FACT<RelationMetrics>
  -> difference -> FACT<DifferenceSummary>
  -> linear -> FACT<LinearMetrics>
  -> pearson | spearman -> FACT<Metric>
  -> largest -> FACT<Vec<PairEvidence>>
~~~

PairedComparison:
- name()
- comparison()
- matched_pairs()
- paired_comparison_fact()
- equal(left_value, right_value, top_examples)
- check_pairs(predicate, top_examples)
- numeric(left_value, right_value)

Always certify correspondence completeness before interpreting matched subsets when exact correspondence is required.

PreparedNumericComparison:
- values()
- correlation()
- linear_fit()
- relation()
- difference() — right minus left
- linear()
- pearson()
- spearman()
- largest(top)

These operations reuse the same paired projection and cached numeric analyses. correlation(), linear_fit(), relation(), and difference() return Facts consumed by CorrelationExpectation, LinearFitExpectation, RelationExpectation, and DifferenceExpectation respectively. linear(), pearson(), spearman(), and largest(top) expose narrower Facts when the proof needs those specific results. Typical chain: paired.numeric(...).correlation().expect(CorrelationExpectation::pearson_at_least(0.95).unwrap()).

## Statistical expectation builders

CorrelationExpectation:
- pearson_at_least(min)
- spearman_at_least(min)
- spearman_at_most(max)
- with_bound(measure, min, max)
- and_pearson_bounds(min, max)
- and_spearman_bounds(min, max)
- bound(measure, min, max)

CorrelationMeasure:
Pearson, Spearman, AbsolutePearson, AbsoluteSpearman.

LinearFitExpectation:
- slope_between(min, max)
- intercept_between(min, max)
- r_squared_at_least(min)
- residual_mean_absolute_at_most(max)
- residual_max_absolute_at_most(max)
- with_bound(measure, min, max)
- and_slope_between(min, max)
- and_intercept_between(min, max)
- and_r_squared_at_least(min)
- and_residual_mean_absolute_at_most(max)
- and_residual_max_absolute_at_most(max)
- bound(measure, min, max)

LinearFitMeasure:
Slope, Intercept, RSquared, ResidualMeanAbsolute, ResidualMaxAbsolute.

DifferenceExpectation:
- mean_absolute_at_most(max)
- p95_absolute_at_most(max)
- max_absolute_at_most(max)
- with_bound(measure, min, max)
- and_mean_absolute_at_most(max)
- and_p95_absolute_at_most(max)
- and_max_absolute_at_most(max)
- bound(measure, min, max)

DifferenceMeasure:
MeanAbsolute, P95Absolute, MaxAbsolute.

RelationExpectation:
- spearman_at_least(min)
- spearman_at_most(max)
- with_bound(measure, min, max)
- bound(measure, min, max)

RelationMeasure:
Pearson, Spearman, AbsolutePearson, AbsoluteSpearman, Slope, Intercept, RSquared, MeanAbsoluteDifference, P95AbsoluteDifference, MaxAbsoluteDifference, ResidualMeanAbsolute, ResidualMaxAbsolute.

All numeric bound constructors validate finite/order/domain constraints before evidence evaluation.

## Anomaly, jump, and lag API

OutlierConfig::new(threshold, zero_mad_policy) validates a finite nonnegative threshold. Default is 3.5 with ZeroMadPolicy::ExactMedian. ZeroMadPolicy::absolute_tolerance(value) uses original numeric units.

JumpConfig::new(absolute_threshold, relative_threshold, denominator_floor) requires at least one threshold, finite nonnegative thresholds, and a positive finite floor. Either strict threshold can flag a transition.

LagRange::new(start, end) validates the inclusive range and candidate bound. LagConfig selects Pearson or Spearman. Positive lag d pairs left key k with right key k+d. LagReport::zero_lag() finds the zero-lag candidate; LagAlignment::has_interior_gaps() distinguishes missing interior keys from expected boundary loss.

## Direct analysis

Use prepared APIs when evidence is reused; direct functions remain available for small independent analysis:

- NumericSeries::new(iterator), iter(), finite_count(), summary(), quantile(q), robust_summary(), integrity(rule), outliers(config), jumps(config)
- check_count(actual, CountRule)
- check_unique(values, key)
- check_contiguous(values, key, start, end)
- check_same_keys(left, right, left_key, right_key)
- join_by(left, right, left_key, right_key)
- join_presorted_by(left, right, left_key, right_key)

join_by and join_presorted_by return Join; use comparison(), matched_pairs(), and require_complete() to inspect or require correspondence before downstream analysis.
- linear_metrics(pairs)
- pearson(pairs)
- spearman(pairs)
- correlation(cloneable_pairs)
- linear_fit(cloneable_pairs)
- numeric_relation(cloneable_pairs)
- difference_summary(pairs)
- lag_relation_by(left, right, left_project, right_project, config)

Direct analysis returns data or typed errors, never assertions. Convert analysis into facts/expectations and finish the certification with assert_certify.