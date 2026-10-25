# Framework performance audit

## Evidence and reproduction

The accepted run compares production owners at `b5d9c425f4d75fee15154e6806282ae14e82992c`
with this implementation, using **identical current benchmark fixtures** on both sides.
Baseline production files were restored byte-for-byte to HEAD before building into
a separate target directory; a production-path Git diff was empty. No historical
timing is substituted for this baseline. No dependencies, compiler requirements,
public APIs, persisted formats, budgets or coverage requirements were changed.

[Runner](../scripts/audit_framework_performance.py),
[additional production-path fixtures](../crates/sun-observations/benches/framework_audit.rs),
[archived measurements](../scripts/framework_audit/results.json),
[artifact/runner verification](../scripts/test_audit_framework_performance.py).
The archive contains full raw `metrics.csv` (7,704 rows), `summary.json`
(826 metric/configuration comparisons), all 56 native report locations, and build
manifests. Every timed configuration has two process runs per phase, three samples
per process, scheduled **before/after/after/before**; medians use all six samples.
Allocation configurations have two independent counts per phase, not timing samples.

`repeat_metrics.csv` contains another full 7,704-row ABBA comparison with the same
fixtures, before the final removal of unnecessary candidate-cache allocation for
relation-free solves. It independently confirms the solver, weak cache, dataset
and parallel-lag gains; it is not merged into the accepted medians. Both complete
runs, including noisy and regressing rows, are retained. The JSON archive uses
standard gzip/base64 with original byte lengths and SHA-256 checksums, avoiding
large repetitive CSV/pretty-JSON duplication. To extract it with standard Python:

```python
import base64, gzip, hashlib, json
from pathlib import Path
archive = json.loads(Path("scripts/framework_audit/results.json").read_text())
out = Path("audit-results")
out.mkdir(exist_ok=True)
for name, entry in archive["files"].items():
    data = gzip.decompress(base64.b64decode("".join(entry["gzip_base64"]), validate=True))
    assert len(data) == entry["bytes"]
    assert hashlib.sha256(data).hexdigest() == entry["sha256"]
    (out / name).write_bytes(data)
```

Build or measure the current checkout:

```text
python scripts/audit_framework_performance.py --output C:/Temp/audit-current --target-dir C:/Temp/audit-current-build
python -B -m unittest discover -s scripts -p test_audit_framework_performance.py
```

For a new paired comparison, apply the same benchmark-only changes to the baseline
checkout, build it with `--build-only` into a **different** target directory, and
pass its `executables.json` as `--baseline` to the current runner. `--manifest`
replays already built executables; do not rebuild/overwrite their target directories
between phases. `--suites negative capture lag` selects a focused comparison.
The runner propagates child failures and rejects missing/duplicate samples per
configuration, unequal semantic counters, unequal iteration counts and unequal
before/after sample counts. It does not enforce machine-dependent timing gates.
The retained suite also includes a separately archived 288-row/24-comparison
parallel-threshold follow-up at 32,768 and 65,536 rows; its raw CSV, summary and
matching baseline/current manifests are in the same archive.

Environment: Windows 11 10.0.26200, AMD64 Family 25 Model 33, 32 logical CPUs,
MSVC x86-64, rustc 1.96.0, LLVM 22.1.2, ordinary release profile. No CPU affinity,
isolated host, peak-RSS measurement or CPU-specific build flags. Host activity
occasionally slowed even unchanged analysis operations by 20% or more; an earlier
capture process slowed by 2–3x and was rerun. Small differences are not claimed as
optimizations. Absolute numbers are local observations, not capacity promises.

## Retained changes

All times below are milliseconds unless explicitly marked otherwise.

| Area / bottleneck | Change | Before → after | Speedup | Allocation/memory and tradeoff | Correctness evidence |
| --- | --- | --- | --- | --- | --- |
| Multi-negative DFS, chain/7 constraints/21 attempted pairs | Solve-local source candidates, borrowed operands/comparisons, incremental known assignments, allocation-free activation | 368.212 → 8.909 | 41.33x | 1,039,633 → 71,158 allocations; 220,561,256 → 1,927,664 requested bytes. Cache lifetime is one immutable violation set; no cache for relation-free solves. | Baseline witness/coordinate fingerprints for three seeds, chain/star/dense; exhaustive dense feasibility oracle; existing Domain certifications |
| Relation classification/severity | Stream invalid-distance extrema instead of cloning, deduplicating and sorting the entire pair list | Full positive/single/pair negative report: 388.011 → 16.306 | 23.80x combined with DFS changes | No pair-vector allocation for extrema; original total ordering and distance thresholds preserved. | Reference extrema, duplicate values, signed zero, integer extremes and floating overflow |
| Relation compile/negative targets | Deduplicate operand references before Cartesian invalid pairs, retaining original final sort | 5 constraints: 3.282 → 0.526; 10: 24.806 → 2.716 | 6.24x / 9.13x | Two small reference vectors replace repeated clones and growing pair-vector membership scans. | Exact original-style invalid-pair reference and coordinate fingerprints |
| Alternating Observation types miss four TLS slots and lock/re-resolve plans | Keep four strong slots; bounded 28-entry **weak** overflow in cold resolution | 1M inactive calls/8 types: 76.881 → 31.293; active sinkless: 185.623 → 142.755 | 2.46x / 1.30x | Bounded per-thread metadata; overflow hits still allocate an Rc wrapper. Weak references do not extend detached sink/capture ownership. | 40-type budgets/generation changes, 32-type capture order/completeness, reentrant sink destruction and configuration churn |
| Repeated exact-namespace reads scan unrelated records | Snapshot-owned lazy namespace projection; boxed record membership avoids the previous Arc-slice conversion copy | 1M/32 types query: 21.974 → 0.602; decode: 27.307 → 4.685 | 36.49x / 5.83x | At least 4,096 records, built only after the first namespace scan. Mixed index stores original usize positions; uniform index stores only its namespace. 10K/8 types plus 16 queries: requested bytes 10,055,091 → 10,118,743 (+0.63%). | Selector parity, concurrent initialization, immutable snapshots after drain/retention, Arc identity and strict decode-error coordinates |
| Large independent Pearson lag shifts | Parallelize shifts only above 4M estimated row-shift work and width ≥4; retain scalar reduction order inside each shift | 262,144 rows/65 shifts/8 threads: 359.363 → 65.693 | 5.47x | O(candidate-count) ordered results plus existing projected inputs. Sequential small workloads and Spearman remain unchanged. | Individual-shift oracle, serialized equality over 1/2/4/8 threads and repeated runs, projection counts, earliest numeric-error ordering |

The DFS optimization reduces repeated work per search node, **not** the number of
semantic targets or the search limit. It does not eliminate general exponential
search. `SEARCH_LIMIT=100_000` still fails closed; no eager Cartesian enumeration,
heuristic pruning, shared witness cache or pair reordering was introduced.

Additional multi-negative medians confirm more than one topology:

| Shape | Constraints / attempted pairs / produced witnesses | Before | After |
| --- | --- | ---: | ---: |
| Independent ranges | 4 / 6 / 6 | 0.113 | 0.115 |
| Chain | 5 / 10 / 10 | 22.859 | 0.410 |
| Chain | 7 / 21 / 16 | 368.212 | 8.909 |
| Star | 7 / 21 / 18 | 88.247 | 1.401 |
| Dense stress, range 0..=8 | 10 / 45 / 14 | 53.885 | 5.271 |

Attempted pairs are not feasible witnesses: impossible exact-two-negative pairs
remain absent on both sides. The dense stress uses a deliberately bounded value
domain, not an unreported successful large-range exhaustive solve.

## Scaling and end-to-end controls

At budget 64, four active enum dimensions with potential universes
16 / 256 / 4,096 / 65,536 / 1,048,576 retain lazy behavior. After-change compile
medians are 0.279 / 0.257 / 0.244 / 0.244 / 0.287 ms; pair selection is
0.013 / 0.012 / 0.012 / 0.012 / 0.012 ms. Materialization stays about 0.032 ms.
Solve time is 0.168 / 0.189 / 0.238 / 0.380 / 0.914 ms; it is not constant.
The smallest universe has fewer feasible positive cases; raw counters expose this
rather than pretending every traversal executes 64 distinct positives.

| Real execution control | Before | After | Interpretation |
| --- | ---: | ---: | --- |
| Relation-free four-field compile | 0.06646 | 0.06532 | No small-domain cache-allocation regression |
| Cheap seeded report, 1,024 executed cases | 65.239 | 59.952 | Modest end-to-end improvement; hashing itself unchanged |
| Domain + capture + cheap executor + check + report, 64 cases | 0.357 | 0.330 | Framework overhead exposed |
| Same with moderate deterministic computation | 0.556 | 0.529 | User computation retained |
| Same with 100 observations per successful case | 3.105 | 3.220 | No repeatable win/loss: repeat run 3.180 → 3.037, overlapping sample ranges |
| 1M captured records/32 types + 16 namespace queries | 1,059.871 | 816.065 | 1.30x write/read improvement, not query-only gaming |
| Same, one query | 728.429 | 718.463 | Approximately unchanged; index not built for a single read |
| 1M captured records/one type | 661.503 | 615.330 | Throughput not materially worsened; repeat 634.068 → 658.150 |

Single-type capture at 100 / 10K / 100K records measured
0.043 / 5.018 / 64.878 ms before and 0.041 / 4.823 / 63.515 ms after.
The ordinary inactive hot path stays about 6.6 ns/call and remains lazy; active
sinkless emission stays about 151 ns/call. Payload construction is inside the
existing lazy closure and is never run on rejection.

The mixed namespace index has a real tradeoff: uniform full-history queries do
not benefit. At 1M records a warm uniform scan measured 30.210 → 33.793 ms;
capture plus 16 such reads measured 1,101.155 → 1,161.920 ms (repeat
1,127.570 → 1,129.493). This cost is exposed, not removed from the matrix. The
index is accepted for the much larger mixed-query gain and reproducible 23–42%
write/read improvement, without a material small-capture regression. No per-sink
index or retention synchronization was added. Namespace-subtree and other
selectors retain canonical filtering; no unsupported time-selector index exists.

Pearson 1/2/4/8-thread after medians are 346.958 / 214.555 / 117.354 / 65.693 ms.
The repeat measured 337.113 / 285.959 / 149.109 / 81.028 ms; scaling varies with
host scheduling. The threshold follow-up measured 32,768-row sequential vs
8-thread execution at 45.073 → 7.140 ms and 65,536 rows at 124.344 → 14.404 ms.
This confirms a gain near the 4M-work threshold, not just at the largest stress size.
Small 64/1,024/16,384-row searches stay sequential. Thread-pool creation
is outside timers. Projections execute once in caller order before parallel work;
workers run no user projections/checks/executors. Indexed collection and ordered
aggregation preserve first errors and best-lag ties. Certification execution was
not parallelized: hard-stop budgets cannot permit speculative user effects.

The rest of the matrix covers range/collection composition and budgets, coverage
accounting, classification, 0/1/5/20 checks, bounded failures/reporting, Result vs
CapturedRun, snapshots/decode/analysis/serialization, per-category selectors,
MemorySink append and repeated compaction, direct/captured emission and 1/2/4/8
concurrent producers. MemorySink already uses bounded VecDeque batch eviction;
measured linear storage/eviction did not justify another storage authority or a
lock-free queue. Contention benchmarks retain counts and joined-worker evidence.
Checks remain approximately linear: 256 real inputs with 0/1/5/20 checks take
0.003 / 0.103 / 0.506 / 1.995 ms after. They were not micro-optimized.

## Vectorization and rejected experiments

- `NumericSeries::finite_count` already auto-vectorizes. Emitted x86-64 assembly
  for the benchmark wrapper contains packed loads, masks, comparisons and integer
  accumulation (`movupd`, `andpd`, `cmpneqpd`, `cmpordpd`, `psubq`, `paddq`) plus
  a scalar tail. Production/simple scan medians at 64/4,096/1M values are
  0.0000355/0.000686/0.149227 and 0.000036/0.000686/0.151423 ms. A simpler scan
  gives no meaningful win; explicit unsafe/ISA SIMD was not justified. Source
  remains portable and floating reductions are not reassociated. To inspect a
  fresh build: `cargo rustc --release -p sun-observations --bench framework_audit -- --emit=asm`.
- Coordinate-key hashing remains unchanged: 10K eight-entry seed derivations
  measured 2.080 → 2.109 ms. Plain/seeded reports did not establish a meaningful
  added end-to-end cost warranting a second hashing representation.
- Enlarging the **strong** TLS cache was rejected by
  `retired_sink_can_reenter_context_and_configuration_during_generation_eviction`.
  Keeping only weak overflow preserves the ownership boundary.
- Delegating the namespace iterator fold to its enum variants did not remove
  uniform-scan overhead (1M uniform scan 29.041 → 32.425 ms in an isolated ABBA
  comparison); the experiment was removed rather than retaining unused machinery.

Allocation bytes are cumulative allocator requests, including reallocations,
**not peak live memory or RSS**. No unsafe production code or new parallel/SIMD
abstraction was introduced.

## Validation

- `cargo fmt` check; `cargo check --workspace --all-targets`;
  `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo test --release --workspace`: pass, including 291 framework library tests,
  1M-record evidence tests, downstream compile fixtures, process/worker/lifecycle
  contracts, documentation certifications and doctests.
- Seven Python runner/archive tests: pass; all 826 saved medians are recomputed
  from raw samples, checksums verified, complete repeated matrix verified.
- Python validation: Vulture passes; Semgrep cannot run because this workspace
  has no Semgrep configuration. No unrelated configuration was added to hide that
  environmental limitation.
