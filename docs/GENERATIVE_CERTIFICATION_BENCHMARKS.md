# Generative certification verification and benchmarks

This is a separate planned/generative dataset, not a relabeling of the
[historical eager certification measurements](history/CERTIFICATION_BENCHMARKS.md).
The public API and mathematical algorithms are unchanged.

## Reproduce

From the workspace root:

```text
cargo test --release -p sun-observations --bench generative_certifications -- --measure
python scripts/measure_generative_certifications.py
python scripts/measure_generative_certifications.py --summary
```

The first command is the actual release measurement command. The Python runner
executes that command, isolates SUN_CERT_SEED=184729361,
SUN_CERT_CASES_BUDGET=64 and SUN_CERT_REPORT=0, propagates child failure, parses
all CSV rows, requires every operation family and all three samples, and verifies
execution/hard-stop counters. It has no timing thresholds. Without --measure,
the Rust benchmark main returns without building fixtures or executing cases.

[Harness](../crates/sun-observations/benches/generative_certifications.rs),
[validated run transcript](../scripts/generative_certification_benchmark_run.txt),
[selected sample-0 CSV](../scripts/generative_certification_metrics.csv).
The retained CSV is an explicitly selected subset of a successful 999-row run
(333 configurations, 22 operation families), **not all samples or medians**.
The runner without --summary emits every raw row. CSV_BEGIN/CSV_END separate
machine-readable rows from runner environment/build diagnostics.

Recorded environment: Windows 11 10.0.26200, AMD64 Family 25 Model 33 Stepping 0
(AuthenticAMD), 32 logical CPUs; rustc 1.96.0 (ac68faa20 2026-05-25), Python
3.13.15, default optimized release profile. Checkout
852c909dffd753be99f7b8ec7930e0378daf36c7 plus this working-tree implementation.
Build output: C:/Temp/sun-observations-build/release. No affinity, isolated host,
custom allocator, RSS profiling, or CPU-specific performance gates. Unrelated
host activity can materially affect individual rows.

## Timed boundaries

Each configuration has one untimed warmup and three samples. Each sample is the
arithmetic mean of five separately timed operations, in milliseconds. Setup runs
before every timer; inspection, CSV printing, and destruction of returned outputs
run afterward. Intermediate allocations/destruction inside an operation remain
included. Inputs, Domain specifications, fixture rows and prepared targets are
constructed outside timing unless their construction is the named operation.
No compile/planner measurement first solves or materializes the complete universe.

| Operation | Included in timing |
| --- | --- |
| domain_compile | Consume a prebuilt Domain specification; canonical compile, validation/preflight, lazy coverage catalog construction. Specification cloning is outside. |
| planner_pair_target_selection | Select at most budget indexed pair targets from the compiled catalog. No solving, input materialization, executor, or checks. |
| planner_higher_order_target_selection | Select at most budget higher-order enum targets. No materialization/execution. |
| target_solve | Solve a bounded preselected target prefix. Target selection is outside. |
| input_materialize | Build DomainValue and typed input from already solved bounded targets. Solving/selection are outside. |
| planner_next_materialize | Repeated canonical positive next calls: target selection, solving, materialization, classification and occupied-coordinate deduplication. No executor/check/report work. |
| planned_report | report() of an already configured and compiled runner: scheduling, generated cases, executor, coverage/failure aggregation. Compile and check registration are outside. |
| range_target_selection | Post-anchor seeded free-target selection; no input materialization or executor. |
| range_explore | Positive next traversal including anchors and subsequent seeded strata; budgets 2/8/64 distinguish the phases. |
| collection_target_solve | Solve a bounded mixed/spread target prefix, including infeasible attempts. executed counts feasible solutions, budget counts attempted targets. |
| collection_materialize | Mixed/spread DomainValue and typed collection construction from bounded feasible solved targets; uniform_typed_value measures only the typed conversion of a prebuilt uniform DomainValue. |
| collection_explore | Positive next traversal across uniform anchors, pairwise mixed and spread compositions, including solving, materialization and deduplication. |
| negative_exploration | Precompiled runner report with positive-only, single-negative or single+pair-negative scheduling; independent fixture validator rejects invalid requests. |
| classification | Classify prebuilt real inputs or one prebuilt collection, including input-to-DomainValue projection. No executor. |
| data_source_prefill | Report over n preloaded inputs: classification, coordinate prefill/deduplication, cheap execution, coverage and one generated slot. Compile/loading are outside. |
| planned_check_evaluation | Precompiled generated scalar report with 0/1/5/20 deferred checks, including scheduling and cheap execution. |
| checks_real_cases | Same check counts over 256 preloaded inputs, without Domain/generation/classification; isolates check evaluation with the real-case runner baseline. |
| full_planned | Runner construction, Domain compile, planning, near-no-op executor and report; Domain specification cloning is outside. |
| failure_hard_stop | Precompiled report with failure budgets 1/5 or healthy execution; diagnostic generation and representative selection are included. |
| planned_plain_result | Linked production planned report over preloaded real inputs returning Result. No Domain/generation. |
| planned_captured_run | Same real-input count returning CapturedRun<Result>, one deterministic typed Observation per case; includes capture, emission/serialization, finalization and completeness enforcement. |
| capture_only | One complete capture/emission/finalization without a planned runner. Decode/count/completeness inspection is outside. |

The benchmark source-links the canonical certification module so existing stage
methods can be crate-visible without adding any public benchmark API. Source-linked
measurements include existing cfg(test) solver-counter bookkeeping; they are not
an alternate algorithm. Capture/plain integration uses the linked production
package and its canonical capture adapter. These boundaries should not be summed
or subtracted as if allocations, optimization and cache state were identical.

## Fixture and scaling contract

- Enum products use eight typed fields; 2/4/6/8 active dimensions and 2/4/8/16/32
  choices. Inactive fields select one choice. Positive potential = cardinality to
  the active dimension count; excluded choices are real negatives for the fixture
  validator, not failures of a mathematical algorithm. Budgets are 8/64/256.
- Scalars have the benchmark request contract 0..=9 or 0..=1,000,000,000,000.
  Small ranges expose exhaustion; large ranges expose post-anchor stratification.
  The large single-scalar catalog currently executes 208 total cases (202 positive,
  six negative) even with a larger budget. Eight 32-choice enums supply sufficient
  frontier for full execution at budgets 8/64/256/1024.
- Four numeric fields in 0..=1000 plus 0/1/3/6 ordered relations give 4/5/7/10
  constraints. Compile measures all four shapes. Negative execution measures
  0/1/3 relations; the six-relation transitive/redundant shape is compile-only:
  exact negative combinations can exceed the bounded solver search. This benchmark
  does not change planner semantics or disguise solver errors as successful runs.
- Collections vary fixed length 1/8/64/256 and element cardinality 2/8/32.
  Length-one mixed/spread targets are infeasible: solver rows expose this and no
  materialization row is fabricated. Mixed materialization uses at most 64 targets;
  spread uses the canonical three-representative composition. A separate nested
  enum collection includes active payload fields and lengths 1..=32.
- Real inputs are 10/100/10,000 scalar requests. Checks are 0/1/5/20 over identical
  256-case fixtures. Failure-stop input has four enums and one bounded scalar,
  providing distinct semantic failure representatives. Capture uses 8/64/256
  real cases and one small i64 payload per case.
- CSV constraints counts source and relation rules; negative_pairs is the potential
  number of distinct negative-constraint pairs, not a promise they are all feasible.
  Zero-valued irrelevant dimensions are not measured. Collection potential
  saturates at u128::MAX; that sentinel is not an exact cardinality.
- executed means completed stage items for stage rows and actual executor calls
  for report rows. Pure selection/solving/materialization may revisit equivalent
  typed inputs; planner/report rows deduplicate by canonical coordinates.

## Representative recorded rows

Selected sample 0, milliseconds per operation, fixed budget 64 and four active
enum dimensions:

| Positive potential | Compile | Pair target selection | Solve | Materialize | Positive next traversal | Cases traversed |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 0.298400 | 0.013020 | 0.410460 | 0.032700 | 1.055700 | 16 |
| 256 | 0.246940 | 0.011920 | 0.411960 | 0.033140 | 1.512200 | 64 |
| 4,096 | 0.245180 | 0.011580 | 0.442920 | 0.033300 | 0.957700 | 64 |
| 65,536 | 0.261800 | 0.011600 | 0.587700 | 0.033800 | 1.283860 | 64 |
| 1,048,576 | 0.306120 | 0.011620 | 1.110780 | 0.033200 | 1.828300 | 64 |

For potential 16, stage prefixes have 62 target items but traversal exhausts at 16
unique inputs; do not compare these as identical case throughput. For the other
rows all stage prefixes and traversal have 64 items. Growing potential from 256 to
1,048,576 does **not** cause eager Cartesian planning: target selection stays near
0.012 ms, materialization near 0.033 ms, compile near 0.25–0.31 ms and traversal
near 0.96–1.83 ms. Solver cost still grows with choice count; this is evidence of
frontier-bounded work for these fixtures, not constant-time solving for arbitrary
constraints or a machine-independent asymptotic proof.

Other selected results:

- Full enum execution, budgets 8/64/256/1024: 1.133220 / 4.233320 / 17.639980 /
  63.058960 ms; executed counts exactly match those budgets.
- Hard stop: failure budget 1 executes 1 case, budget 5 executes 129, healthy run
  executes 4096 (0.060480 / 4.454440 / 190.645880 ms). The budget counts distinct
  failure representatives, not raw failed executions; waiting for new sectors is
  expected and explains 129 rather than 5.
- 256 real cases with 0/1/5/20 checks: 0.003460 / 0.104160 / 0.522260 / 2.021480 ms.
- 256 real cases: plain 0.004060 ms, captured 0.293700 ms. These measure the complete
  capture path, not planner overhead. Tiny rows approach timer resolution.
- Negative generation is constraint-sensitive: the 7-constraint/21-potential-pair
  shape costs 427.854240 ms at budget 256, versus 3.684580 ms for four independent
  range constraints. Lazy target storage does not make difficult constraints cheap.

## Semantic verification surface

Owner-adjacent generative certifications supplement, rather than delete, existing
handcrafted boundary/regression tests and independent seed-loop controls:

| Owner | Added contracts |
| --- | --- |
| stats_generative_certification | Sample size/order/shape/offset/scale/quantile interactions; exact integer-lattice moments, sorted quantiles/MAD, permutation/translation/positive-scale/reflection laws; naive-sum negative control. |
| relation_generative_certification | Association/ties/order/offset/scale/count; independent centered moments/average ranks/differences, known affine law, direct-to-prepared parity, undefined expectations and metamorphic controls. |
| lag_generative_certification | Measure/shift/sign/state/gaps/order/magnitude; exhaustive per-candidate matching, boundary/interior evidence, independent correlation/ranks, best strength/unique winner and exact tie ordering. A separate 128-case-budget proof visits all 72 shape/gap/order triples. |
| join_generative_certification | Overlap/cardinality/duplicate-side/order; independent map reference, presorted equivalence and exact decreasing side/index. |
| integrity_generative_certification | Empty/singleton/holes/duplicates/unexpected/complete at zero/ordinary/u64::MAX; exact indices/ranges/key evidence and same-key join parity. Numeric nonfinite/sign/zero/inclusive-boundary payloads. |
| grouping_certification | Group counts/sizes/order/value classes; source-order membership, independent per-group moments/median/MAD, robust opt-in, identity/error isolation and cached projection counts. |
| source_generative_certification | Generated prepared-to-owner uniqueness/contiguity/numeric/relation parity, source names, source-local indices and transferred provenance. |
| anomaly_generative_certification | Absolute/relative/both thresholds, strict boundaries, floor regimes, sign, repeated maxima, empty/stable/nonfinite traces against an adjacent-pair reference; outlier affine-index invariance. |
| planned_controls + planned_process_certification | Zero failure budget, duplicate Domain, bounded UTF-8 diagnostics, later-check continuation, invalid envs and effective local overrides. Subprocess-isolated environment/hook mutation; concurrent executor/check panics versus unrelated hooks during and after active evaluation. |
| evidence/planned_capture_certification | Real and generated multitype captures with distinct execution IDs, counts and signed payloads, including empty execution; exact case-local dataset/result parity. Existing incomplete-capture rejection priority remains. |

Every mathematical/structural scenario certification explicitly seeds planning
and asserts actual enum/pairwise coverage, not only pass status. Domains describe
semantic scenario classes; they do not impose artificial numeric validity ranges
on the production algorithms. Existing overflow, underflow, nonfinite, duplicate,
sorted-input, extreme-key and capture-completeness regressions remain intact.
