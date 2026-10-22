---
name: certifications
description: Certify semantic Rust contracts over ordinary values, collections, artifacts, reference results and optional Observation evidence with sun-certifications; use named prepared sources, typed-key comparison, statistics, expectations and hierarchical collect-all bounded diagnostics.
---

# Semantic certification methodology and API

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

`use sun_certifications::*`. Ordinary evidence needs no Serde or Observation; Observation-backed adapters come from `sun-observations-data-source`. The generic crate does not depend on the runtime.

Canonical flow: **source → fact / Fact → expect → CheckResult → check → Certification → assert_certify**. Analysis and Fact are neutral; failed expect returns data, never panics. One acceptance owner; final assertion only at the proof boundary.

### Fact and CheckResult

| API | Contract |
| --- | --- |
| `fact(source:Into<String>, value:T) -> Fact<T>`; `try_fact(source, Result<T,E:Display>)` | Observed evidence or bounded AnalysisError{message}; direct analysis retains typed errors. |
| `Fact{source,observed:Result<T,AnalysisError>,details:String}` | `details(text)` adds context; `expect(E:Expectation<T>) -> CheckResult<T,E::Violation>` consumes without cloning. |
| `expect_valid()` | Delegates to AllValid for IntegrityReport, &Comparison or PairCheck. |
| `Fact<LinearFitMetrics>::slope()/intercept()/r_squared() -> Fact<f64>` | Undefined metric becomes EvaluationError. |
| `CheckResult<T,V=Violation>` | Public source/expected/observed/violations/class/analysis_error/details; `is_pass()`. Full observed value survives failed acceptance; failed preconditions set analysis_error even with observed Ok. |
| `CheckClass::{Pass,HardViolation,StatisticalViolation,EvaluationError}` | Deterministic mismatch / statistical mismatch / invalid, unavailable or incomplete evidence/evaluation. **Every non-Pass fails final proof**. |
| `Expectation<T>` | Extension trait: associated Violation:Debug; `assess(&T)->Result<Vec<Violation>,AnalysisError>`, `describe()->String`, `class()->CheckClass` (default HardViolation). Own each domain acceptance rule once. |

### Expectations

| API | Acceptance |
| --- | --- |
| Plain `T:Debug+PartialEq`; `Equal<T>(T)`; `NotEqual<T>(T)` | Equality/inequality for scalars, structs, enums, bool, Option, collections; &str also expects owned String. Select fields for independent failures. |
| `Criterion::{Equal(T),NotEqual(T),Greater(T),AtLeast(T),Less(T),AtMost(T),Range{min:T,max:T}}` | T:Debug+PartialOrd; inclusive range; invalid/incomparable/reversed bounds → EvaluationError. |
| `criterion(description,class,Fn(&T)->bool) -> Predicate` | Custom hard/statistical rule; false → Violation{expected:String}; Pass/EvaluationError classifications invalid. Ordinary Rust control flow, no conditional DSL. |
| `close_to(expected:f64) -> CloseTo` | abs(actual−expected) <= absolute + relative*abs(expected); both default 1e-9. `absolute(tolerance)/relative(tolerance)`; nonfinite evidence/expected/tolerance/arithmetic or negative tolerance → EvaluationError. |
| `CountRule::{Exact(usize),Minimum(usize),NonEmpty}` | Hard count; plain expected usize is exact. Absence = expect(0usize). |
| `NumericRule::{Finite,Range{min:f64,max:f64},Positive,NonNegative}` | Hard numeric acceptance, finite evidence even for sign checks; invalid bounds → EvaluationError. |
| `AllValid` | Hard structural validity; prefer expect_valid(). |
| `AtMostAnomalies(usize)`; `LagExpectation{best_lag:i64}`; `GroupExpectation{minimum_count,mean_min,mean_max}` | Statistical report acceptance below. |

## Prepared sources, groups and typed comparison

Names accept Into<Arc<str>>. Keep borrowed inputs alive. PreparedSource clones share slices/name/provenance, never payloads; registration does not borrow Certification. Projections/selectors express semantic fields.

### PreparedSource

| API | Result/semantics |
| --- | --- |
| `source(name,&T)` / `c.source(name,&T)`; `source_slice(name,&[T])` / `c.collection(name,&[T])` | PreparedSource<'a,T>. Singular &Vec is one Vec value, not rows; collections accept array/Vec slice coercions. Typed handles, no erased registry. |
| `name() -> &str`; `values() -> &'a [T]`; `entry(index) -> SourceEntry<'a,T>`; `iter()` | Entry{value:&T,index:usize,provenance:Option<&dyn SourceProvenance>}; exact/double-ended/cloneable iterator. entry uses ordinary indexing preconditions. |
| `with_provenance(&'a dyn SourceProvenance) -> Self` | Adapter-owned `SourceProvenance::describe(index,&mut dyn fmt::Write)->fmt::Result`; never infer semantics from formatted coordinates. |
| `select(FnOnce(&T)->V) -> Fact<V>` | Exactly one value, otherwise EvaluationError without calling selector. |
| `project(Fn(&T)->V) -> PreparedValues<V>`; `numeric(Fn(&T)->f64) -> PreparedNumeric` | Eager once-per-row projection for reuse. |
| `count() -> Fact<usize>`; `matching_count(predicate)` | All/selected count, including forbidden absence. |
| `unique(key)`; `contiguous(u64_key,start,end)` | Fact<IntegrityReport<K/u64>>; unique K:Ord+Clone, inclusive range, compact missing ranges. |
| `keys(key) -> PreparedKeys<'a,T,K>`; `compare_with(&other) -> ComparisonBuilder` | K:Ord+Clone cached sorted keys or explicit correspondence builder. |
| `group_by(key) -> GroupedView<T,K>` | K:Ord, no Clone for membership; ascending groups, source order inside. |
| `lag_relation_by(&other,left_selector,right_selector,LagConfig) -> Fact<LagReport>` | Each selector yields (i64,f64), projected once per search. |

### PreparedValues

`values() -> &[V]`, `source() -> PreparedSource<V>`; external provenance not carried—retain original source-local indices for lookup.

### PreparedNumeric

`values() -> &[f64]`, `series() -> NumericSeries`; `summary()/robust_summary()/quantile(q)/check(NumericRule)/outliers(OutlierConfig)/jumps(JumpConfig)` all return Fact. Report indices remain source-local. Scratch separate; repeated analysis never reruns projection.

### PreparedKeys

`indices() -> &[(K,usize)]`, `compare_with(&PreparedKeys) -> PreparedComparison`; merges cached indices for several counterparts.

### GroupedView

`iter()` yields Group{key:&K}; `numeric(project) -> GroupedNumeric` caches one f64 per source row.

### Group

`len()/is_empty()/values()/iter()/numeric(project)`; iter yields SourceEntry, numeric is lazy NumericSeries.

### GroupedNumeric

`summaries()/summaries_with_robust() -> GroupedSummaryReport<K>` reuse cached values (K:Ord+Clone for owned keys). Report.groups contains GroupSummary{key,member_count,summary:Result<Summary,NumericError>,robust:Option<Result<RobustSummary,NumericError>>}; failed groups retain identity, healthy groups remain. GroupExpectation rejects zero groups/count/mean violations statistically; invalid bounds/unavailable summaries → EvaluationError.

### ComparisonBuilder

`on(lk,rk)`, `on_u64(lk,rk)` → PreparedComparison<'a,L,R,K/u64>; `on_sorted(lk,rk) -> Result<PreparedComparison,JoinError>` rejects decreasing promised-sorted keys. Generic K:Ord+Clone; adaptive sort/merge and u64 bounded-dense optimization share ambiguity semantics. Left/right types may differ; no Hash requirement or string field-name joins.

### PreparedComparison

| API | Result/semantics (K:Clone+Debug) |
| --- | --- |
| `name() -> String`; `comparison() -> &Comparison<K>` | Named left → right and full mapping; field/check names belong on checks. |
| `pairs()` | Iterator<(&K,SourceEntry<L>,SourceEntry<R>)> for deliberate subset investigation; original indices/provenance. |
| `comparison_fact() -> Fact<&Comparison<K>>` | No giant copy; expect_valid requires exact unique correspondence, bounded missing/duplicate examples. |
| `equal(lp,rp,top)`; `check_pairs(Fn(&L,&R)->bool,top)` | Fact<PairCheck<K>{matched,failures,examples}>; equality V:PartialEq. Examples cap min(top,32) with provenance; counts precede examples. Incomplete comparison → EvaluationError, never subset proof. |
| `numeric(lp,rp) -> PreparedNumericComparison<K>` | f64 pairs projected once in comparison order; no new join. Reuse comparison across fields. |
| `lag(lp,rp,config) -> Fact<LagReport>` | (i64,f64) selectors; separate shifted-domain sort, not zero-lag mapping. Partition repeated keys by fold/target/execution first. |

### PreparedNumericComparison

Owns eagerly projected pairs; lazily caches compact linear/rank/difference/residual results **including errors**, once. Repeated views do not decode/project/sort keys/rebuild correspondence or copy comparisons; rank/quantile scratch may allocate. All analysis requires complete comparison.

| Method | Result |
| --- | --- |
| `values()` | &[(f64,f64)] |
| `correlation()` | Fact<CorrelationMetrics{pearson:Metric,spearman:Metric}> |
| `linear_fit()` | Fact<LinearFitMetrics{linear:LinearMetrics,residual:Option<DifferenceSummary>}> |
| `relation()` | Fact<RelationMetrics{linear,spearman,difference,residual}>; advanced composite, ordinary checks prefer separate views. |
| `difference()` | Fact<DifferenceSummary>; right−left, not fitted residuals. |
| `linear()` | Fact<LinearMetrics>; covariance/OLS summary. |
| `pearson()`; `spearman()` | Fact<Metric>; low-level scalar views; prefer correlation() for correlation expectations. |
| `largest(top)` | Fact<Vec<PairEvidence<K>>>; bounded top-N scratch, decreasing absolute disagreement, deterministic key-order ties. |

### Statistical expectation builders and measures

Builders below return Result<Self,NumericError>, except default(). Bounds inclusive, finite, ordered; reversed/nonfinite → NumericError. Required undefined/nonfinite metrics → EvaluationError; violated valid bounds → StatisticalViolation with typed `MetricConstraint<M>{measure:M,min:Option<f64>,max:Option<f64>}`.

| Owner | Builders and complete measure enum |
| --- | --- |
| CorrelationExpectation | `positive(min_spearman)` (0,1], `negative(max_spearman)` [-1,0); `default().pearson(min,max)/spearman(min,max)/bound(CorrelationMeasure,min,max)`, min/max Option<f64>. CorrelationMeasure::{Pearson,Spearman,AbsolutePearson,AbsoluteSpearman}. |
| LinearFitExpectation | `default().slope(min:f64,max:f64)/intercept(min,max)/r_squared(min)/residual_mean_absolute(max)/residual_max_absolute(max)`; `bound(LinearFitMeasure,min:Option<f64>,max:Option<f64>)`. LinearFitMeasure::{Slope,Intercept,RSquared,ResidualMeanAbsolute,ResidualMaxAbsolute}. |
| DifferenceExpectation | `default().mean_absolute(max)/p95_absolute(max)/max_absolute(max)`; `bound(DifferenceMeasure,min,max)` with Option bounds. DifferenceMeasure::{MeanAbsolute,P95Absolute,MaxAbsolute}. |
| RelationExpectation | `positive(min_spearman)/negative(max_spearman)/default().bound(RelationMeasure,min,max)`; same correlation bound domains. Public constraints:Vec<RelationConstraint>; `RelationConstraint = MetricConstraint<RelationMeasure>`. RelationMeasure::{Pearson,Spearman,AbsolutePearson,AbsoluteSpearman,Slope,Intercept,RSquared,MeanAbsoluteDifference,P95AbsoluteDifference,MaxAbsoluteDifference,ResidualMeanAbsolute,ResidualMaxAbsolute}. Applies to RelationMetrics or RelationReport<K>. |

## Anomaly, jump and temporal lag

- `OutlierConfig{threshold:f64,zero_mad:ZeroMadPolicy}` defaults 3.5/ExactMedian; threshold finite nonnegative. modified-z = 0.6744897501960817*(value−median)/MAD, strict magnitude threshold. `ZeroMadPolicy::{ExactMedian,AbsoluteTolerance(f64)}`: zero MAD flags every nonmedian or deviations strictly beyond finite nonnegative tolerance in original units. OutlierReport{config,summary:RobustSummary,suspicious:Vec<Outlier{index,value,score}>}; `RobustScore::{ModifiedZ(f64),ZeroMadDeviation{signed_deviation}}`.
- `JumpConfig{absolute_threshold:Option<f64>,relative_threshold:Option<f64>,denominator_floor:f64}`: thresholds finite nonnegative, floor finite positive; both None measures maxima without flagging. Source order matters; either strict threshold flags; relative=abs(delta)/max(abs(previous),floor). JumpReport{config,record_count,transition_count,maximum_absolute,maximum_relative,suspicious}; Jump{from_index,to_index,from_value,to_value,signed_change,absolute_change,relative_change}. Earliest maximum wins ties. AtMostAnomalies bounds suspicious counts for either report.
- `LagConfig{range:LagRange{start:i64,end:i64},measure:LagMeasure::{Pearson,Spearman}}`: inclusive start<=end, at most `MAX_LAG_CANDIDATES=4097`. Positive d pairs **left k with right k+d**, never vector offsets; overflow-safe shifts, duplicate keys error. Best maximizes absolute correlation, ties smallest |d| then signed d. One unique temporal domain/search.
- LagReport{config,left_source/right_source:Option<String>,left_count,right_count,candidates,best}, `zero_lag()->Option<&LagCandidate>`. LagCandidate{lag,alignment:LagAlignment,relation}; LagAlignment{pair_count,boundary_left,boundary_right,missing_left,missing_right}, `has_interior_gaps()` distinguishes expected boundary loss/interior gaps. BestLag{lag,relation,strength}; best=None if all undefined. LagExpectation rejects `LagViolation::{UnexpectedBestLag,NoDefinedCandidate,InteriorMissingKeys}`.

## Direct analysis and report/error reference

Operations return data/errors, never assertions. Cloneable iterators must replay identical ordered values; lazy selectors must be pure. Reports/errors support Serde.

| Function/type | Public surface/meaning |
| --- | --- |
| NumericSeries | `new(Iterator<Item=f64>+Clone)`, `iter()`; `finite_count()->FiniteCount{count,finite_count}` does not reject invalids. `summary()/quantile(q)/robust_summary()/check(NumericRule)/outliers(config)/jumps(config)` → Result<report,NumericError>. Lazy selectors may rerun; prefer PreparedNumeric for reuse. |
| Summary; RobustSummary | Summary{count,finite_count,min,max,sum,mean,variance(population),sample_variance:Option<f64>(n<2 absent),standard_deviation}; RobustSummary{count,median,q1,q3,iqr,mad(unscaled)}. Type-7 interpolation; empty/nonfinite/overflow/underflow errors; q finite in [0,1]. |
| Integrity functions | `check_count(actual,CountRule)`, `check_unique(slice,key)`, `check_contiguous(slice,u64_key,start,end)` → IntegrityReport{record_count,issues}, `is_valid()`. IntegrityIssue::{Count{expected,actual},Duplicate(KeyGroup),UnexpectedKey{key},MissingRange{start,end},InvalidKeyRange{start,end},Numeric{index,value,expected}}. Aggregate via fact(...).expect_valid(). |
| Comparison<K> | `check_same_keys(left,right,lk,rk)` → exact unique correspondence. Fields left_count/right_count, matched:Vec<MatchedKey{key,left_index,right_index}>, missing_left/missing_right/duplicate_left/duplicate_right:Vec<KeyGroup{key,indices}>, optional left_source/right_source labels. `is_complete()` includes duplicates; duplicate keys never pair. |
| Join | `join_by(left:&[L],right:&[R],lk,rk) -> Join<L,R,K>` (K:Ord+Clone), `join_u64_by(...)`, `join_sorted_by(...) -> Result<Join,JoinError>`. `comparison()->&Comparison<K>`, `pairs()->Iterator<(&K,&L,&R)>`, `require_complete()->Result<(),&Comparison<K>>`, `set_source_names(&str,&str)`. JoinError{side:Side::{Left,Right},index}. |
| Join numeric | `difference(lp,rp,top)->Result<DifferenceReport<K>,RelationError<K>>`; `numeric_relation(lp,rp,top)->Result<RelationReport<K>,RelationError<K>>`. f64 selectors, Clone for numeric_relation. Direct owned reports copy full comparison; prefer prepared comparisons for repeated checks. |
| Pair analysis | `linear_metrics(pairs)/pearson(pairs)/spearman(pairs)` accept IntoIterator<(f64,f64)> → Result<LinearMetrics/Metric/Metric,NumericError>; `correlation(cloneable_pairs)/linear_fit(cloneable_pairs)/numeric_relation(cloneable_pairs)` → corresponding metrics. Cloneable_pairs is Iterator+Clone. `difference_summary(IntoIterator<(f64,f64)>)` → DifferenceSummary. |
| LinearMetrics; Metric | LinearMetrics{count,pearson,covariance(population),sample_covariance,slope,intercept,r_squared}; OLS right on left. Spearman average tied ranks, including signed zero. `Metric::{Defined(f64),Undefined(UndefinedReason::{InsufficientPairs,ConstantLeft,ConstantRight})}`, `value()->Option<f64>`; no silent NaN. |
| DifferenceSummary; pair reports | DifferenceSummary{signed:Summary,absolute:Summary,median_signed,median_absolute,p95_absolute}; direct right−left. Fit residuals = observed right−predicted right. RelationMetrics{linear,spearman,difference,residual:Option<DifferenceSummary>}; RelationReport<K>{comparison,observed,largest_disagreements}; DifferenceReport<K>{summary,largest}; PairEvidence<K>{key,left,right,signed_difference,absolute_difference}. |
| Direct lag | `lag_relation_by(left,right,lp,rp,config)->Result<LagReport,LagError>`: slices, one (i64,f64) projection/row. LagError::{InvalidRange,DuplicateKeys{left,right},Projection{side,error},Numeric{lag,error}}. |
| Numeric/error values | NumericError::{Empty,NonFinite{index,value},InvalidParameter{name,value},ArithmeticOverflow{operation},ArithmeticUnderflow{operation}}. NumericValue::{Finite(f64),Nan,PositiveInfinity,NegativeInfinity}, From<f64>, JSON-safe. RelationError<K>::{Comparison(Box<Comparison<K>>),Numeric{comparison,error}}. |

Low-level `RelationExpectation::evaluate(RelationReport<K>)->Result<RelationReport<K>,Box<RelationFailure<K>>>` and `LagExpectation::evaluate(LagReport)->Result<LagReport,Box<LagFailure>>` delegate to canonical acceptance. Failures{expected,violated,observed} implement Display/Error; RelationFailure also retains analysis_error for invalid evidence/preconditions. Prefer Fact::expect for collect-all; never unwrap evaluate as intermediate assertion. No separate assert_* authority.

## Hierarchy, report and final assertion

`certify(name:Into<String>) -> Certification`. Methods: `source(name,&T)/collection(name,&[T])` above; `check(name,CheckResult)`; `check_fact_expect(name,value,expected)` (exactly check(name,fact(name,value).expect(expected))); `section(name,FnOnce(&mut Certification))`; `limits(DiagnosticLimits)`. Check/section/limits return &mut Self. Eager collect-all, nested semantic paths (encoder/prediction/timing/), no independent short-circuiting. Fallible dependent preparation uses try_fact, not ? out of the entire proof.

`Certification::certify() -> CertificationReport` is a nonpanicking bounded snapshot: public name/checks/failed/failures, `is_pass()`. CheckFailure{section,name,class,diagnostic}. Both `Certification::assert_certify()` and `CertificationReport::assert_certify()` track caller; failure panics with one report, success silent. Report Display is empty on success.

`DiagnosticLimits{failures,characters_per_failure}` defaults 64/2048 UTF-8 bytes, plus truncation marker and bounded names/paths; omitted failures remain counted. Success aggregation retains no fact report/duplicate comparison. Bounded writers stop traversal when formatters propagate errors; custom Debug must propagate write errors and avoid unbounded allocation/work before writing (a writer cannot interrupt ignored errors). `bounded_debug(&value,limit)` supports custom detail. Keep full typed reports separately; never inject unbounded JSON into panics.

## Ordinary + Observation-backed composition

`CapturedRun::certify` is the completeness-aware data-source bridge. Start every capture proof with **run.certify(name)**, not standalone certify: eviction, emission failure or escaped contexts/workers/spans adds an EvaluationError prerequisite; healthy capture adds no extra check. A retained subset never establishes completeness. Capture mechanics belong to the sun-observations skill. Decode TypedView once; reuse its source. Filter proc/scope or include execution identity for repeated keys. Provenance optional; ordinary sources work identically.

```rust
use sun_certifications::*;
fn ordinary() {
    let returned = 3usize;
    let rows = [1u64, 2, 3];
    let mut c = certify("canonical result and artifact");
    let result = c.source("result", &returned);
    let data = c.collection("artifact", &rows);
    c.section("result", |c| { c.check("ready", result.select(|v| *v).expect(3usize)); });
    c.check("unique", data.unique(|v| *v).expect_valid());
    c.check("contiguous", data.contiguous(|v| *v, 1, 3).expect_valid());
    c.check("forbidden future", data.matching_count(|v| *v > 3).expect(0));
    c.assert_certify();
}
```

```rust
use sun_certifications::*;
use sun_observations_data_source::{CapturedRun, DecodeError};
use serde::{Serialize, Deserialize};
use sun_observations::Observation;
#[derive(Serialize,Deserialize,Observation)]
#[observation(namespace="encoder::prediction")]
struct Prediction { fold:u8, row:u64, signal:f64 }
struct Output { fold:u8, row:u64, signal:f64 }
fn mixed(run:&CapturedRun<Vec<Output>>) -> Result<(),DecodeError> {
    let typed = run.typed::<Prediction>()?;
    let mut c = run.certify("internal transfer to canonical result");
    let internal = typed.source("predictions");
    let result = c.collection("output", run.result());
    let pair = internal.compare_with(&result).on(|v|(v.fold,v.row), |v|(v.fold,v.row));
    let signal = pair.numeric(|v|v.signal, |v|v.signal);
    c.check("keys", pair.comparison_fact().expect_valid());
    c.check("exact transfer", pair.equal(|v|v.signal, |v|v.signal, 5).expect_valid());
    c.check("association", signal.correlation().expect(CorrelationExpectation::positive(0.9).unwrap()));
    c.check("difference", signal.difference().expect(DifferenceExpectation::default().max_absolute(0.).unwrap()));
    c.assert_certify();
    Ok(())
}
```
