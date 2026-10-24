---
name: certifications
description: Build semantic proofs with sun_observations::certifications. Use when certifying production execution over real/generated typed inputs or checking ordinary/captured evidence, expectations, comparisons, statistics, anomalies, lag and bounded reports.
---

# Semantic certification methodology and API

Use this skill as domain guidance within the user's requested scope; explicit task instructions take precedence if they conflict with workflow guidance here. Do not use it solely for Observation emission, routing, sinks, spans, or capture mechanics without a semantic proof; use the `sun-observations` skill. When a task spans production evidence and semantic proof, use both.

## Methodology + Evidence plan before implementation

**Certify semantic behavior, not implementation shape.** Scope proofs to the narrowest semantic owner, normally `<owner>_certification.rs`; cross-owner names describe transfer/parity. A Rust test is the execution mechanism.

1. Plan contract, canonical outcome, required/absent events, Boundary / Decision / Diagnostic evidence, typed keys, causality, statistical expectations, negative/control scenarios, independent oracle/metamorphic evidence and bounded failure diagnostics. Prefer deterministic fixtures; concurrent proofs check membership/counts/uniqueness/outcomes, not incidental order.
2. Evidence priority: canonical returned result/persisted artifact → promised effect/state → typed production Observation → independent reference/oracle → statistical/anomaly evidence. Never duplicate authoritative returned facts with instrumentation.
3. State what must happen and must never happen. Hard invariants are logically required; statistical/anomaly thresholds are deterministic scenario-owned acceptance, not warnings or production Observation properties. Verify negative paths beyond is_err(): owning rejection boundary/category, forbidden downstream events/artifacts/effects absent, explanatory evidence and independent healthy paths.
4. Trace input → producer boundary → consumer input → decision → canonical result/effect. Join by typed domain identity (tuples/dedicated IDs, including execution when keys repeat), never insertion order or string field names. Prove source/output/consumer time/row, horizon, decision/entry offsets: quality alone cannot prove temporal correspondence. Assert known offsets exactly; lag diagnoses unknown stale/off-by-one/future relations.
5. Global scores hide broken folds/targets/directions/episodes/models: group evidence. After exact invariants, add correlation, monotonicity, slope/intercept, residuals, robust outliers, jumps, lag, distribution/trend checks; not only for ML. Math/optimized paths need a simple independent oracle, not an algorithm copy. Metamorphic controls: partition invariance, shuffled relation destruction, direction inversion, known scaling.
6. Prefer production evidence to test-only visibility. Fixtures/generators/simple oracles/mocks at genuine typed boundaries/properties/compile scaffolding may use cfg(test); production getters/counters/callbacks/alternate branches solely for inspection may not. Evidence roles: Boundary transfer first, Decision branching next, Diagnostic only for a concrete consumer. Design/emission belongs to the sun-observations skill.
7. Collect independent failures in a named hierarchy; do not unwrap failed acceptance or stop at the first criterion. Unavailable dependent analysis is EvaluationError; unrelated checks continue. Reports localize contract/path/source/expected/observed/counts/bounded examples without raw-log archaeology. Direct Rust assertions are exceptions for framework/bootstrap self-tests or low-level compile/process mechanisms where self-dependence obscures the mechanism.

## Core proof API

`use sun_observations::certifications::*`. Ordinary evidence needs no Serde or Observation; Observation-backed adapters come from `sun_observations::evidence`. Both layers ship in the sun-observations package without features. The generic certification layer remains independent of runtime concepts and state.

Canonical flow: **source → fact / Fact → expect → CheckResult → check → Certification → assert_certify**. Analysis and Fact are neutral; failed expect returns data, never panics. One acceptance owner; final assertion only at the proof boundary.

### Fact and CheckResult

| API | Contract |
| --- | --- |
| `fact(source:impl Into<String>, observed:T) -> Fact<T>`; `try_fact(source:impl Into<String>, observed:Result<T,E:Display>) -> Fact<T>` | Observed evidence or bounded AnalysisError{message}; direct analysis retains typed errors. |
| `Fact{source,observed:Result<T,AnalysisError>,details:String}` | `details(text)` adds context; `expect(E:Expectation<T>) -> CheckResult<T,E::Violation>` consumes without cloning. |
| `expect_valid()` | Delegates to AllValid for IntegrityReport, &Comparison or PairCheck. |
| `Fact<LinearFitMetrics>::slope()/intercept()/r_squared() -> Fact<f64>` | Undefined metric becomes EvaluationError. |
| `CheckResult<T,V=Violation>` | Immutable result: `source()/expected()/observed()/violations()/class()/analysis_error()/details()`; `is_pass()`. Full observed evidence survives rejection. |
| `CheckClass::{Pass,HardViolation,StatisticalViolation,EvaluationError}` | Every non-Pass fails final proof. |
| `Expectation<T>` | Extension trait: Violation:Debug; `assess(observed:&T)->Result<Vec<Violation>,AnalysisError>`, `describe()->String`, `class()->ViolationClass` (default Hard). `ViolationClass::{Hard,Statistical}` classifies rejection only. |

### Expectations

| API | Acceptance |
| --- | --- |
| Plain `T:Debug+PartialEq`; `NotEqual<T>(T)` | Exact equality/inequality for scalars, structs, enums, bool, Option, collections; &str also expects owned String. |
| `Criterion::greater_than(T)/at_least(T)/less_than(T)/at_most(T)/between(min:T,max:T) -> Result<Criterion<T>,AnalysisError>` | T:Debug+PartialOrd; inclusive range; invalid bounds rejected at construction, incomparable evidence → EvaluationError. |
| `criterion(expected:impl Into<String>, class:ViolationClass, predicate:impl Fn(&T)->bool) -> Predicate` | Custom hard/statistical expectation; false → Violation{expected:String}. |
| `close_to(expected:f64) -> Result<CloseTo,NumericError>` | abs(actual−expected) <= absolute + relative*abs(expected); defaults absolute=1e-9, relative=1e-9. `absolute_tolerance(tolerance:f64)` / `relative_tolerance(tolerance:f64) -> Result<Self,NumericError>` validate finite nonnegative tolerances and combined arithmetic. Nonfinite evidence/delta → EvaluationError. |
| `CountRule::{Exact(usize),Minimum(usize),NonEmpty}` | Hard count; plain expected usize is exact. Absence = expect(0usize). |
| `NumericRule::{Finite,Positive,NonNegative}`; `NumericRule::between(min:f64,max:f64) -> Result<Self,NumericError>` | Finite inclusive bounds validated at construction using `NumericRange::new(min,max)`; hard numeric acceptance, finite evidence even for sign checks. |
| `AllValid` | Hard structural validity; prefer expect_valid(). |
| `AtMostAnomalies(usize)`; `LagExpectation{best_lag:i64}`; `GroupExpectation::new(minimum_count:usize,mean_min:f64,mean_max:f64) -> Result<Self,NumericError>` | Statistical report acceptance below. |

## Planned production execution

Keep `certification(name)` for eager proofs. For repeated production execution use `certification(name).executor(|input: &Input| -> Result<Output, Error> { ... })` and bind `let case = c.case();` once. The typed symbolic root is necessary for Rust inference; it does not borrow the runner. Do not wrap the whole proof in a test closure.

```rust
use sun_observations::certifications::*;
#[derive(DomainInput)] enum Mode { Fast, Accurate }
#[derive(DomainInput)]
struct Request { mode: Mode, window: usize, values: Vec<f64> }
fn execute(input: &Request) -> Result<Vec<f64>, &'static str> {
    let allowed = match input.mode { Mode::Fast => 1..=3, Mode::Accurate => 4..=6 };
    if !allowed.contains(&input.window) || !(2..=8).contains(&input.values.len())
        || input.window > input.values.len()
        || input.values.iter().any(|v| !v.is_finite() || !(-1.0..=1.0).contains(v)) {
        return Err("invalid request");
    }
    Ok(input.values[..input.window].to_vec())
}
fn planned() {
let mut c = certification("production prediction").executor(execute);
c.data_source([Request { mode: Mode::Fast, window: 2, values: vec![0.0; 2] }]);
c.domain(domain!(Request {
    mode.all_enum_values(),
    when(mode.equal(Mode::Fast)) { window.in_range(1..=3) },
    when(mode.equal(Mode::Accurate)) { window.in_range(4..=6) },
    values { length().in_range(2..=8), each(in_range(-1.0..=1.0)) },
    window.less_equal(values.length()),
}));
c.cases_budget(48);
let case = c.case();
c.check(fact!(case.result().len()).expect(fact!(case.input().window))
    .description("one output per requested item"));
c.assert_certify();
}
```

- `data_source` contains real **inputs**, not result/Observation views. Inputs run first through the same executor and reduce generated duplication. Data-source-only planned execution does not require `DomainInput`; derive it only when registering a Domain. Real cases may exceed the total case budget, but a Domain requires some generated capacity.
- `DomainInput` derives typed descriptors for concrete input structs/enums. `domain!` requires exactly one constructive source for every active field: `fixed_value`, `selected_values`, `in_range`, `one_of_ranges`, or a nested domain. Use `all_enum_values` / `selected_enum_values` for choices; payload blocks select their own enum variant fields. Collections use `length().in_range` and `each(...)` or `each { ... }`. `when(...)` activates another field's domain.
- Numeric relations: `less_than`, `less_equal`, `greater_than`, `greater_equal`, `equal`, `not_equal`. Operands must have the same scalar type; collection length is `usize`; nested struct leaves may be referenced by paths such as `config.window.less_equal(values.length())`. No opaque Domain predicates, baseline templates, or implicit intersections. Incomplete, ambiguous or unsatisfiable construction fails closed before generated execution. No shrinking.
- `fact!(value)` is eager. `fact!(case.input()...)` / `fact!(case.result()...)` are deferred, resolving on every successful positive case through ordinary Fact/Expectation acceptance. `check` registers heterogeneous plans or eager results; `.description(...)` is optional human context, never identity. Clone borrowed non-Copy evidence when an owned fact is needed.
- Positive `Err`, accepted negative inputs, and executor panics fail. Expected negative rejection passes without evaluating positive checks. Independent positive checks still collect failures. Use unwind-enabled Rust builds for panic capture.
- An executor returning `capture_observations(config, || production(input))` supplies `CapturedRun<Result<...>>`. Its `case.observations::<T>() -> Result<TypedView<T>, DecodeError>` is available only for captured execution; e.g. `fact!(case.observations::<Row>().unwrap().values().len())`. Completeness is mandatory even for negative rejection. Generic planning never generates from evidence.
- `cases_budget(n)` overrides `SUN_CERT_CASES_BUDGET` (default 64); `seed(n)` overrides `SUN_CERT_SEED` (default 184729361). `failure_budget(n)` defaults to five representative cases and stops further construction/execution when filled. Diagnostics stay bounded.
- `assert_certify()` consumes the runner and is silent on success. `report()` returns an immutable result without asserting. `SUN_CERT_REPORT=1` or `report_mode(true)` requests a compact coverage/check summary, not a per-execution dump.

For deterministic fixture variation, use `.executor_with_seed(|input, seed: u64| ...)`; the executor still receives an already materialized input. The framework derives the case-local seed from the global seed and certification identity, using canonical Domain coordinates for generated cases and the zero-based registered data-source ordinal for real cases. It never advances a shared execution RNG: reordering other executions or changing the budget does not change the seed of the same case. Domain/planner randomness remains framework-owned. Ordinary `.executor(|input| ...)` needs no seed or RNG state; both forms share Result, captured evidence, panic, rejection and failure-budget semantics.

Call `c.require_pairwise_coverage();` to make exploration completeness part of `report().is_pass()` and `assert_certify()`. It requires all feasible enum values and pairwise enum combinations from the existing context-aware catalog, including conditional/nested contexts; infeasible combinations do not enter the denominator. Real cases prefill coverage. Missing Domain or zero pairwise total fails closed, not vacuously. Incomplete coverage yields an EvaluationError with covered/total counts; the requirement never increases the case budget or bypasses the failure stop. Without it, coverage remains informational.

```rust
use sun_observations::certifications::*;
#[derive(DomainInput)] enum Choice { A, B }
#[derive(DomainInput)] struct Scenario { left: Choice, right: Choice }
fn planned() {
let mut c = certification("seeded pairwise fixtures")
    .executor_with_seed(|_: &Scenario, seed: u64| Ok::<_, ()>(seed));
c.domain(domain!(Scenario { left.all_enum_values(), right.all_enum_values() }));
c.seed(719).cases_budget(4);
c.require_pairwise_coverage();
c.assert_certify();
}
```

## Prepared sources, groups and typed comparison

Names accept Into<Arc<str>>. Keep borrowed inputs alive. PreparedSource clones share slices/name/provenance, never payloads; registration does not borrow Certification. Projections/selectors express semantic fields.

### PreparedSource

| API | Result/semantics |
| --- | --- |
| `source(name:impl Into<Arc<str>>,value:&T)` / `c.source(name,value)`; `collection(name:impl Into<Arc<str>>,values:&[T])` / `c.collection(name,values)` | PreparedSource<'a,T>. Singular &Vec is one Vec value, not rows; collections accept array/Vec slice coercions. Typed handles, no erased registry. |
| `name() -> &str`; `values() -> &'a [T]`; `entry(index) -> SourceEntry<'a,T>`; `iter()` | Entry{value:&T,index:usize,provenance:Option<&dyn SourceProvenance>}; exact/double-ended/cloneable iterator. entry uses ordinary indexing preconditions. |
| `with_provenance(provenance:&'a dyn SourceProvenance) -> Self` | Adapter-owned `SourceProvenance::describe(index,&mut dyn fmt::Write)->fmt::Result`; never infer semantics from formatted coordinates. |
| `single_map(project:impl FnOnce(&T)->V) -> Fact<V>` | Exactly one value, otherwise EvaluationError without calling projection. |
| `project(project:impl Fn(&T)->V) -> PreparedValues<V>`; `numeric(project:impl Fn(&T)->f64) -> PreparedNumeric` | Eager once-per-row projection for reuse. |
| `count() -> Fact<usize>`; `matching_count(predicate:impl Fn(&T)->bool) -> Fact<usize>` | All/selected count, including forbidden absence. |
| `unique(key:impl Fn(&T)->K)`; `contiguous(key:impl Fn(&T)->u64,start:u64,end:u64)` | Fact<IntegrityReport<K/u64>>; unique K:Ord+Clone, inclusive range, compact missing ranges. |
| `keys(key:impl Fn(&T)->K) -> PreparedKeys<'a,T,K>`; `compare_with(right:&PreparedSource<R>) -> ComparisonBuilder` | K:Ord+Clone cached sorted keys or explicit correspondence builder. |
| `group_by(key:impl Fn(&T)->K) -> GroupedView<T,K>` | K:Ord, no Clone for membership; ascending groups, source order inside. |
| `lag_self(projection,config)`; `lag_with(right:&PreparedSource<R>,left_projection,right_projection,config:LagConfig) -> Fact<LagReport>` | One projection for self-lag; two for cross-source or same-source fields. Each search projects once. |

### PreparedValues

`values() -> &[V]`, `source() -> PreparedSource<V>`; external provenance not carried—retain original source-local indices for lookup.

### PreparedNumeric

`values() -> &[f64]`, `series() -> NumericSeries`; `summary()/robust_summary()/quantile(q:f64)/outliers(config:OutlierConfig)/jumps(config:JumpConfig)` return Fact. `expect(rule:NumericRule) -> CheckResult<IntegrityReport>` composes series integrity → Fact → expect_valid. Indices are source-local; analysis never reruns projection.

### PreparedKeys

`indices() -> &[(K,usize)]`, `compare_with(&PreparedKeys) -> PairedComparison`; merges cached indices for several counterparts.

### GroupedView

`iter()` yields Group{key:&K}; `numeric(project) -> GroupedNumeric` caches one f64 per source row.

### Group

`len()/is_empty()/values()/iter()/numeric(project)`; iter yields SourceEntry, numeric is lazy NumericSeries.

### GroupedNumeric

`summaries()/summaries_with_robust() -> GroupedSummaryReport<K>` reuse cached values (K:Ord+Clone for owned keys). Report.groups contains GroupSummary{key,member_count,summary:Result<Summary,NumericError>,robust:Option<Result<RobustSummary,NumericError>>}; failed groups retain identity, healthy groups remain. GroupExpectation rejects zero groups/count/mean violations statistically; construction validates mean bounds; unavailable summaries → EvaluationError.

### ComparisonBuilder

`on(left:impl Fn(&L)->K,right:impl Fn(&R)->K) -> PairedComparison<'a,L,R,K>`; `on_presorted(left,right) -> Result<PairedComparison,JoinError>` rejects decreasing promised-sorted keys. Generic K:Ord+Clone; implementation chooses the canonical sort/merge behavior. Left/right types may differ; no Hash requirement or string field-name joins.

### PairedComparison

| API | Result/semantics (K:Clone+Debug) |
| --- | --- |
| `name() -> String`; `comparison() -> &Comparison<K>` | Named left → right and full mapping; field/check names belong on checks. |
| `matched_pairs()` | Iterator<(&K,SourceEntry<L>,SourceEntry<R>)> over the matched subset, with original indices/provenance. |
| `paired_comparison_fact() -> Fact<&Comparison<K>>` | No giant copy; expect_valid requires exact unique correspondence, bounded missing/duplicate examples. |
| `equal(left:impl Fn(&L)->V,right:impl Fn(&R)->V,top:usize)`; `check_pairs(predicate:impl Fn(&L,&R)->bool,top:usize)` | Fact<PairCheck<K>> with read-only matched()/failures()/examples(); equality V:PartialEq. Examples cap min(top,32) with provenance; counts precede examples. Incomplete comparison → EvaluationError, never subset proof. |
| `numeric(left:impl Fn(&L)->f64,right:impl Fn(&R)->f64) -> PreparedNumericComparison<K>` | f64 pairs projected once in comparison order; no new join. Reuse comparison across fields. |
### PreparedNumericComparison

Owns eagerly projected pairs; lazily caches compact linear/rank/difference/residual results **including errors**, once. Repeated views do not decode/project/sort keys/rebuild correspondence or copy comparisons; rank/quantile scratch may allocate. All analysis requires complete comparison.

| Method | Result |
| --- | --- |
| `values()` | &[(f64,f64)] |
| `correlation()` | Fact<CorrelationMetrics{pearson:Metric,spearman:Metric}> |
| `linear_fit()` | Fact<LinearFitMetrics{linear:LinearMetrics,residual:Option<DifferenceSummary>}> |
| `relation()` | Fact<RelationMetrics{linear,spearman,difference,residual}>; composite constraints over cached metrics. |
| `difference()` | Fact<DifferenceSummary>; right−left, not fitted residuals. |
| `linear()` | Fact<LinearMetrics>; covariance/OLS summary. |
| `pearson()`; `spearman()` | Fact<Metric>; scalar metric views over the same caches. |
| `largest(top)` | Fact<Vec<PairEvidence<K>>>; bounded top-N scratch, decreasing absolute disagreement, deterministic key-order ties. |

### Statistical expectation builders and measures

Expectations start with a real bound; constructors/builders return Result<Self,NumericError>. Bounds are inclusive, finite, ordered, and cannot both be absent. Owners also reject impossible metric domains: Pearson/Spearman stay in [-1,1], absolute correlations and R² in [0,1], and absolute difference/residual bounds are nonnegative. Required undefined metrics → EvaluationError; violated valid bounds → StatisticalViolation with `MetricConstraint<M>`. `MetricConstraint::new(measure:M,min:Option<f64>,max:Option<f64>)` validates generic finite/order structure; expectation owners validate metric domains. Read-only `measure()/min()/max()`.

| Owner | Constructors, additional bounds and measures |
| --- | --- |
| CorrelationExpectation | `pearson_at_least(min:f64)`, `spearman_at_least(min:f64)`, `spearman_at_most(max:f64)`, `with_bound(measure,min,max)`; `and_pearson_bounds(min,max)`, `and_spearman_bounds(min,max)`, `bound(measure,min,max)`. CorrelationMeasure::{Pearson,Spearman,AbsolutePearson,AbsoluteSpearman}. |
| LinearFitExpectation | `slope_between(min,max)`, `intercept_between(min,max)`, `r_squared_at_least(min)`, `residual_mean_absolute_at_most(max)`, `residual_max_absolute_at_most(max)`, `with_bound(measure,min,max)`; additional `and_slope_between`, `and_intercept_between`, `and_r_squared_at_least`, `and_residual_mean_absolute_at_most`, `and_residual_max_absolute_at_most`, `bound(measure,min,max)`. LinearFitMeasure::{Slope,Intercept,RSquared,ResidualMeanAbsolute,ResidualMaxAbsolute}. |
| DifferenceExpectation | `mean_absolute_at_most(max)`, `p95_absolute_at_most(max)`, `max_absolute_at_most(max)`, `with_bound(measure,min,max)`; additional `and_mean_absolute_at_most`, `and_p95_absolute_at_most`, `and_max_absolute_at_most`, `bound(measure,min,max)`. DifferenceMeasure::{MeanAbsolute,P95Absolute,MaxAbsolute}. |
| RelationExpectation | `spearman_at_least(min)`, `spearman_at_most(max)`, `with_bound(measure,min,max)`, `bound(measure,min,max)`. `RelationConstraint = MetricConstraint<RelationMeasure>`. RelationMeasure::{Pearson,Spearman,AbsolutePearson,AbsoluteSpearman,Slope,Intercept,RSquared,MeanAbsoluteDifference,P95AbsoluteDifference,MaxAbsoluteDifference,ResidualMeanAbsolute,ResidualMaxAbsolute}. |

## Anomaly, jump and temporal lag

- `OutlierConfig::new(threshold:f64,zero_mad:ZeroMadPolicy)` validates finite nonnegative threshold; default is 3.5/ExactMedian. modified-z = 0.6744897501960817*(value−median)/MAD, strict magnitude threshold. `ZeroMadPolicy::absolute_tolerance(f64)` validates tolerance in original units; ExactMedian flags every nonmedian value at zero MAD. Reports retain summary and source-local suspicious indices; `RobustScore::{ModifiedZ(f64),ZeroMadDeviation{signed_deviation}}`.
- `JumpConfig::new(absolute_threshold:Option<f64>,relative_threshold:Option<f64>,denominator_floor:f64)` validates finite nonnegative thresholds and positive finite floor. At least one threshold is required. Either strict threshold flags; relative=abs(delta)/max(abs(previous),floor). Reports retain record/transition counts, maxima, suspicious transitions and both endpoints; earliest maximum wins ties. AtMostAnomalies bounds suspicious counts.
- `LagRange::new(start:i64,end:i64)` validates inclusive range with at most `MAX_LAG_CANDIDATES=4097`; accessors `start()/end()`. `LagConfig{range:LagRange,measure:LagMeasure::{Pearson,Spearman}}`. Positive d pairs **left k with right k+d**; overflow-safe shifts, duplicate keys error. Best maximizes absolute correlation, ties smallest |d| then signed d. Partition one unique temporal domain/search.
- LagReport{config,left_source/right_source:Option<String>,left_count,right_count,candidates,best}, `zero_lag()->Option<&LagCandidate>`. LagCandidate{lag,alignment:LagAlignment,relation}; `LagAlignment::default()` sets all five counters to 0; LagAlignment{pair_count,boundary_left,boundary_right,right_only,left_only}, `has_interior_gaps()->bool` distinguishes expected boundary loss/interior gaps. BestLag{lag,relation,strength}; best=None if all undefined. LagExpectation rejects `LagViolation::{UnexpectedBestLag,NoDefinedCandidate,InteriorMissingKeys}`.

## Direct analysis and report/error reference

Operations return data/errors, never assertions. Cloneable iterators must replay identical ordered values; lazy selectors must be pure. Numeric reports/errors support Serde; structural Comparison is output-only Serialize to prevent fabricated correspondence.

| Function/type | Public surface/meaning |
| --- | --- |
| NumericSeries | `new(Iterator<Item=f64>+Clone)`, `iter()`; `finite_count()->FiniteCount{count,finite_count}`; `summary()/quantile(probability:f64)/robust_summary()/integrity(rule:NumericRule)/outliers(config:OutlierConfig)/jumps(config:JumpConfig)` → Result<report,NumericError>. Lazy selectors may rerun. |
| Summary; RobustSummary | Summary{count,finite_count,min,max,sum,mean,variance(population),sample_variance:Option<f64>(n<2 absent),standard_deviation}; RobustSummary{count,median,q1,q3,iqr,mad(unscaled)}. Type-7 interpolation; empty/nonfinite/overflow/underflow errors; q finite in [0,1]. |
| Integrity functions | `check_count(actual:usize,expected:CountRule)`, `check_unique(values:&[T],key:impl Fn(&T)->K)`, `check_contiguous(values:&[T],key:impl Fn(&T)->u64,start:u64,end:u64)` → IntegrityReport{record_count,issues}, `is_valid()`. IntegrityIssue::{Count{expected,actual},Duplicate(KeyGroup),UnexpectedKey{key},MissingRange{start,end},InvalidKeyRange{start,end},Numeric{index,value,expected}}. Aggregate via fact(...).expect_valid(). |
| Comparison<K> | `check_same_keys(left,right,left_key,right_key)` → structural report; read-only `left_count()/right_count()/matched()/left_only()/right_only()/duplicate_left()/duplicate_right()/left_source()/right_source()`. `is_complete()` requires matched == left_count == right_count and no one-sided/duplicate keys. |
| Join | `join_by(left,right,left_key,right_key)`, `join_presorted_by(...) -> Result<Join,JoinError>`; `comparison()`, `matched_pairs()`, `require_complete()`, `set_source_names(&str,&str)`. JoinError{side:Side::{Left,Right},index}. Integer-specialized join machinery is not part of the agent-facing proof API. |
| Pair analysis | `linear_metrics(pairs)/pearson(pairs)/spearman(pairs)` accept IntoIterator<(f64,f64)> → Result<LinearMetrics/Metric/Metric,NumericError>; `correlation(cloneable_pairs)/linear_fit(cloneable_pairs)/numeric_relation(cloneable_pairs)` → corresponding metrics. Cloneable_pairs is Iterator+Clone. `difference_summary(IntoIterator<(f64,f64)>)` → DifferenceSummary. |
| LinearMetrics; Metric | LinearMetrics{count,pearson,covariance(population),sample_covariance,slope,intercept,r_squared}; OLS right on left. Spearman average tied ranks, including signed zero. `Metric::{Defined(f64),Undefined(UndefinedReason::{InsufficientPairs,ConstantLeft,ConstantRight})}`, `value()->Option<f64>`; no silent NaN. |
| DifferenceSummary; pair reports | DifferenceSummary{signed:Summary,absolute:Summary,median_signed,median_absolute,p95_absolute}; direct right−left. Fit residuals = observed right−predicted right. RelationMetrics{linear,spearman,difference,residual:Option<DifferenceSummary>}; PairEvidence<K>{key,left,right,signed_difference,absolute_difference}. |
| Direct lag | `lag_relation_by(left:&[L],right:&[R],left_project:impl Fn(&L)->(i64,f64),right_project:impl Fn(&R)->(i64,f64),config:LagConfig)->Result<LagReport,LagError>`: slices, one (i64,f64) projection/row. LagError::{InvalidRange,DuplicateKeys{left,right},Projection{side,error},Numeric{lag,error}}. |
| Numeric/error values | NumericError::{Empty,NonFinite{index,value},InvalidParameter{name,value},ArithmeticOverflow{operation},ArithmeticUnderflow{operation}}. NumericValue::{Finite(f64),Nan,PositiveInfinity,NegativeInfinity}, From<f64>, JSON-safe. |

## Hierarchy, report and final assertion

`certification(name:impl Into<String>) -> Certification`. Methods: `source(name,value:&T)/collection(name,values:&[T])` above; `check(name:impl Into<String>,result:CheckResult)`; `check_fact_expect(name,observed,expected)` (exactly check(name,fact(name,observed).expect(expected))); `section(name,checks:impl FnOnce(&mut Certification))`; `limits(limits:DiagnosticLimits)`. Check/section/limits return &mut Self. Eager collect-all, nested semantic paths (encoder/prediction/timing/), no independent short-circuiting. Fallible dependent preparation uses try_fact, not ? out of the entire proof.

`Certification::report() -> CertificationReport` returns a nonpanicking immutable snapshot: `name()/checks()/failed()/failures()/is_pass()`. Zero checks cannot pass. `Certification::assert_certify()` and `CertificationReport::assert_certify()` assert at the proof boundary with bounded diagnostics; successful Display is empty.

`DiagnosticLimits{failures,characters_per_failure}` defaults 64/2048 UTF-8 bytes, plus truncation marker and bounded names/paths; omitted failures remain counted. Success aggregation retains no fact report/duplicate comparison. Bounded writers stop traversal when formatters propagate errors; custom Debug must propagate write errors and avoid unbounded allocation/work before writing (a writer cannot interrupt ignored errors). `bounded_debug(&value,limit)` supports custom detail. Keep full typed reports separately; never inject unbounded JSON into panics.

## Ordinary + Observation-backed composition

`CapturedRun::certification` is the completeness-aware evidence bridge. Start every capture proof with **run.certification(name)**, not standalone certification: it always contributes an explicit capture-completeness prerequisite; healthy capture passes it, while eviction, emission failure or escaped contexts/workers/spans makes it EvaluationError. A retained subset never establishes completeness. Capture mechanics belong to the sun-observations skill. Decode TypedView once; reuse its source. Filter proc/scope or include execution identity for repeated keys. Provenance optional; ordinary sources work identically.

```rust
use sun_observations::certifications::*;
fn ordinary() {
    let returned = 3usize;
    let rows = [1u64, 2, 3];
    let mut c = certification("canonical result and artifact");
    let result = c.source("result", &returned);
    let data = c.collection("artifact", &rows);
    c.section("result", |c| { c.check("ready", result.single_map(|v| *v).expect(3usize)); });
    c.check("unique", data.unique(|v| *v).expect_valid());
    c.check("contiguous", data.contiguous(|v| *v, 1, 3).expect_valid());
    c.check("forbidden future", data.matching_count(|v| *v > 3).expect(0));
    c.assert_certify();
}
```

```rust
use sun_observations::certifications::*;
use sun_observations::evidence::{CapturedRun, DecodeError};
use serde::{Serialize, Deserialize};
use sun_observations::Observation;
#[derive(Serialize,Deserialize,Observation)]
#[observation(namespace="encoder::prediction")]
struct Prediction { fold:u8, row:u64, signal:f64 }
struct Output { fold:u8, row:u64, signal:f64 }
fn mixed(run:&CapturedRun<Vec<Output>>) -> Result<(),DecodeError> {
    let typed = run.to_typed::<Prediction>()?;
    let mut c = run.certification("internal transfer to canonical result");
    let internal = typed.source("predictions");
    let result = c.collection("output", run.result());
    let pair = internal.compare_with(&result).on(|v|(v.fold,v.row), |v|(v.fold,v.row));
    let signal = pair.numeric(|v|v.signal, |v|v.signal);
    c.check("keys", pair.paired_comparison_fact().expect_valid());
    c.check("exact transfer", pair.equal(|v|v.signal, |v|v.signal, 5).expect_valid());
    c.check("association", signal.correlation().expect(CorrelationExpectation::spearman_at_least(0.9).unwrap()));
    c.check("difference", signal.difference().expect(DifferenceExpectation::max_absolute_at_most(0.).unwrap()));
    c.assert_certify();
    Ok(())
}
```
