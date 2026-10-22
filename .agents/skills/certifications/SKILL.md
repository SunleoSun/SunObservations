---
name: certifications
description: Certify semantic Rust contracts over ordinary values, collections, artifacts, reference results and optional Observation evidence with sun-certifications; use named prepared sources, typed-key comparison, statistics, expectations and hierarchical collect-all bounded diagnostics.
---

# Semantic certification methodology and API

## Methodology

**Certify semantic behavior, not implementation shape.** A `#[test]` is an execution mechanism, not the philosophy. Scope each proof to the narrowest meaningful semantic owner. Prefer `<owner>_certification.rs`; cross-owner files may be `prediction_decision_certification.rs` or `backend_parity_certification.rs`.

1. Prefer evidence in this order: canonical returned result/persisted artifact; promised external effect/state; typed production Observation; independent reference/oracle; statistical/anomaly evidence. Do not duplicate an authoritative returned value with an Observation.
2. Define what **must happen** and what **must never happen**. Hard invariants are logically required; statistical/anomaly expectations are deterministic scenario-owned acceptance, not warnings. Either violation fails the proof. Thresholds do not belong on production Observation types.
3. Verify negative paths beyond `is_err()`: correct failure category, rejection at owning boundary, no forbidden downstream event/artifact/effect, explanatory diagnostic evidence, independent healthy paths where relevant.
4. Trace `input → producer boundary → consumer input → decision → canonical result/effect`. Use explicit typed domain identity, including execution identity if keys repeat across runs. Never join by insertion order or untyped field-name strings. Composite tuples or dedicated ID structs are canonical.
5. Prove causality: source/output/consumer time or row, horizon, decision and entry offsets. Final quality alone does not prove alignment. Assert known offsets exactly; use lag to diagnose unknown stale/off-by-one/future relations.
6. Global success may hide broken folds/targets/directions/episodes/models: use grouped evidence. After exact invariants, strengthen with correlation, monotonicity, slope/intercept, residuals, robust outliers, jumps, lag and distribution/trend criteria. This is not limited to ML.
7. Use a simple independent oracle for optimized/math-heavy paths. Do not copy the optimized algorithm into its oracle. Use metamorphic/control scenarios: partition changes preserve outputs; shuffle destroys relation; direction inversion flips meaning; known scaling transforms outputs predictably.
8. Prefer production evidence over test-only visibility. `#[cfg(test)]` is appropriate for fixtures, generators, simple oracles, mocks at genuine typed boundaries, properties and compile scaffolding—not internal getters/counters/callbacks or alternate production branches solely for inspection. Evidence-design roles: **Boundary** semantic transfer first; **Decision** meaningful branching next; **Diagnostic** only for a concrete certification/diagnosis. When internal production evidence is required, use `.agents/skills/sun-observations/SKILL.md` to design and emit it.
9. Collect independent failures in one named hierarchy. Do not unwrap a failed acceptance check or stop after the first ordinary criterion. Model unavailable dependent analysis as EvaluationError; unrelated sections continue. Failure reports must localize contract/name/source/expected/observed/counts/bounded examples without raw-log archaeology.
10. Use the framework for repository behavior verification. Direct Rust assertions are exceptions for framework/bootstrap/self-tests and low-level compile/process infrastructure where self-dependence or indirection obscures the mechanism. Successful final assertions are silent.

## Evidence plan before implementation

Plan the contract, canonical outcome, Boundary / Decision / Diagnostic evidence, required and absent events, keys/alignment/causality, statistical expectations, negative/control scenario, independent oracle/metamorphic evidence, and failure diagnostics. Keep the plan proportional to the contract. Prefer deterministic fixtures. For legitimately concurrent scenarios, verify invariant membership, counts, uniqueness and outcomes rather than incidental execution order.

## Dependencies and ownership

sun-certifications is a Rust 2021 crate with no SunObservations dependency. Use the dependency source already configured by the consuming workspace and keep related SunObservations crates on a compatible release or revision; never invent a local checkout path when source is unavailable. The normal public API is imported from sun_certifications. Ordinary returned values, inputs and outputs, slices, loaded artifacts and independent references use identical generic APIs. Optional sun-observations-data-source adapters supply named borrowed evidence with provenance; decoding, query and envelope rules stay there.

Canonical flow:

`source → fact / Fact → expect → CheckResult → check → Certification → assert_certify`

Analysis is neutral. `expect` never panics for a failed criterion. `assert_certify` is the final assertion boundary and tracks its caller.

## Ordinary scalar, struct, collection and absence

```rust
use sun_certifications::*;
#[derive(Debug)]
struct ResultValue { ready: usize, score: f64 }
fn ordinary() {
    let returned = ResultValue { ready: 3, score: 1.5 };
    let rows = [1u64, 2, 3];
    let mut c = certify("ordinary returned contract");
    let result = c.source("result", &returned);
    let data = c.collection("loaded artifact", &rows);
    c.section("result", |c| {
        c.check_fact_expect("ready", returned.ready, 3usize);
        c.check("finite score", result.select(|r| r.score).expect(NumericRule::Finite));
        c.check("score range", result.select(|r| r.score).expect(Criterion::Range { min: 1., max: 2. }));
    });
    c.section("artifact", |c| {
        c.check("nonempty", data.count().expect(CountRule::NonEmpty));
        c.check("unique", data.unique(|r| *r).expect_valid());
        c.check("contiguous", data.contiguous(|r| *r, 1, 3).expect_valid());
        c.check("forbidden future row", data.matching_count(|r| *r > 3).expect(0));
    });
    c.assert_certify();
}
```

Plain expected values / `NotEqual(value)` work for any Debug+PartialEq struct/collection, not just ordered scalars. Select fields for independent failures. Keep scalar input alive until borrowed prepared source/fact usage finishes.

## Prepared sources: complete normal-use surface

`PreparedSource<'a,T>` borrows `&'a [T]`; cloning shares slices/name/provenance, never payloads. Names accept Into<Arc<str>>. Projections/selectors must express semantic fields; ordinary sources need no Serde or Observation implementation.

| API | Result and semantics |
| --- | --- |
| `source(name, &T)` / `c.source(name,&T)` | Singular PreparedSource using a one-element slice. A `&Vec<T>` here is **one Vec value**, not its rows. |
| `source_slice(name,&[T])` / `c.collection(name,&[T])` | Collection PreparedSource; works with Vec/array slice coercions. Typed handles, no erased heterogeneous registry. Registration does not borrow c. |
| `name() -> &str`; `values() -> &'a [T]` | Named identity and original borrowed values. |
| `entry(index) -> SourceEntry<'a,T>`; `iter()` | Entry fields `value:&T,index:usize,provenance:Option<&dyn SourceProvenance>`. Iterator exact/double-ended/cloneable. Bounds on entry are ordinary Rust indexing preconditions. |
| `with_provenance(&'a dyn SourceProvenance) -> Self` | Adapter context; implement `describe(index, out:&mut dyn fmt::Write)->fmt::Result`. Never infer generic semantics from formatted coordinates. |
| `select(FnOnce(&T)->V) -> Fact<V>` | Exactly one value required; otherwise EvaluationError, not panic and selector is not called. |
| `project(Fn(&T)->V) -> PreparedValues<V>` | Materializes one projection per source row. `values() -> &[V]` and `source() -> PreparedSource<V>` reuse it. This owner does not carry external provenance; retain original source-local indices for lookup. |
| `count() -> Fact<usize>`; `matching_count(predicate)` | Count all or selected events; absence = expect(0usize), nonempty = CountRule::NonEmpty. |
| `unique(key:Fn(&T)->K)` | Fact<IntegrityReport<K>>, K:Ord+Clone. |
| `contiguous(u64_key,start,end)` | Fact<IntegrityReport<u64>>; inclusive expected range; compact missing ranges. |
| `numeric(Fn(&T)->f64) -> PreparedNumeric` | Eager numeric projection once; `values()` exposes &[f64], `series()` exposes a borrowed NumericSeries. Repeated summary/robust/check operations do not invoke projection again. |
| `keys(Fn(&T)->K) -> PreparedKeys<'a,T,K>` | Projects/sorts once, K:Ord+Clone. `indices() -> &[(K,usize)]`; `left_keys.compare_with(&right_keys)` merges already prepared indices, useful with multiple counterpart sources. |
| `group_by(Fn(&T)->K) -> GroupedView<T,K>` | K:Ord, no Clone needed for membership. Ascending key groups, original source order within group. |
| `compare_with(&other) -> ComparisonBuilder<L,R>` | Await explicit typed key correspondence (below). |
| `lag_relation_by(&other,left_selector,right_selector,LagConfig)` | Fact<LagReport>; each selector yields (i64 key,f64 value), projected once per search. |

`PreparedNumeric` methods all return typed `Fact`: `summary()`, `robust_summary()`, `quantile(q:f64)`, `check(NumericRule)`, `outliers(OutlierConfig)`, `jumps(JumpConfig)`. Projections need not be cloned; numeric/analysis scratch is separate from the cached values. Source-local indices in reports can be resolved through the original source.

## Composite-key correspondence and reuse

```rust
use sun_certifications::*;
#[derive(Debug)]
struct Row { fold:u8, target:u8, row:u64, signal:f64, strength:f64 }
fn correspondence(left:&[Row], right:&[Row]) {
    let a = source_slice("predictions", left);
    let b = source_slice("decisions", right);
    let pair = a.compare_with(&b).on(
        |l| (l.fold,l.target,l.row), |r| (r.fold,r.target,r.row));
    let signal = pair.numeric(|v| v.signal, |v| v.signal);
    let strength = pair.numeric(|v| v.strength, |v| v.strength);
    let mut c = certify("prediction -> decision");
    c.check("comparison_fact", pair.comparison_fact().expect_valid());
    c.check("target", pair.equal(|v| v.target, |v| v.target, 5).expect_valid());
    c.check("signal", signal.correlation().expect(CorrelationExpectation::positive(0.90).unwrap()));
    c.check("strength", strength.correlation().expect(CorrelationExpectation::positive(0.85).unwrap()));
    c.check("custom transfer", pair.check_pairs(|l,r| (l.signal-r.signal).abs() <= 0.1, 5).expect_valid());
    c.assert_certify();
}
```

A dedicated key `#[derive(Clone,Debug,Eq,PartialEq,Ord,PartialOrd)] struct Key(FoldId,TargetId,u64);` works identically. Left/right element types may differ. No Hash requirement or string field-name join.

| API | Result/semantics |
| --- | --- |
| `builder.on(lk,rk)` | PreparedComparison<'a,L,R,K>, K:Ord+Clone. Sorts only if needed, then merges. |
| `on_u64(lk,rk)` | u64-key optimized sorted/bounded-dense path, same ambiguity semantics. |
| `on_sorted(lk,rk)` | Result<PreparedComparison,JoinError>; rejects decreasing keys, never silently reorders a promised sorted input. |
| `PreparedComparison::name() -> String` | Named left -> right identity. Field/check names belong to checks. Normal methods require K:Clone+Debug. |
| `comparison() -> &Comparison<K>` | Full reusable mapping. `left_count,right_count,matched:Vec<MatchedKey<K>>,missing_left,missing_right,duplicate_left,duplicate_right:Vec<KeyGroup<K>>`; optional left_source/right_source diagnostic labels. MatchedKey has key/left_index/right_index. KeyGroup has key/indices. Duplicate keys never pair. |
| `pairs()` | Iterator of (&K,SourceEntry<L>,SourceEntry<R>) for deliberate subset investigation; preserves original source-local/provenance identity. |
| `comparison_fact() -> Fact<&Comparison<K>>` | Borrowed comparison, no giant report copy. `expect_valid()` requires exact unique key correspondence, includes bounded missing/duplicate evidence. |
| `equal(lp,rp,top:usize)` | Fact<PairCheck<K>>; V:PartialEq projections. `check_pairs(Fn(&L,&R)->bool,top)` is custom equivalent. Report matched/failures/examples; diagnostic examples cap at min(top,32), with available provenance; mismatch counts precede truncated examples. Incomplete comparison yields EvaluationError, never silently proves a matched subset. |
| `numeric(lp,rp) -> PreparedNumericComparison<K>` | Caches projected f64 pairs once in prepared comparison order; no new join. `values() -> &[(f64,f64)]`. |
| Numeric pair `correlation()/linear_fit()/difference()/largest(top)` | Fact<CorrelationMetrics/LinearFitMetrics/DifferenceSummary/Vec<PairEvidence<K>>>. All require complete comparison. No comparison copy into each fact. largest uses bounded top-N scratch and deterministic key-order ties. |
| `lag(lp,rp,config)` | Fact<LagReport>, selectors (i64,f64). Reuses decoded sources and projects once per search, not per candidate. Shifted-domain sort differs from zero-lag mapping. Use one unique temporal domain per search; partition by fold/target/execution first if row keys repeat. |

Reuse one pair for structural comparison/equality/predicates and multiple numeric fields; reuse each numeric field pair for differences, Pearson, Spearman, covariance/fit, residuals and top disagreements. Statistics may allocate their own sorting scratch (e.g. ranks/quantiles); they do **not** rebuild domain correspondence. A separate lag search projects/sorts its temporal keys once per call.

## Correlation, linear fit and difference

Prepare one numeric pair with pair.numeric(left_project,right_project). It owns projected f64 pairs once, borrows the same typed comparison, and caches linear moments, ranks, differences and residual results (including failures). Repeated semantic views neither decode, project, sort keys nor rebuild correspondence. Only the first request computes each numeric analysis; later checks clone compact summaries, not traces.

| Operation | Fact and acceptance |
| --- | --- |
| `numeric.correlation()` | Fact<CorrelationMetrics> with observed pearson/spearman. CorrelationExpectation::positive(min_spearman) / negative(max_spearman) return Result; positive (0,1], negative [-1,0). default().pearson(min:Option<f64>,max:Option<f64>) / spearman(min,max); bound(CorrelationMeasure::{Pearson,Spearman,AbsolutePearson,AbsoluteSpearman},min,max). |
| `numeric.linear_fit()` | Fact<LinearFitMetrics> with observed linear:LinearMetrics and residual:Option<DifferenceSummary>. .slope()/.intercept()/.r_squared() on the fact yield Fact<f64>, undefined metrics are EvaluationError. LinearFitExpectation::default().slope(min,max)?.intercept(min,max)?.r_squared(min)?.residual_mean_absolute(max)?.residual_max_absolute(max)?; bound(LinearFitMeasure,min:Option<f64>,max:Option<f64>) for explicit bounds. |
| `numeric.difference()` | Fact<DifferenceSummary>, right-minus-left, never fitted residuals. DifferenceExpectation::default().mean_absolute(max)?.p95_absolute(max)?.max_absolute(max)?; bound(DifferenceMeasure::{MeanAbsolute,P95Absolute,MaxAbsolute},min,max). |

All bounds are inclusive and finite; reversed/nonfinite bounds return NumericError. Required undefined/nonfinite metrics yield EvaluationError. Failed valid bounds yield StatisticalViolation with typed MetricConstraint<M> violations. Use close_to for compact scalar deterministic proximity. Standalone correlation(cloneable_pairs) / linear_fit(cloneable_pairs) / difference_summary(pairs) return the same reports without preparing sources.

An advanced composite numeric.relation() / numeric_relation(pairs) returns RelationMetrics{linear,spearman,difference,residual}. RelationExpectation::positive/negative/default().bound(RelationMeasure,min,max) supports combined correlation/fit/difference/residual investigation, and RelationConstraint retains violated criteria. RelationMeasure includes Pearson, Spearman, AbsolutePearson, AbsoluteSpearman, Slope, Intercept, RSquared, MeanAbsoluteDifference, P95AbsoluteDifference, MaxAbsoluteDifference, ResidualMeanAbsolute, ResidualMaxAbsolute. Use the separate semantic views for ordinary checks; the composite reuses their canonical computations.

## Grouped/statistical/anomaly workflows

```rust
use sun_certifications::*;
fn grouped_and_lag(left:&[(u8,i64,f64)], right:&[(u8,i64,f64)]) {
    let a=source_slice("left",left); let b=source_slice("right",right);
    let groups=a.group_by(|v|v.0);
    let fields=groups.numeric(|v|v.2);
    let mut c=certify("group and timing");
    c.check("per group", fact("groups",fields.summaries_with_robust())
        .expect(GroupExpectation{minimum_count:3,mean_min:0.,mean_max:100.}));
    let numeric=a.numeric(|v|v.2);
    c.check("outliers",numeric.outliers(OutlierConfig{threshold:3.5,zero_mad:ZeroMadPolicy::AbsoluteTolerance(0.01)})
        .expect(AtMostAnomalies(2)));
    c.check("jumps",numeric.jumps(JumpConfig{absolute_threshold:Some(10.),relative_threshold:Some(0.5),denominator_floor:1.})
        .expect(AtMostAnomalies(0)));
    c.check("known offset",a.lag_relation_by(&b,|v|(v.1,v.2),|v|(v.1,v.2),
        LagConfig{range:LagRange{start:-2,end:2},measure:LagMeasure::Pearson})
        .expect(LagExpectation{best_lag:0}));
    c.assert_certify();
}
```

The lag example requires globally unique row keys; grouped lag requires a separate search for each group, not flattening repeated row identities.

- `GroupedView::iter()` yields Group with `key:&K,len(),is_empty(),values(),iter()` (SourceEntry) and lazy `numeric(project)->NumericSeries`. `groups.numeric(project) -> GroupedNumeric` caches one f64 per source row; `summaries()/summaries_with_robust()` reuse that cache. K:Ord+Clone for owned summary keys.
- `GroupedSummaryReport<K>.groups` contains `GroupSummary{key,member_count,summary:Result<Summary,NumericError>,robust:Option<Result<RobustSummary,NumericError>>}`. Failures retain identity and do not hide healthy groups. GroupExpectation fails statistically for count/mean violations, errors for unavailable summaries/invalid bounds, and rejects zero groups.
- `OutlierConfig{threshold:f64,zero_mad:ZeroMadPolicy}` defaults 3.5/ExactMedian. Nonnegative finite threshold; modified-z = 0.6744897501960817*(value−median)/MAD, strictly greater magnitude flags. `ZeroMadPolicy::{ExactMedian,AbsoluteTolerance(f64)}`; zero MAD flags every nonmedian, or deviations strictly beyond nonnegative finite tolerance in original units. OutlierReport has config/RobustSummary/suspicious Vec<Outlier{index,value,score}>; `RobustScore::{ModifiedZ(f64),ZeroMadDeviation{signed_deviation}}`.
- `JumpConfig{absolute_threshold:Option<f64>,relative_threshold:Option<f64>,denominator_floor:f64}` accepts optional finite nonnegative thresholds and requires a positive finite floor; both None measures maxima without flagging transitions. Current source order matters: either strict threshold flags; relative=abs(delta)/max(abs(previous),floor). JumpReport has config/record_count/transition_count/maximum_absolute/maximum_relative/suspicious; Jump has from/to indices/values, signed_change/absolute_change/relative_change. Earliest maximum wins ties.
- `LagRange{start:i64,end:i64}` inclusive, start<=end, at most `MAX_LAG_CANDIDATES=4097`; `LagMeasure::{Pearson,Spearman}`. Positive d pairs **left k with right k+d**, never vector offsets. Best maximizes absolute correlation; ties prefer smallest |d| then signed d. Overflow-safe shifting. Duplicates error. LagReport fields config/optional left_source and right_source labels/left_count/right_count/candidates/best, `zero_lag()->Option<&LagCandidate>`. Candidate has lag/alignment/relation. `LagAlignment{pair_count,boundary_left,boundary_right,missing_left,missing_right}` distinguishes expected boundary loss from interior gaps; `has_interior_gaps()`. BestLag has lag/relation/strength. `best=None` if all undefined. LagExpectation rejects unexpected/undefined/interior-incomplete best; violations `UnexpectedBestLag,NoDefinedCandidate,InteriorMissingKeys`.

## Fact, expectation and typed results

| API/type | Contract |
| --- | --- |
| `fact(source:Into<String>,value:T) -> Fact<T>` | Successfully observed evidence, independent of acceptance. |
| `try_fact(source,Result<T,E:Display>) -> Fact<T>` | Fallible evidence; preserves bounded AnalysisError{message:String}. Direct analysis retains its original typed error. |
| `Fact{source,observed:Result<T,AnalysisError>,details:String}` | Neutral evidence; details(text) adds diagnostics. |
| `fact.expect(E:Expectation<T>) -> CheckResult<T,E::Violation>` | One acceptance owner; consumes without cloning. Failed expectations return data, never panic. |
| `fact.expect_valid()` | Canonical structural validity: delegates to AllValid for IntegrityReport, borrowed Comparison or PairCheck. |
| `CheckResult<T,V=Violation>` | Public source/expected/observed/violations/class/analysis_error/details; is_pass(). Full typed observed value survives expectation failure; failed preconditions populate analysis_error while observed remains Ok. |
| `CheckClass::{Pass,HardViolation,StatisticalViolation,EvaluationError}` | Pass proves acceptance. HardViolation = evaluated deterministic semantic mismatch. StatisticalViolation = evaluated scenario statistical/anomaly mismatch. EvaluationError = invalid/unavailable/incomplete evidence, bounds or evaluation. Every non-Pass fails final proof. |
| `fact("count",count).expect(64usize)` | Plain T:Debug+PartialEq means equality, including bool, enum, &str, String, Option and collections; an &str also expects an owned String. Keep Equal<T> for advanced explicit equality; NotEqual<T> for inequality. Specialized expectations share the same expect boundary. |
| `close_to(expected:f64)` | Default absolute=relative=1e-9: abs(actual-expected) <= absolute + relative*abs(expected). .absolute(tolerance)/.relative(tolerance) override. Nonfinite evidence/expected/tolerance/arithmetic or negative tolerances yield EvaluationError. |
| `Criterion::{Equal(T),NotEqual(T),Greater(T),AtLeast(T),Less(T),AtMost(T),Range{min:T,max:T}}` | T:Debug+PartialOrd; inclusive range. Invalid/incomparable/reversed bounds yield EvaluationError. |
| `CountRule::{Exact(usize),Minimum(usize),NonEmpty}` | Hard count acceptance; plain expected usize is concise exact equality. |
| `NumericRule::{Finite,Range{min:f64,max:f64},Positive,NonNegative}` | Finite evidence required, including sign checks. Invalid bounds are evaluation errors. |
| `AllValid` | Advanced generic hard structural expectation; prefer expect_valid(). |
| `LagExpectation{best_lag:i64}` | Statistical acceptance, including defined best and no interior gaps. |
| `AtMostAnomalies(usize)` | Statistical outlier/jump-count bound. |
| `GroupExpectation{minimum_count,mean_min,mean_max}` | Statistical group bounds; missing summaries/invalid bounds are EvaluationError. |
| `criterion(description,CheckClass::HardViolation or CheckClass::StatisticalViolation,Fn(&T)->bool)` | Custom Predicate. False returns Violation{expected:String}. Pass/EvaluationError are invalid criterion classifications. |
| `Expectation<T>` | Extension trait: associated Violation:Debug, assess(&T)->Result<Vec<Violation>,AnalysisError>, describe()->String, class()->CheckClass (default HardViolation). One owner per domain acceptance rule. |

No conditional DSL: use normal Rust control flow to choose expectations.

### Direct analysis and result inspection (no final assertion required)

```rust
use sun_certifications::*;
fn inspect() -> Result<(), NumericError> {
    let report=numeric_relation([(1.,5.),(2.,7.),(3.,9.)].into_iter())?;
    let expectation=RelationExpectation::positive(0.99)?
        .bound(RelationMeasure::Slope,Some(1.99),Some(2.01))?;
    let comparison=fact("reference -> output",report).expect(expectation);
    let _full_report=comparison.observed.as_ref();
    let _typed_violations=&comparison.violations;
    let _status=comparison.class;
    let mut c=certify("optional aggregation");
    c.check("affine semantics",comparison);
    let _nonpanicking_report=c.certify();
    Ok(())
}
```

## Standalone analysis and report reference

All public analysis/comparison operations return data/errors, not assertions. Iterator clones must replay the same ordered values; projection closures for lazy APIs must be pure.

| Function/type | Signature shape and useful report |
| --- | --- |
| `NumericSeries::new(iter:Iterator<Item=f64>+Clone)` | `iter()` replays; `finite_count()->FiniteCount{count,finite_count}` does not reject invalids. `summary()->Result<Summary,NumericError>`; `quantile(q)->Result<f64,_>`; `robust_summary()->Result<RobustSummary,_>`; `check(NumericRule)->Result<IntegrityReport,_>`; `outliers(config)/jumps(config)`. Lazy selectors may rerun—PreparedNumeric caches them. |
| Summary | count/finite_count/min/max/sum/mean/variance(population)/sample_variance Option (n<2 absent)/standard_deviation. RobustSummary: count/median/q1/q3/iqr/mad (unscaled), type-7 interpolation. Empty/nonfinite/overflow/underflow fail explicitly. q finite in [0,1]. |
| `check_count(actual,CountRule)`; `check_unique(slice,key)`; `check_contiguous(slice,u64_key,start,end)` | IntegrityReport{record_count,issues}, `is_valid()` for investigation; aggregate with `fact(...).expect_valid()`. Issues: Count{expected,actual}, Duplicate(KeyGroup), UnexpectedKey{key}, MissingRange{start,end}, InvalidKeyRange{start,end}, Numeric{index,value,expected}. |
| `check_same_keys(left,right,lk,rk) -> Comparison<K>` | Exact unique key-set correspondence using the canonical generic join. |
| `join_by(left:&[L],right:&[R],lk,rk) -> Join<L,R,K>` | K:Ord+Clone; generic adaptive sorted merge. `join_sorted_by(...) -> Result<Join,JoinError>`; `join_u64_by(...) -> Join<...,u64>`. JoinError{side:Side::Left/Right,index}. |
| Join | `comparison()->&Comparison<K>`, `pairs()->Iterator<(&K,&L,&R)>`, `require_complete()->Result<(),&Comparison<K>>`, `set_source_names(&str,&str)` diagnostic labels. `comparison.is_complete()` includes duplicates. |
| Join numeric | `difference(lp,rp,top)->Result<DifferenceReport<K>,RelationError<K>>`; `numeric_relation(lp,rp,top)->Result<RelationReport<K>,RelationError<K>>`. lp/rp f64 selectors (Clone required for numeric_relation). Direct reports copy full comparison for owned serializable investigation; prefer PreparedComparison for repeated checks. |
| `linear_metrics(cloneable_pairs)/pearson(cloneable_pairs)/spearman(cloneable_pairs)` | Input Iterator<(f64,f64)>+Clone; Result<LinearMetrics/Metric/Metric,NumericError>. Linear: count/Pearson/population covariance/sample covariance/slope/intercept/R², ordinary least squares right on left. Spearman average tied ranks including signed-zero ties. |
| `difference_summary(IntoIterator<(f64,f64)>)` | Result<DifferenceSummary,NumericError>. Right−left; signed/absolute Summary, median_signed/median_absolute/p95_absolute. |
| `numeric_relation(cloneable_pairs)` | Result<RelationMetrics,NumericError>; linear/Spearman/difference/residual Option (observed right−predicted right). |
| Metric | `Defined(f64)` or `Undefined(UndefinedReason::{InsufficientPairs,ConstantLeft,ConstantRight})`; `value()->Option<f64>`. No silent NaN. |
| RelationReport<K> | comparison/observed RelationMetrics/largest_disagreements Vec<PairEvidence<K>>. DifferenceReport<K>: summary/largest. PairEvidence: key/left/right/signed_difference/absolute_difference. Top disagreements decrease by absolute error, stable key-order ties. |
| `lag_relation_by(left,right,lp,rp,config)->Result<LagReport,LagError>` | Slices/one (i64,f64) projection per row; LagError: InvalidRange, DuplicateKeys{left,right}, Projection{side,error}, Numeric{lag,error}. |
| NumericError/NumericValue | NumericError: Empty, NonFinite{index,value}, InvalidParameter{name,value}, ArithmeticOverflow{operation}, ArithmeticUnderflow{operation}. NumericValue: Finite(f64), Nan, PositiveInfinity, NegativeInfinity, From<f64>; Serde-safe. RelationError<K>: Comparison(Box<Comparison>), Numeric{comparison,error}. |

Analysis reports/errors support Serde. Existing direct `RelationExpectation::evaluate(RelationReport<K>) -> Result<RelationReport<K>,Box<RelationFailure<K>>>` and `LagExpectation::evaluate(LagReport) -> Result<LagReport,Box<LagFailure>>` delegate to the same acceptance owner; failures retain expected/violated/observed and implement Error. RelationFailure also preserves analysis_error for invalid evidence or expectation preconditions. Prefer `Fact::expect` for collect-all proofs; never unwrap evaluate as an intermediate assertion. No separate assert_* acceptance authority exists.

## Hierarchy, collect-all and bounded diagnostics

`certify(name:Into<String>) -> Certification`. `check(name,CheckResult)` / `check_fact_expect(name,value,expected)` consume a result and return &mut Self. `section(name,FnOnce(&mut Certification))` nests semantic paths, returns &mut Self. Checks execute eagerly; failed checks never short-circuit independent ones. check_fact_expect(name,value,expected) is exactly check(name,fact(name,value).expect(expected)); it has identical typed acceptance and diagnostics. Preparing a dependent source fallibly? Convert Err with `try_fact("source", result)` and compare an expectation; continue unrelated checks rather than `?` out of the entire proof.

`certify() -> CertificationReport` returns a nonpanicking bounded snapshot with public name/checks/failed/failures and `is_pass()`. Both Certification and CertificationReport expose `#[track_caller] assert_certify()`; failure panics with one report, success writes nothing. `CertificationReport` Display is empty on success. CheckFailure has section/name/class/diagnostic. Section paths retain hierarchy (e.g. encoder/prediction/timing/).

`limits(DiagnosticLimits{failures,characters_per_failure})` configures retained/rendered failures (default 64 and 2048 UTF-8 bytes per diagnostic, plus bounded names/paths). Omitted failures remain counted. Formatting uses a bounded writer instead of allocating a whole giant trace then truncating it; formatters that propagate write errors stop traversal at the budget. Successful aggregation retains no fact report or duplicate comparison. Custom Debug implementations must propagate formatting errors and avoid unbounded allocation/work before writing; a writer cannot interrupt a formatter that ignores its errors. `bounded_debug(&value,limit)` is available for custom bounded detail.

Useful failures include root/name/path, HardViolation/StatisticalViolation/EvaluationError, named sources, expected criteria, observed summaries, counts and bounded key/provenance examples. Keep full typed reports separately when investigating, never inject an unbounded JSON trace into a panic.

## Mixed ordinary input + internal Observation + ordinary output

```rust
use sun_certifications::*;
use sun_observations_data_source::{CapturedRun, DecodeError};
use serde::{Serialize, Deserialize};
use sun_observations::Observation;
#[derive(Serialize,Deserialize,Observation)]
#[observation(namespace="encoder::prediction")]
struct Prediction { row:u64, signal:f64 }
struct Request { rows:usize }
struct Output { row:u64, signal:f64 }
fn mixed(request:&Request, run:&CapturedRun<Vec<Output>>) -> Result<(),DecodeError> {
    let predictions=run.typed::<Prediction>()?;
    let outputs=run.result();
    let mut c=run.certify("Harness evaluation");
    let input=c.source("input",request);
    let internal=predictions.source("encoder predictions");
    let result=c.collection("canonical output",outputs);
    let pair=internal.compare_with(&result).on(|v|v.row,|v|v.row);
    c.section("input",|c|{c.check("present",input.select(|v|v.rows).expect(CountRule::NonEmpty));});
    c.section("encoder",|c|{c.section("prediction",|c|{
        c.check("comparison_fact",pair.comparison_fact().expect_valid());
        c.check("value",pair.equal(|v|v.signal,|v|v.signal,5).expect_valid());
    });});
    c.section("result",|c|{c.check("ready rows",result.count().expect(request.rows));});
    c.assert_certify();
    Ok(())
}
```

Use capture_observations and CaptureConfig from the SunObservations skill for the real production call. Start every CapturedRun-backed proof with `run.certify(name)`, not standalone `certify`: the bridge adds an EvaluationError prerequisite if retention evicted evidence, emission failed, or contexts/workers/spans escaped. Healthy capture adds no extra check. A retained subset cannot establish completeness; all non-Pass prerequisites fail the final proof. The TypedView is decoded once by the adapter before repeated checks. Filter proc/scope before decoding if rows repeat across runs. All operations above also work with ordinary source_slice instead; Observation provenance is optional, never a generic semantic dependency.
