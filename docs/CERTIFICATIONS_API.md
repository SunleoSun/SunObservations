# Generic semantic certification

sun-certifications owns generic named sources, preparation, alignment, measurements,
acceptance and hierarchical proofs. It has no dependency on SunObservations. An ordinary
returned value, input/output collection, loaded artifact or independent reference is
first-class evidence. The optional Observation adapter supplies identical PreparedSource
handles with external provenance.

## Semantic proof, not implementation shape

Certify the narrowest meaningful owner: canonical results/effects first, production
Boundary/Decision/Diagnostic evidence when internal semantics would otherwise disappear.
Prove both presence and forbidden absence, negative-path category/effects and causal
producer-consumer correspondence. Use independent oracles and metamorphic controls.
Statistical expectations strengthen exact invariants; declared anomalies fail, not warn.

## Sources and one prepared correspondence

```rust
use sun_certifications::*;
#[derive(Debug)]
struct Row { fold:u8, target:u8, row:u64, signal:f64, strength:f64 }
fn prove(left:&[Row],right:&[Row]) {
    let a=source_slice("producer",left); let b=source_slice("consumer",right);
    let pair=a.compare_with(&b).on(|v|(v.fold,v.target,v.row),|v|(v.fold,v.target,v.row));
    let signal=pair.numeric(|v|v.signal,|v|v.signal);
    let mut proof=certify("semantic transfer");
    proof.section("coverage",|c|{c.check("exact keys",pair.coverage().compare(AllValid));});
    proof.section("meaning",|c|{
        c.check("strength",pair.equal(|v|v.strength,|v|v.strength,5).compare(AllValid));
        c.check("signal relation",signal.relation().compare(RelationExpectation::positive(0.9).unwrap()));
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
coverage borrows the mapping rather than copying it. Pair checks retain bounded examples
and optional adapter provenance. Numeric field pairs cache values and reuse correspondence
for relation, difference, linear fit, Pearson, Spearman and top disagreements.

## Measurement and acceptance

Measurements are neutral. measured(name,value) / measurement(name,Result<T,E>) wrap direct
evidence; compare(expectation) produces CompareResult retaining source, expected condition,
full observed Result, typed violations, class and any expectation-precondition error.
Ordinary Equal/NotEqual need PartialEq; ordered Criterion supports greater/less/inclusive
range. CountRule, NumericRule, AllValid, RelationExpectation, GroupExpectation,
AtMostAnomalies and LagExpectation express standard contracts. criterion(description,class,predicate)
and the Expectation<T> trait extend domain acceptance without a second assertion authority.

Classifications are PASS, HARD, STATISTICAL and ERROR. Undefined metrics violate declared
statistical bounds; malformed inputs/preparation block proof as ERROR. check consumes any
CompareResult; success drops its measured report, failure retains bounded diagnostics.
Independent checks/sections continue; unavailable dependent analysis does not silently pass.
certify() returns a nonpanicking CertificationReport. Only assert_certify() panics, tracks
the invocation and is silent on success. Default diagnostics cap 64 failures × 2048 bytes,
plus bounded names/paths. Omitted failures remain counted; raw source length cannot grow
panic output without bound.

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

The [mixed workflow](../crates/sun-observations-data-source/examples/skill_workflows.rs)
combines ordinary input, internal production evidence and canonical output, with grouping,
lag, independent formula reference, nested sections and negative forbidden-event absence.
[Benchmarks](CERTIFICATION_BENCHMARKS.md) preserve release samples through 1M and compare
reusable vs repeated preparation and ordinary vs Observation-adapted projections.
[Certification matrix](CERTIFICATION.md) maps important invariants to executable evidence.
The [self-contained certification skill](../.agents/skills/certifications/SKILL.md) is the
complete methodology and normal-use API reference, including direct analysis/result inspection.
