# Compilation footprint: 0.2.2 implementation

These controlled pre-release measurements compare the original 0.2.1 owners
with the type-independent implementation retained in 0.2.2. They are not
whole-workspace cold-build predictions or timing gates.

## Implementation

- `CompiledDomain<T>` embeds a non-generic solver core. Schema acquisition,
  input conversion and materialization remain typed; algorithms and traversal
  state stay under the same semantic owner.
- Panic-hook installation/restoration and payload formatting are non-generic;
  the operation and unwind boundary remain typed.
- Runtime metadata resolution is shared, while cache identity remains
  `(engine_id, TypeId)`, with unchanged policies, routing and invalidation.

No new boxing, dynamic dispatch, per-case allocations or runtime caches were
introduced. Public APIs, cases and budgets were not reduced.

## Compilation and artifacts

Windows 11 build 26200, Ryzen 9 5950X, 32 logical CPUs, rustc 1.96.0 MSVC,
LLVM 22.1.2. Experimental targets used D: NVMe; no native-CPU override.
Dependencies were warm, `CARGO_INCREMENTAL=0`; both profiles retained assertions
and overflow checks. The default profile used full debug information; the
optimized profile used opt-level 2/debug 0.

| Lib-test profile | Baseline unit | Final unit | Speedup | Executable before/after |
| --- | ---: | ---: | ---: | ---: |
| Default | 46.45 s | 23.10 s | 2.01x | 56.327 / 25.604 MiB |
| Optimized | 210.68 s | 60.20 s | 3.50x | 24.252 / 12.354 MiB |

Whole no-run commands were 46.648/24.047 s and 210.836/62.097 s respectively.
Final commands also rebuilt the ordinary library. The baseline contained 350
tests and final 352: two additional regressions were added, none removed.
Fresh default PDB file lengths were 463.527/222.387 MiB. File lengths are not
allocated disk blocks, working memory or accumulated cache sizes.

Lightweight compiler profiling on an identical 350-test source attributed
further improvements after the Domain split:

| State | Instrumented rustc | Instantiations | Estimated MIR |
| --- | ---: | ---: | ---: |
| Domain core | 72.292 s | 61,694 | 2,826,678 |
| Shared panic boundary | 64.445 s | 54,617 | 2,603,534 |
| Shared metadata resolver | 59.636 s | 52,900 | 2,462,933 |

The headline comparison was uninstrumented. MIR estimates are not time
percentages. LLVM work dominated; no LTO/profile changes were retained.

## Execution tradeoff

Five alternating process pairs ran the same 84 Domain tests. Optimized suite
medians were 0.29/0.28 s; default 2.12/2.16 s, with overlapping ranges.

Separate unmodified public benchmarks compared published 0.2.1 with the final
local library using identical non-framework dependency versions/checksums,
opt-level 2/debug 0 and nine samples per workload.

| Workload | Baseline | Final | Change |
| --- | ---: | ---: | ---: |
| Cheap 64-budget campaign | 0.402214 ms | 0.427250 ms | +6.22% |
| Moderate campaign | 0.594998 ms | 0.619634 ms | +4.14% |
| Capture-heavy campaign | 3.123340 ms | 3.107164 ms | -0.52% |
| Admitted sinkless emission | 195.595 ns/call | 195.686 ns/call | +0.05% |
| Inactive emission | 8.751 ns/call | 8.987 ns/call | +2.70% |

Cheap campaigns incur about 25 microseconds of repeatable overhead. This is
not a runtime-free optimization; it is retained for the much larger build-time
and code-size improvement. Most emission/selector sample ranges overlapped.
The [release chart refresh](BENCHMARKS_0.2.2.md) is a separate protocol, not
additional paired regression evidence.

## Behavior and reproduction

The independent Domain transcript matched all 192 configurations exactly:
445,250 bytes, SHA-256
`b6e5be6f30f593d6062b28ebf0bd9d41b31ed1758c69b5c6a86f88c4def515a0`.
It includes seeds/budgets, pairwise coverage, real-input prefill, conditional
sources, enum payloads, collections, extreme integers and error controls.
Materialization errors still prevent executor calls.

Owner regressions cover materialization failure/cursor progress, nested and
thread-isolated panic handling, and distinct Observation types sharing a
namespace across reconfiguration. Pre-release workspace validation passed
352 library tests, 79 doctests and integration/downstream/skill controls.

~~~sh
cargo test --offline --locked -p sun-observations --lib --no-run --timings --target-dir <profile-target>
~~~

Set `CARGO_INCREMENTAL=0`. For optimized mode additionally set
`CARGO_PROFILE_TEST_OPT_LEVEL=2`, `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true` and
`CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true`. Compare rebuilt owners with warm
dependencies, not a cache hit against a rebuild. Cargo profiles were not changed.

Raw measurements, timing HTML, archived binaries, runtime CSVs and the parity
transcript were retained under
`D:/Temp/sun-observations-compile-knb7ua/evidence/`.
