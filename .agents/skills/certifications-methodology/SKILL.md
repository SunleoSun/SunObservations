---
name: certifications-methodology
description: Design, review, or migrate semantic certifications. Use when deciding proof scope, evidence, Domain coverage, Observations, negative paths, or how existing tests should change.
---

# Semantic certification methodology

Use this skill whenever creating, changing, reviewing, or migrating certifications. Also read certifications-api when writing proof code, domain-api when a Domain exists or should be considered, and observations-api when proof needs production Observations or capture.

## Core model

**Certify semantic behavior, not implementation shape.**

A certification is executable proof of one semantic contract on the real production path. Start from the **semantic owner**: the component that owns the domain decision, invariant, transformation, validation rule, side effect, or producer-consumer boundary.

Unit, integration, and end-to-end are scales, not different philosophies. A small function deserves its own certification only when it owns independent semantics. A large private helper may need none when its behavior is fully owned elsewhere.

Prefer one authoritative proof over several tests that restate the same contract. Do not certify temporary containers, helper call counts, loop shape, cache representation, or other implementation details unless the detail is itself promised behavior such as exactly-once persistence or bounded work.

## Evidence priority

Use the most authoritative evidence that can prove the contract:

1. canonical returned result or persisted artifact;
2. promised external effect/state;
3. typed production Observation;
4. independent reference/oracle;
5. statistical/anomaly evidence.

Do not add an Observation that merely repeats an already canonical return value, artifact, or external state.

### Certification only vs Observations

Use **certification only** when input, canonical result, artifact, or promised state exposes enough evidence.

Use **certification + Observations** when a proof-critical semantic fact disappears inside the production pipeline: actual rows transferred between owners, a meaningful selected/rejected branch, causal identity, checkpoint choice, provenance, or another internal boundary fact. Emit the minimum typed production evidence and certify it together with the canonical outcome.

Use **Observations without certification** for runtime diagnostics or telemetry when no acceptance proof is being made. Observations record facts; certification owns expectations and pass/fail policy.

Before adding an Observation, answer:
- Which certification or diagnostic consumer needs it?
- Which semantic contract does it expose?
- Why can a more canonical result, artifact, or effect not prove that contract?

### Observation roles

When internal evidence is necessary, instrument semantic facts rather than implementation state:
- **Boundary Observation** records a meaningful transfer between owners or stages and is the default evidence for cross-owner correspondence.
- **Decision Observation** records a domain-significant accept/reject/select/suppress branch that changes downstream behavior.
- **Diagnostic Observation** records metrics or state needed by a specific certification or diagnostic workflow.

Do not observe temporary containers, loop iterations, helper entry, or other incidental state unless that state is itself a promised contract.
## Domain as input-proof strategy

Use a Domain when the owner promises behavior over a **family of inputs** that can be faithfully described by the Domain DSL. Domain describes where the contract must hold; checks describe what must hold after execution.

Keep concrete data_source cases when the specific case itself carries regression, boundary, fixture, external-data, or diagnostic meaning. A strong owner certification often combines meaningful concrete cases with Domain exploration.

Generate invalid states only against the semantic owner that owns validation or rejection of those states. A downstream owner receiving an already validated or typed input should be certified over its reachable valid domain, not forced to handle impossible raw states.

Do not use Domain merely because generation is available. Exact fixtures remain preferable when the contract is inherently example-specific, expensive, externally defined, or cannot be faithfully expressed by the DSL.

## Migrating existing tests and certifications

For every existing single-case certification, decide whether the contract is **example-specific** or **family-wide**.

- **Keep the concrete case** when it is a historical regression, exact fixture, meaningful boundary exemplar, external scenario, or otherwise has independent value.
- **Concrete case + Domain** when that meaningful case should remain but the same contract must hold over a broader input family.
- **Domain only** when the old input was arbitrary and carries no independent meaning.
- **Remove the certification** only when it has no independent semantic contract or another certification already proves the same contract with no loss of diagnostic value.

When a family-wide contract is still represented by one arbitrary case, Domain must be considered. Negative generation belongs at the validation or rejection owner; downstream validated owners use only reachable valid inputs.

Do not rewrite a good semantic certification solely to use newer DSL. Preserve named regressions whose concrete reproduction remains useful even when Domain could generate a similar case.

## Owner and composition proof

An **owner certification** proves semantics owned by one semantic owner. It may combine deterministic examples, Domain-generated cases, regression cases, oracle checks, statistical checks, and metamorphic checks; proof technique does not create a new owner.

A **composition certification** proves a semantic contract across multiple owners or an explicit producer-consumer boundary. It should prove only the cross-owner promise instead of re-testing all local owner details.

Repository-specific placement and filenames for these two levels belong to the repository AGENTS.md, not this portable skill.

## Strong proof design

For each meaningful contract, choose only evidence categories that materially strengthen it:

- canonical outcome;
- must happen;
- must never happen;
- producer to consumer correspondence;
- typed domain identity;
- causal or temporal alignment;
- failure or rejection semantics;
- grouped evidence;
- statistical strengthening;
- independent oracle;
- metamorphic or control scenario.

### Must happen and must never happen

Failure-path proof is stronger than checking only Result::is_err. When semantically promised, prove the rejection owner or category and absence of forbidden downstream events, artifacts, state changes, duplicate actions, or other side effects.

### Identity and causality

Join evidence by explicit domain identity, never incidental record order, Vec index, sink order, or formatted field names unless that ordering is itself contractual.

Value agreement does not prove causality. Certify row, timestamp, horizon, and decision-entry offsets explicitly when known. Use lag analysis when diagnosing an unknown stale, off-by-one, or future dependency.

### Hard, statistical, and evaluation failure

A **hard invariant** is logically incompatible with a correct system when violated.

A **statistical expectation** is scenario-owned acceptance policy such as correlation, slope, grouped deviation, outlier count, or lag. It is not a warning and does not belong in production Observation metadata.

An **evaluation error** means the proof could not be validly evaluated: incomplete capture, decode failure, undefined required metric, invalid bounds, incomplete correspondence, or invalid configuration. Proof fails closed.

### Grouping, statistics, oracle, metamorphic evidence

Global aggregates can hide broken folds, targets, directions, episodes, or runs. Check completeness of natural semantic groups before per-group expectations.

After exact invariants, add statistical evidence only when it can reveal a realistic defect the exact checks miss.

For optimized or mathematically complex behavior, prefer a simple independent oracle that uses a meaningfully different path. Do not compute expected values with the same production helper.

When exact expected output is awkward, use metamorphic relations: known scaling, sign reversal, partition invariance, shuffled-association collapse, or another transformation with a known semantic consequence.

Strengthen statistical evidence with a null/control scenario when it distinguishes the intended relationship from an accidentally high metric.

Certification names should state the semantic promise, not the implementation mechanism. For large composition proofs, group checks by semantic pipeline stages so failures point to the responsible owner or boundary without re-testing owner-local details. The SECTION node in certifications-api supplies nested semantic paths in both eager and planned proofs; it does not change which cases satisfy the contract.

Performance belongs in certification only when it is promised semantics such as bounded work, exactly-once external calls, or bounded queues. Keep incidental wall-clock thresholds in benchmarks.

When production code changes, preserve expectations that encode independent intent. Do not make a broken certification green by deriving expected values from the implementation under test or by rewriting the contract to match the defect.
## Execution and diagnostics

Run the real production path. Prefer deterministic fixtures; when production randomness matters, use deterministic seeds or invariants independent of the sample.

Collect independent checks so one execution yields a useful bounded semantic report instead of stopping at the first unrelated failure. Dependent analysis that cannot be evaluated should become an evaluation error; unrelated checks should still run.

Instrumentation must observe production semantics, never become a second behavior owner or a test-only execution path.

## Completion standard

A certification is complete when its smallest sufficient evidence set proves the intended semantic contract, realistic negative or failure behavior is covered at the owning boundary, and the proof remains valid across implementation refactors that preserve semantics.