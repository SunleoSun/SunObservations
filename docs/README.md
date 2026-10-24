# Documentation map

Start with the [framework human guide](../README.md): ordinary certification first,
then real production capture, followed by task-oriented feature examples.
The map below separates guidance, reference, tooling and version-specific evidence.

## Current guidance and reference

| Document | Role | Read when… |
| --- | --- | --- |
| [README](../README.md) | Primary human guide | Learning and composing all three crates |
| [Operations](OPERATIONS.md) | Focused user guide | Configuring delivery, retention, shutdown or a custom sink |
| [Capture](CAPTURE.md) | Technical/API reference | Using scoped capture, snapshots, queries, strict decode and provenance |
| [Certification semantics](CERTIFICATIONS_API.md) | Technical/API reference | Choosing preparation, comparison, acceptance and analysis semantics |
| [FORMAT v1](FORMAT.md) | Technical reference | Reading/writing persisted evidence and pairing spans |
| [Agent skills](AGENT_SKILLS.md) | Operational/tooling reference | Installing and synchronizing package-owned API manuals |

Exhaustive agent API inventories remain package-owned:
[certifications](../crates/sun-observations/agent-skills/certifications/SKILL.md) and
[evidence](../crates/sun-observations/agent-skills/sun-observations/SKILL.md).
Root skill files are synchronized mirrors, never separate authorities.

## Release and benchmark evidence

| Document | Role / measurement boundary |
| --- | --- |
| [Framework release validation](RELEASE_VALIDATION.md) | Release evidence: executable owner matrix, release commands, historical acceptance snapshots; not the application proof API |
| [Benchmark charts](BENCHMARK_CHARTS.md) | Benchmark evidence navigation and visualizations; each chart retains its dataset boundary |
| [Runtime benchmarks](BENCHMARKS.md) | Runtime benchmark evidence: principal run and separately labeled follow-ups, not one normalized dataset |
| [Bounded writer](BATCH_BENCHMARKS.md) | Specific runtime follow-up: before/after and alternative chunk caps |
| [Capture/cache](CAPTURE_BENCHMARKS.md) | Current-representation benchmark evidence: scoped capture, retention, cached numeric views |
| [Generative certifications](GENERATIVE_CERTIFICATION_BENCHMARKS.md) | Separate planned execution, lazy planning, materialization, checks and capture measurements; semantic verification map |

“Current” means the represented contract/implementation, not a fresh measurement
of every checkout. Recorded machine timings never become correctness thresholds,
application capacity promises, or instructions to optimize without workload evidence.
Benchmark and raw CSV artifacts are retained; see each report for reproduction limits.

## Historical evidence

| Document | Historical scope |
| --- | --- |
| [Analysis benchmarks](history/ANALYSIS_BENCHMARKS.md) | Earlier algorithm/extension and grouped projection representation; original 240 + 336 raw samples |
| [Certification benchmarks](history/CERTIFICATION_BENCHMARKS.md) | Later prepared-source dataset, 348 samples, but before scoped capture and numeric-result caching |
| [Performance investigation](history/PERFORMANCE_INVESTIGATION.md) | Investigation dated 2026-10-20 at checkout 283b6448ec9008f175ff4540a3b4a7dc913845f5; provisional experiments, not current guidance |

The next measurement stage after the historical certification dataset is the
[capture/cache report](CAPTURE_BENCHMARKS.md). These runs differ in representation,
operation boundaries and host scheduling; chronology alone is not a speedup proof.
Old metric identifiers and experimental conclusions remain evidence of their own runs.
