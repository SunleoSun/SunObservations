# Generic certification API and semantics reference

Technical reference for `sun_observations::certifications`. For a first proof and progressive
examples, start with the [human guide](../README.md#certifications).
This page describes preparation, acceptance and analysis semantics, not the
SunObservations project release gate.

The certification layer owns generic named sources, preparation, comparison, facts,
acceptance and hierarchical proofs. It remains independent of runtime concepts and state. An ordinary
returned value, input/output collection, loaded artifact or independent reference is
first-class evidence. The Observation evidence adapter supplies identical PreparedSource
handles with external provenance.

## Semantic proof, not implementation shape

Certify the narrowest meaningful owner: canonical results/effects first, production
Boundary/Decision/Diagnostic evidence when internal semantics would otherwise disappear.
Prove both presence and forbidden absence, negative-path category/effects and causal
producer-consumer correspondence. Use independent oracles and metamorphic controls.
Statistical expectations strengthen exact invariants; declared anomalies fail, not warn.

## Sources and one prepared correspondence

```rust
use sun_observations::certifications::*;
#[derive(Debug)]
struct Row { fold:u8, target:u8, row:u64, signal:f64, strength:f64 }
fn prove(left:&[Row],right:&[Row]) {
    let mut proof=certify("semantic transfer");
    let a=proof.collection("producer",left); let b=proof.collection("consumer",right);
    let pair=a.compare_with(&b).on(|v|(v.fold,v.target,v.row),|v|(v.fold,v.target,v.row));
    let signal=pair.numeric(|v|v.signal,|v|v.signal);
    proof.section("correspondence",|c|{c.check("exact keys",pair.comparison_fact().expect_valid());});
    proof.section("meaning",|c|{
        c.check("strength",pair.equal(|v|v.strength,|v|v.strength,5).expect_valid());
        c.check("signal correlation",signal.correlation().expect(CorrelationExpectation::positive(0.9).unwrap()));
        c.check("slope",signal.linear_fit().slope().expect(close_to(1.)));
    });
    proof.assert_certify();
}
```

source(name,&T) is singular, source_slice(name,&[T]) is a collection. Certification::source
and collection return typed handles, not a runtime Any registry. select extracts one
scalar/field (requires one value); project caches arbitrary projections; numeric caches
f64 projections; keys caches sorted key/index vectors for correspondence with multiple
counterparts. group_by retains deterministic key/member identity.

PreparedComparison owns one mapping, independent of chosen fields. on supports Ord+Clone
tuples/dedicated key structs; on_sorted validates monotonicity; on_u64 offers adaptive
integer specialization. Missing/duplicate keys are explicit and duplicate keys never pair.
comparison_fact borrows the mapping rather than copying it. Pair checks retain bounded examples
and optional adapter provenance. Numeric field pairs cache values and reuse correspondence
for correlation, linear_fit, difference and top disagreements. Linear moments, ranks,
difference and residual results are cached, so requesting clearer semantic views does not
repeat projections, key sorting or correspondence. Standalone source/source_slice and
Certification::source/collection construct identical PreparedSource handles; no heterogeneous registry.

## Fact and acceptance

Facts are neutral. fact(name,value) / try_fact(name,Result<T,E>) wrap direct
evidence; expect(expectation) produces CheckResult retaining source, expected condition,
full observed Result, typed violations, class and any expectation-precondition error.
Plain expected T:Debug+PartialEq means equality (including enums, booleans, strings and Options);
Equal<T> remains an advanced explicit building block. NotEqual<T> means inequality; ordered Criterion supports greater/less/inclusive
range. CountRule, NumericRule, AllValid, CorrelationExpectation, LinearFitExpectation, DifferenceExpectation, GroupExpectation,
AtMostAnomalies and LagExpectation express standard contracts. criterion(description,class,predicate)
and the Expectation<T> trait extend domain acceptance without a second assertion authority.

CheckClass variants are Pass (proven), HardViolation (evaluated deterministic mismatch),
StatisticalViolation (evaluated scenario statistical/anomaly mismatch) and EvaluationError
(invalid/unavailable/incomplete evidence or evaluation). Required undefined metrics and
invalid bounds fail closed as EvaluationError. check consumes CheckResult; success drops
its fact report, failure retains bounded diagnostics.
check_fact_expect(name,value,expected) exactly delegates to check(name,fact(name,value).expect(expected)).
comparison_fact().expect_valid() checks exact unique correspondence; direct comparison() remains typed.
close_to(expected) uses absolute=relative=1e-9 and accepts .absolute(t)/.relative(t) overrides:
abs(actual-expected) <= absolute + relative*abs(expected). Invalid/nonfinite inputs are EvaluationError.
Independent checks/sections continue; unavailable dependent analysis does not silently pass.
certify() returns a nonpanicking CertificationReport. Only assert_certify() panics, tracks
the invocation and is silent on success. Default diagnostics cap 64 failures × 2048 bytes,
plus bounded names/paths. Omitted failures remain counted; raw source length cannot grow
panic output without bound.

## Statistical semantic views

correlation() returns Fact<CorrelationMetrics> with pearson/spearman metrics; CorrelationExpectation::positive/negative
constrain rank association, or default().pearson(min,max)?.spearman(min,max)? sets optional bounds.
linear_fit() returns Fact<LinearFitMetrics> with linear/residual metrics; the fact exposes slope()/intercept()/
r_squared() as fallible f64 facts. LinearFitExpectation::default().slope(min,max)?.intercept(min,max)?
.r_squared(min)?.residual_mean_absolute(max)?.residual_max_absolute(max)? constrains regression.
difference() returns Fact<DifferenceSummary> (right minus left); DifferenceExpectation::default()
.mean_absolute(max)?.p95_absolute(max)?.max_absolute(max)? constrains direct errors. All builders
return Result and all bounds are inclusive. The advanced relation()/RelationExpectation composite
remains for deliberate multi-class analysis and delegates to the same mathematical owners.

When internal production evidence is required, use the [SunObservations skill](../crates/sun-observations/agent-skills/sun-observations/SKILL.md):
capture_observations wraps the real global observe! path, and run.certify automatically
incorporates capture completeness before mixing ordinary inputs/outputs and typed evidence.

## Analysis semantics

- Summary: compensated sums/shifted moments; population and sample variance (sample absent
  at n<2); type-7 quantiles, median/IQR/unscaled MAD. Invalid/nonfinite/empty/overflow/underflow
  are explicit. NumericValue preserves nonfinite identity in serializable errors.
- Relations: Pearson and covariance/OLS are linear with constant numeric scratch; Spearman
  averages tied ranks including signed-zero ties. Constant/insufficient axes yield Metric::Undefined.
  Difference is right−left; residual is observed right−fitted right. Full relations require
  nonempty valid complete evidence; correlation alone does not certify scale or causality.
- Groups: ascending typed keys, original source member order; cached grouped projection;
  all groups retain Result summaries, so healthy evidence survives another group's error.
- MAD outliers: strict modified-z threshold, explicit zero-MAD ExactMedian or original-unit
  AbsoluteTolerance. Jumps: current source order, strict optional absolute/relative thresholds,
  relative denominator max(abs(previous),positive floor), earliest max on ties.
- Lag: positive d pairs left k with right k+d; bounded inclusive range ≤4097 candidates,
  one temporal key/value projection and sort per search, no per-candidate full trace.
  Absolute relation strength wins; exact ties prefer smallest |d| then signed d. Boundary
  truncation differs from interior missing keys; expected best lag rejects undefined/interior
  incomplete candidates. Use one unique temporal domain, or search separately per group.

## Examples, performance and complete operational reference

The [mixed workflow](../crates/sun-observations/examples/skill_workflows.rs)
combines ordinary input, internal production evidence and canonical output, with grouping,
lag, independent formula reference, nested sections and negative forbidden-event absence.
[Current capture/cache benchmarks](CAPTURE_BENCHMARKS.md) measure reusable semantic views.
[Earlier certification samples](history/CERTIFICATION_BENCHMARKS.md) preserve the
pre-cache dataset, including ordinary vs Observation-adapted projections.
[Framework release validation](RELEASE_VALIDATION.md) maps the project's own invariants to executable evidence.
The [self-contained certification skill](../crates/sun-observations/agent-skills/certifications/SKILL.md) is the
complete methodology and normal-use API reference, including direct analysis/result inspection.
