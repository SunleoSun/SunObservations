# Production release certification

The suite is executable, not a collection of placeholders. Owner-adjacent modules are
compiled by the runtime lib test harness; the workspace package
sun-observations-certifications supplies public cross-module, downstream and OS-process
checks. Existing regression tests remain part of the release gate, especially the allocation,
context lifecycle, retention hysteresis and compile-diagnostic checks listed below.

For application usage and operational boundaries, see the [production user guide](README.md).

## Required release commands

Run from the workspace root:

~~~sh
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-features --all-targets
cargo test --workspace
cargo test --workspace --all-features
cargo test -p sun-observations --all-features --lib certification
cargo test -p sun-observations-certifications
cargo test -p sun-observations-certifications --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
~~~

Inspect discovery with:

~~~sh
cargo test -p sun-observations --all-features --lib -- --list
cargo test -p sun-observations-certifications --all-features -- --list
~~~

The default-feature package does not build the Rayon test target. The all-features gate
is mandatory for releases including Rayon. The temporary downstream crate is compiled and
run offline with a genuinely renamed sdk dependency; it needs the already-fetched workspace
dependencies and a working Cargo/toolchain. It is not an ignored/manual example.

## Contract matrix

Paths below are under crates/sun-observations/src unless prefixed certifications/.
Each row is enforced by the full release commands, not solely the name-filtered owner subset.

| Production contract | Executable evidence |
| --- | --- |
| Explicit semantic identity, exact/descendant/segment boundary, invalid route atomicity | namespace_certification_test.rs; routing_test.rs |
| Derive INFO/WARN/ERROR, generic Serializable, invalid/missing/duplicate/unknown metadata | observation_test.rs + fixtures; certifications/tests/derive_downstream.rs (temporary renamed dependency, compile-pass/run and 10 compile-fail cases) |
| Lazy union activation, global/local exclusion veto, WARN/ERROR selection, Never | engine_certification_test.rs; engine_test.rs; plan_test.rs |
| Sinkless callbacks, serialization error before dispatch, checked sink failures, proc/clocks, retired engine identities | engine_certification_test.rs; engine_test.rs |
| Shared parent/child plan identities, warm zero-allocation rejections, no payload evaluation | plan_test.rs (allocation-counting allocator and identity assertions) |
| Four-entry TLS eviction, distinct types/engines/nested scopes, reentrant destruction without TLS/config borrows | plan_certification_test.rs; context_test.rs |
| Generation invalidation for selection/routes/attachment/detachment with counter continuity, already-warm workers | plan_test.rs; certifications/tests/configuration_churn.rs (8 workers, 7 synchronized phases) |
| Scope nesting, out-of-order drop, captured lifetime, explicit empty context, no override leakage | context_test.rs; scope_test.rs; scope_certification_test.rs |
| Exact concurrent Always/Never/FirstN/EveryN/MaxPerScope budgets, saturation, rejected vs failed admission counting | scope_certification_test.rs; engine_certification_test.rs; scope_test.rs |
| Simultaneous parents, transitive public OS-thread propagation, plain-thread isolation, unwind and parent restoration | context/thread_certification_test.rs; certifications/thread_propagation_certification_test.rs |
| Construction-bound independent Rayon pools, nested work, strict shared budgets, empty pool, worker-local scope, caller restoration | context/rayon_certification_test.rs; certifications/rayon_propagation_certification_test.rs |
| Parent/child fan-out, repeated-route dedup, multi-sink channel, intentional multi-channel duplicate Arc identity, failure continuation, detachment, unique physical flush | channel_certification_test.rs; certifications/channel_fanout_certification_test.rs; engine_test.rs |
| Independent concurrent span pairing, typed start/message, null end, one admission per span, frozen scope/sinks, selection mutation, finish/unwind/failure | span_certification_test.rs; span_test.rs; sinks/jsonl_certification_test.rs |
| Lazy worker/file, unused flush, one off-producer worker, periodic flush under traffic | sinks/jsonl_test.rs; sinks/jsonl_certification_test.rs |
| Bounded chunks, >8 caps of gated backlog, FIFO, stats, explicit barrier and disconnect drain | sinks/jsonl_certification_test.rs::bounded_backlog_fifo_barrier_and_disconnect_drain_real_file |
| Actual file framing, Unicode/escaping/nested/array/message/point/span roundtrip and independent v1 validation | certifications/jsonl_roundtrip_certification_test.rs; certifications/src/lib.rs |
| Multi-producer exact integrity and per-producer order (not a fictitious global thread order) | certifications/tests/multi_producer_fifo.rs (16 producers, 48000 records) |
| Definitely-before/racing/definitely-after FIFO barrier; one flush per shared physical sink | certifications/tests/flush_barrier.rs (50000 before, 1000 racing, 1000 after; file flush handshake, no sleeps) |
| Lazy open/write/flush/panic faults, sticky first error, no retry, checked engine error count, healthy fan-out, concurrent failure barriers/Drop | sinks/jsonl_certification_test.rs; sinks/jsonl_test.rs |
| Repeated newest-suffix retention, complete lines, hysteresis, oversized newest, append, allowed unmatched span end | sinks/jsonl_certification_test.rs; sinks/jsonl_test.rs |
| Deep valid payload history has no implicit Value parser depth restriction | sinks/jsonl_certification_test.rs::retention_accepts_valid_deep_payload_without_an_implicit_format_depth_limit |
| No silent corruption repair during retention | sinks/jsonl_certification_test.rs::retention_is_a_newest_complete_suffix_across_cycles_and_rejects_corrupt_history |
| Process-local last-owner Drop and explicit global flush survive real child exit | certifications/tests/process_shutdown.rs (40000 records each) |
| Many concurrent scope/type/policy executions: exact budgets, legal unique semantic IDs, Memory/JSONL equality | certifications/concurrent_scopes_certification_test.rs (8 parents × 4 children × 250 inputs/type) |
| Full public release path: derived types, tags/levels/exclusions, threads, optional Rayon, mixed policy, spans, overlapping routes, generation mutation, explicit barrier, independent file consumer | certifications/runtime_end_to_end_certification_test.rs + isolated scenario child |
| Actual batch performance evidence, enqueue vs engine-barrier completion, alternative caps | BATCH_BENCHMARKS.md; benches/batch.rs |

## Isolation and synchronization

Every test mutating global() runs its scenario in a fresh scenario executable process.
There is no singleton reset, production test hook or execution-order dependency. Parents
independently parse files after successful child exit and compare the full immutable record
multiset to the child's MemorySink capture. Local-engine tests may run in parallel.

Barriers, joins, writer events and captured contexts establish correctness boundaries.
Timeouts only diagnose failure to make progress; there are no sleep-based concurrency
expectations or machine-dependent performance thresholds. Counting-policy winner identities
are scheduling-dependent: tests require the exact budget, legal input membership, unique IDs
and correct scope, not a deterministic winner across threads. Point input sets are reconstructed
independently. Cross-sink ordering between concurrent producers is not promised; comparisons
use multisets while dedicated single-sink tests enforce FIFO/per-producer order.

Cargo discovery was also checked by temporarily breaking a fan-out assertion: the dedicated
Cargo target failed with exit 101, then passed after restoration. No assertion remains disabled.

## Acceptance evidence

Validated on the current bounded implementation and production guide: default workspace
tests (84 runtime, 11 cross-module/process certifications, 2 derive tests, 14 doctests);
all-features workspace tests (88 runtime including 23 owner certifications,
12 cross-module/process certifications, 2 derive tests, 14 doctests). The benchmark-report
integration test also passes. All seven public examples compile and execute with all
features; the six non-Rayon examples also compile and execute with default features.
The dedicated default/all-features package gates, owner subset, Cargo discovery, fmt,
both workspace checks and warning-denying all-targets Clippy also pass.
No certification is ignored. Benchmark raw output and limitations are in BATCH_BENCHMARKS.md.

## Findings and limits

Certification exposed retention silently hiding malformed preexisting JSONL during compaction.
The background file owner now validates existing complete JSON object lines before append
when retention is enabled and reports a sticky InvalidData error without changing history.
This adds a one-time streaming read of existing retained files, not producer I/O. FORMAT.md v1
is unchanged. Without retention, preexisting corruption is not repaired or retrospectively
validated; an independent consumer must report malformed history.

The scope-wide policy counter remains owned by ScopeState per concrete observation TypeId,
including when the same captured scope is used with independent engines. Engines have distinct
plans/configuration/process identities; changing that counter ownership would be a policy
semantic change and is deliberately not part of this optimization.

Not guaranteed: fsync/power-loss durability; unflushed global static persistence on exit;
multiple independent writers to the same physical file; bounded producer backlog/backpressure;
atomic multi-sink rollback/retry; span completion after abort/leak/I/O failure/retention;
async task-local propagation or distributed tracing. Batch count bounds records/messages,
not arbitrary user payload bytes or enqueue latency. Performance results include a measured
single-producer Rich regression; see BATCH_BENCHMARKS.md rather than assuming universal speedup.
