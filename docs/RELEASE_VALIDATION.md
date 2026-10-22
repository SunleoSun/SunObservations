# Framework release validation

Release evidence and executable certification matrix for **SunObservations itself**.
This is not the application-facing `sun_observations::certifications` tutorial; use the
[human guide](../README.md#certifications) for that.

The suite is executable, not a collection of placeholders. Owner-adjacent modules are
compiled by the runtime lib test harness; the workspace package
sun-observations-certifications supplies public cross-module, downstream and OS-process
checks. Existing regression tests remain part of the release gate, especially the allocation,
context lifecycle, retention hysteresis and compile-diagnostic checks listed below.

For application usage and operational boundaries, see the [framework human guide](../README.md).

## Publication preparation

The only publishable framework packages are `sun-observations-derive` and
`sun-observations`, in that release order. The `sun-observations-certifications`
package is internal (`publish = false`). Do not publish as part of validation.

The main artifact includes its package-local README/rustdoc, both canonical skills,
all executable examples, and all active benchmark harnesses. No compile-time include
may reach outside its package directory. Before the derive version exists in the
registry, verify the main artifact locally with a patch to the packaged derive;
normal unpatched Cargo verification requires publishing derive first.

The workspace is released under the MIT license; publishable packages inherit
`license = "MIT"` from the workspace manifest and the repository license text lives in
`LICENSE`.

## Compact capture and acceptance contracts

| Contract | Executable owner |
| --- | --- |
| Global typed lazy emission, canonical selector/exclusion/policy/routing | crates/sun-observations/src/macros_certification.rs and engine/channel/plan certifications |
| Scoped success/error/unwind, exclusive nesting, parallel/thread isolation, ignored serialization failures, escaped contexts/spans and automatic eviction prerequisites | crates/sun-observations/src/evidence/capture_certification.rs |
| Rayon propagated capture isolation | certifications/rayon_propagation_certification.rs |
| Bounded oldest-batch retention, order, exact stats, concurrent writes, drain/snapshot identity | crates/sun-observations/src/sinks/memory_certification.rs |
| Equality shorthand, close_to, check_fact_expect parity, factories, all failure classes, correlation/fit/difference and typed correspondence | crates/sun-observations/src/certifications/ergonomics_certification.rs |
| One preparation/decode, stable compact numeric caches, bounded million-record diagnostics, dropped success reports | source_certification.rs, framework_certification.rs and dataset_certification.rs in their owning modules |
| Skill markers/stale API rejection, downstream compilation/execution, raw benchmark matrices | certifications/documentation_certification.rs |

### Intentional low-level usages

Explicit engines, sinks, routes, selectors and closure emission remain in runtime owner tests,
OS-process persistence/routing fixtures and runtime benchmarks: their subject is delivery,
configuration, isolation, cache generation, flushing or fan-out. The analysis snapshot/drain
fixture intentionally retains a MemorySink to prove frozen provenance after drain; ordinary
analysis captures and production examples use scoped capture. The capture benchmark's
explicit engine is the configured-delivery control. Independent engines are for
proving runtime delivery/isolation, not ordinary production capture. Runtime examples configure global delivery once and emit typed macros.

The sole local close helper is in generic numerical bootstrap test_support: independent
floating-point oracle tests deliberately avoid certifying their own math with itself.
Equal<T> remains in generic framework bootstrap tests to exercise the advanced expectation.
Obsolete API spellings occur only as rejection strings in documentation certifications;
there is no compatibility implementation or ordinary consumer left. Historical benchmark
prose/data remains historical rather than being relabeled as a new API run.

Direct assertions are limited to framework self-tests, proc-macro code generation/grammar and downstream compile fixtures, `plan_mechanism.rs` allocation/cache identity instrumentation, physical-line framing and child-process success prerequisites, and `process_shutdown.rs` / `console_process.rs` exit/drain checks. Writer-thread identity, reentrant Drop and timeout assertions are fail-fast scaffolding inside otherwise aggregated runtime certifications; they do not define a second semantic acceptance path.

## Required release commands

Run from the workspace root:

~~~sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p sun-observations --lib certification
cargo test -p sun-observations --test skill_sync_certification
cargo test -p sun-observations-certifications --test documentation_certification
cargo build -p sun-observations --examples
cargo clippy --workspace --all-targets -- -D warnings
cargo package -p sun-observations-derive
cargo package -p sun-observations --list
~~~

`sun-observations` has an exact registry-version dependency on `sun-observations-derive`.
For the first release, unpatched main-package verification cannot succeed until the matching
derive version exists in the registry. Publish/verify the derive package first; then run a normal
`cargo package -p sun-observations` (or `cargo publish -p sun-observations --dry-run`) before
publishing the framework package.

Inspect discovery with:

~~~sh
cargo test -p sun-observations --lib -- --list
cargo test -p sun-observations-certifications -- --list
~~~

Rayon integration is part of the ordinary framework build; there is no feature gate to enable.
The temporary downstream crate is compiled and run offline with a genuinely renamed sdk
dependency; it needs the already-fetched workspace dependencies and a working Cargo/toolchain.
It is not an ignored/manual example.

## Contract matrix

Paths below are under crates/sun-observations/src unless prefixed certifications/.
Each row is enforced by the full release commands, not solely the name-filtered owner subset.

| Production contract | Executable evidence |
| --- | --- |
| Explicit semantic identity, exact/descendant/segment boundary, invalid route atomicity | namespace_certification.rs; routing_certification.rs |
| Derive INFO/WARN/ERROR, generic Serializable, invalid/missing/duplicate/unknown metadata | observation_certification.rs + fixtures; certifications/tests/derive_downstream.rs (temporary renamed dependency, compile-pass/run and 10 compile-fail cases) |
| Lazy union activation, global/local exclusion veto, WARN/ERROR selection, Never | engine_certification.rs; plan_certification.rs |
| Sinkless callbacks, serialization error before dispatch, checked sink failures, proc/clocks, retired engine identities | engine_certification.rs |
| Shared parent/child plan identities, warm zero-allocation rejections, no payload evaluation | plan_mechanism.rs (allocation-counting allocator and identity assertions) |
| Four-entry TLS eviction, distinct types/engines/nested scopes, reentrant destruction without TLS/config borrows | plan_certification.rs; context_certification.rs |
| Generation invalidation for selection/routes/attachment/detachment with counter continuity, already-warm workers | plan_certification.rs; certifications/tests/configuration_churn_certification.rs (8 workers, 7 synchronized phases) |
| Scope nesting, out-of-order drop, captured lifetime, explicit empty context, no override leakage | context_certification.rs; scope_certification.rs |
| Exact concurrent Always/Never/FirstN/EveryN budgets, saturation, rejected vs failed admission counting | scope_certification.rs; engine_certification.rs |
| Simultaneous parents, transitive public OS-thread propagation, plain-thread isolation, unwind and parent restoration | context/thread_certification.rs; certifications/thread_propagation_certification.rs |
| Construction-bound independent Rayon pools, nested work, strict shared budgets, empty pool, worker-local scope, caller restoration | context/rayon_certification.rs; certifications/rayon_propagation_certification.rs |
| Parent/child fan-out, repeated-route dedup, multi-sink channel, intentional multi-channel duplicate Arc identity, failure continuation, detachment, unique physical flush | channel_certification.rs; certifications/channel_fanout_certification.rs; engine_certification.rs |
| Independent concurrent span pairing, typed start/message, null end, one admission per span, frozen scope/sinks, selection mutation, finish/unwind/failure | span_certification.rs; sinks/jsonl_certification.rs |
| Lazy worker/file, unused flush, one off-producer worker, periodic flush under traffic | sinks/jsonl_certification.rs |
| Bounded chunks, >8 caps of gated backlog, FIFO, stats, explicit barrier and disconnect drain | sinks/jsonl_certification.rs::bounded_backlog_fifo_barrier_and_disconnect_drain_real_file |
| Actual file framing, Unicode/escaping/nested/array/message/point/span roundtrip and independent v1 validation | certifications/jsonl_roundtrip_certification.rs; certifications/src/lib.rs |
| Multi-producer exact integrity and per-producer order (not a fictitious global thread order) | certifications/tests/multi_producer_fifo_certification.rs (16 producers, 48000 records) |
| Definitely-before/racing/definitely-after FIFO barrier; one flush per shared physical sink | certifications/tests/flush_barrier_certification.rs (50000 before, 1000 racing, 1000 after; file flush handshake, no sleeps) |
| Lazy open/write/flush/panic faults, sticky first error, no retry, checked engine error count, healthy fan-out, concurrent failure barriers/Drop | sinks/jsonl_certification.rs |
| Repeated newest-suffix retention, complete lines, hysteresis, oversized newest, append, allowed unmatched span end | sinks/jsonl_certification.rs |
| Deep valid payload history has no implicit Value parser depth restriction | sinks/jsonl_certification.rs::retention_accepts_valid_deep_payload_without_an_implicit_format_depth_limit |
| No silent corruption repair during retention | sinks/jsonl_certification.rs::retention_is_a_newest_complete_suffix_across_cycles_and_rejects_corrupt_history |
| Process-local last-owner Drop and explicit global flush survive real child exit | certifications/tests/process_shutdown.rs (40000 records each) |
| Many concurrent scope/type/policy executions: exact budgets, legal unique semantic IDs, Memory/JSONL equality | certifications/concurrent_scopes_certification.rs (8 parents × 4 children × 250 inputs/type) |
| Full public release path: derived types, tags/levels/exclusions, threads, optional Rayon, mixed policy, spans, overlapping routes, generation mutation, explicit barrier, independent file consumer | certifications/runtime_end_to_end_certification.rs + isolated scenario child |
| Actual batch performance evidence, enqueue vs engine-barrier completion, alternative caps | BATCH_BENCHMARKS.md; benches/batch.rs |

## Analysis certification

Ownership is explicit inside sun-observations: certifications owns generic integrity/comparison/
statistics/relations/grouping/anomalies/lag/expectations/aggregation, independent of runtime
concepts and state; evidence owns capture, snapshots, envelope querying, strict decoding
and provenance, composing runtime and certification. No runtime/persisted
FORMAT v1 behavior changes. Public behavior certification uses the generic framework,
while its own bootstrap/reference tests use direct Rust assertions without circularity.

| Contract | Executable evidence |
| --- | --- |
| Generic scalar/struct/collection/empty sources, naming and criteria | crates/sun-observations/src/certifications/framework_certification.rs |
| Tuple and dedicated composite keys, unordered sources, missing/duplicate evidence, dependent errors | framework_certification.rs plus join_certification.rs in src/certifications |
| One key/field projection per preparation; repeated comparison retains same mapping; cached group projection | framework_certification.rs |
| Numeric/robust summaries, population/sample moments, quantiles, MAD, cancellation, nonfinite/overflow/underflow and independent two-pass references | sun-observations/src/certifications/stats_certification.rs |
| Counts, uniqueness, compressed contiguous ranges, finite/range/positive/nonnegative criteria | sun-observations/src/certifications/integrity_certification.rs |
| Sorted/generic/dense/sparse joins versus independent BTreeMap oracle | sun-observations/src/certifications/join_certification.rs |
| Pearson/Spearman ties/signed zero, covariance/OLS/R², difference/residual, bounded disagreements, independent math reference | sun-observations/src/certifications/relation_certification.rs |
| Zero-MAD explicit policy, modified-z reference, jumps/thresholds/ties | sun-observations/src/certifications/anomaly_certification.rs |
| Positive/zero/negative lag, boundaries/interior gaps, duplicates, undefined/invalid/extreme keys, single projection and exhaustive reference | sun-observations/src/certifications/lag_certification.rs |
| Collect-all HARD+STATISTICAL+ERROR, nested paths, direct report inspection, silent successful Display, final assertion caller location | sun-observations/src/certifications/framework_certification.rs |
| 1M-row bounded failure report, success aggregation without retained giant reports | sun-observations/src/certifications/framework_certification.rs |
| Fixed snapshot/Arc identity, all envelope selectors, strict source errors, decode-once operations | sun-observations/src/evidence/dataset_certification.rs |
| Observation groups preserve generic source-local identity, summaries and explicit per-group failures | sun-observations/src/evidence/grouping_certification.rs |
| Real production capture, wrong-direction/scale controls, negative ambiguity, 100k/1M-record traces, group sums and lag after drain | certifications/analysis_certification.rs |
| Ordinary input + production internal evidence + ordinary output, hierarchical transfer, reference formula, rejection category and forbidden downstream absence | crates/sun-observations/examples/skill_workflows.rs; certifications/documentation_certification.rs |
| Canonical skill manifests/mirrors/API families; skill and focused-reference Rust blocks compiled downstream; README doctests including capture; local Markdown paths/anchors, guide order, historical classification and benchmark medians checked | certifications/documentation_certification.rs |
| End-to-end runtime delivery/scope/admission/domain correspondence/forbidden namespace under one proof | certifications/runtime_end_to_end_certification.rs |

Generic bootstrap/reference tests, adapter capture/snapshot/decode/grouping certifications,
public analysis integration and documentation/API certifications are discovered and executed
by the workspace gate. The runtime, derive and process-level matrices remain part of that gate.
Statistical references use relative/absolute tolerances; timing is never a correctness threshold.

[Pre-cache certification benchmark evidence](history/CERTIFICATION_BENCHMARKS.md) preserves 348 samples through 1M,
ordinary/adapted overhead, prepare-once versus repeated preparation, bounded formatting,
grouping/anomalies/lag and storage estimates. The current harness runs with
`cargo test -p sun-observations --release --bench certifications -- --measure`;
this does not recreate the pre-cache samples. Current-representation capture/cache
measurements are separate in [CAPTURE_BENCHMARKS.md](CAPTURE_BENCHMARKS.md).
[Earlier algorithm/extension measurements](history/ANALYSIS_BENCHMARKS.md) remain historical evidence.

## Human-presentation production group

All paths are relative to the runtime src directory unless prefixed certifications/.
These checks extend the structured production matrix; FORMAT v1 and the JSONL writer
are unchanged. No test uses timing sleeps to establish ordering.

| Production contract | Executable evidence |
| --- | --- |
| Exact renderer/build/serialization counts for rejected, excluded, Never, sinkless, Memory, JSONL, Memory+JSONL, one/two human and human+JSONL routes | render_certification.rs::exact_render_laziness_and_real_structured_sink_isolation |
| Sentinel received by humans but absent from real JSONL and Memory capture; semantic message/data retained | render_certification.rs::exact_render_laziness_and_real_structured_sink_isolation |
| Once-per-attachment capability discovery without engine/scope locks, route addition/attachment/detachment invalidation and continuous EveryN counter | render_certification.rs::human_capability_is_cold_and_route_mutations_preserve_counting_policy; attachment_capability_can_reenter_configuration_without_engine_or_scope_locks |
| Serialization failure never renders/dispatches; span start/end remain profiling lifecycle records and never invoke the point renderer | render_certification.rs::serialization_failure_never_renders_or_delivers_and_spans_never_render |
| Legacy structured Sink needs only write; custom human Sink sees shared canonical record/text; two humans and Console render once | certifications/human_delivery_certification.rs |
| Independent TableStart/Row/Row/End observations, exact ordered Console table, four independently validated v1 JSONL records, no spans | certifications/human_delivery_certification.rs::public_table_stream_shares_render_and_record_without_changing_v1_or_legacy_sinks |
| Wrapper construction/clones allocate zero, preserve renderer buffer/Arc identity; absent-text human path equals structured allocation cost; render-panicking type still has allocation-free rejection | plan_mechanism.rs |
| Lazy stdout/stderr, unused flush, exact fallback format, literal multiline/empty/already-terminated custom text | sinks/console_certification.rs::lazy_targets_default_format_and_literal_multiline_text |
| Off-producer single worker, >4 chunks of gated backlog, FIFO, exact prefix flush snapshot, last-owner disconnect drain | sinks/console_certification.rs::gated_backlog_fifo_flush_boundary_and_drop_drain_off_producer |
| 16 concurrent producers, 16000 records, exact per-producer order and indivisible multiline records | sinks/console_certification.rs::concurrent_producers_preserve_each_producer_order_and_multiline_record_integrity |
| Sticky write/flush/panic errors, no retry, 16 already-queued concurrent public barriers all released, Drop completes | sinks/console_certification.rs::write_flush_and_panic_failures_are_sticky_and_release_all_barriers; failure_releases_concurrent_already_queued_public_flush_barriers |
| Checked error count and healthy structured fan-out after Console failure | sinks/console_certification.rs::failed_console_does_not_prevent_healthy_structured_fanout |
| Actual stdout/stderr target selection, local Drop and explicit global flush survive child exit (4000 multiline records each) | certifications/tests/console_process.rs + certifications/src/bin/console_scenario.rs |
| Same-machine before/after structured controls; default/custom/mixed fan-out and 1/4/16 producer enqueue versus service; raw CSV and generated charts | BENCHMARKS.md human follow-up; benches/human.rs; benches/runtime.rs |

## Isolation and synchronization

Global delivery-configuration scenarios run in a fresh scenario executable process.
Scoped capture certifications isolate evidence through their private context attachment.
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

## Historical acceptance snapshots

The following counts and completed commands belong to earlier implementation
revisions. They are retained release evidence, not the current test inventory or
a claim that these commands were run on every subsequent checkout. Current release
acceptance requires running the commands above.

The analysis addition passes all required workspace checks, default/all-features tests,
warning-denying Clippy, 54 analysis tests plus its doctest, four documentation
certifications and both executable analysis examples. The extended release harness
retains 336 raw samples (including typed/source, grouping and ±2/±8/±32 lag windows
at 1k/10k/100k/1M); the original 240 baseline samples remain separate. See
[historical analysis report](history/ANALYSIS_BENCHMARKS.md) for measured evidence, materialization estimates and noise.
Runtime, derive and FORMAT v1 source contracts remain unchanged.

Validated human-delivery revision: 96 default/100 all-features runtime tests,
14 default/15 all-features public cross-module/process tests, 2 derive tests, and
17 doctests; the separate all-features owner gate passes all 33 certifications.
Default/all-features workspace tests, the all-features public package gate, fmt,
workspace and all-features/all-targets checks, warning-denying Clippy, and benchmark
report integration pass. All eight public examples build and execute with all
features; all seven non-Rayon examples also build and execute with default features.
The human example's exact table output and separate stderr status were checked.
The five-sample benchmark targets and all eight report-validated SVGs pass.
No certification is ignored.
Human follow-up raw samples, variation and destination limitations are in BENCHMARKS.md;
historical JSONL batch evidence remains in BATCH_BENCHMARKS.md.

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
