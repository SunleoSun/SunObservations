# SunObservations

One Rust package for typed runtime evidence, production capture, and generic semantic certification. All normal capabilities, including Rayon propagation, are available without Cargo features.

```toml
[dependencies]
sun-observations = "0.1"
# Only needed for your serializable Observation payloads:
serde = { version = "1", features = ["derive"] }
```

## Certify ordinary values

The generic certification layer is independent of runtime concepts and state. Prefer canonical results and artifacts; facts record evidence, expectations decide acceptance, and one final assertion aggregates all checks.

```rust
use sun_observations::certifications::*;
let rows = [1u64, 2, 3];
let mut proof = certification("exported rows");
let source = proof.collection("output", &rows);
proof.check("count", source.count().expect(3usize));
proof.check("identity", source.unique(|id| *id).expect_valid());
proof.assert_certify();
```

## Execute a production contract over inputs

Keep eager proofs unchanged; attach an executor for repeated execution. Data-source-only execution does not require `DomainInput`; derive it when a Domain is needed. The symbolic `case` root carries input/output types without borrowing the runner.

```rust
use sun_observations::certifications::*;
#[derive(DomainInput)]
struct Request { count: usize }
fn execute(input: &Request) -> Result<Vec<usize>, &'static str> {
    if !(1..=9).contains(&input.count) { return Err("invalid count"); }
    Ok((0..input.count).collect())
}
let mut c = certification("production count").executor(execute);
c.data_source([Request { count: 3 }]);
c.domain(domain!(Request { count.in_range(1..=9) }));
c.cases_budget(32);
let case = c.case();
c.check(fact!(case.result().len())
    .expect(fact!(case.input().count))
    .description("one output per requested item"));
c.assert_certify();
```

Use `.executor_with_seed(|input, seed: u64| ...)` when fixtures need deterministic case-local variation: generated seeds use canonical case coordinates, real seeds use registered data-source ordinals, and neither depends on other executions. Ordinary `.executor(...)` needs no RNG. Declare `c.require_pairwise_coverage();` to require all feasible enum values and pairwise combinations in normal `report()` / `assert_certify()` results, without increasing budgets. Missing or zero pairwise catalogs and incomplete coverage fail closed.

Only inputs are generated. Real inputs run first and prefill coverage; generated inputs use the same executor. Positive inputs must return `Ok` and satisfy checks; negative inputs must return `Err`, without running positive checks. Panics fail the case. Wrap the executor body in `capture_observations` to additionally use `case.observations::<T>()`; capture completeness remains mandatory even for expected rejection.

Domains support named structs, enum payloads, recursive collections, conditional branches, discrete choices, numeric ranges/unions and typed scalar relations. Every active field needs exactly one constructive specification. Compilation fails closed on incomplete, ambiguous, unsatisfiable or unsupported construction; it never certifies a partial solver result. Constructive arithmetic and search are bounded, and generated collections are limited to 1,000,000 elements each. Derive currently accepts concrete (non-generic) types. Shrinking, regex/string generation, maps/sets, uniqueness and collection ordering are not provided.

Success is silent. `SUN_CERT_REPORT=1` requests compact coverage diagnostics; `SUN_CERT_SEED` and `SUN_CERT_CASES_BUDGET` override the default seed (184729361) and case budget (64). Local `.seed(...)` / `.cases_budget(...)` take precedence. `.failure_budget(...)` defaults to five representative failing cases and stops further materialization/execution when filled. Mandatory real cases may exceed the case budget; registering a Domain requires some generated capacity. Reports are snapshots returned by consuming `c.report()`, or asserted once with `c.assert_certify()`.

## Capture typed production evidence

Runtime entrypoints live at the crate root. The evidence layer owns capture, snapshots, queries, strict typed decoding and provenance, adapting evidence to generic sources without duplicating analysis or acceptance. The derive macro is re-exported; no separate dependency is needed.

```rust
use serde::{Deserialize, Serialize};
use sun_observations::{observe, Observation, ObservationError};
use sun_observations::evidence::{capture_observations, CaptureConfig};
use sun_observations::certifications::try_fact;

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::prepared", tags("evidence"))]
struct Prepared { row: u64 }

fn execute() -> Result<u64, ObservationError> {
    observe!(Prepared { row: 1 })?;
    Ok(1)
}
let run = capture_observations(CaptureConfig::new().activate_tag("evidence").unwrap(), execute);
let typed = run.to_typed::<Prepared>()?;
let mut proof = run.certification("production transfer");
proof.check("result", try_fact("execute", run.result().as_ref().copied()).expect(1u64));
proof.check("captured", typed.source("prepared").count().expect(1usize));
proof.check("row", typed.source("prepared").single_map(|row| row.row).expect(1u64));
proof.assert_certify();
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `run.certification` for completeness-aware proofs: eviction, emission errors and unfinished contexts/workers/spans fail closed. Join workers and finish spans inside the capture closure.

## Agent skills

```text
cargo install sun-observations --bin cargo-sun-observations-skills
cargo sun-observations-skills sync
cargo sun-observations-skills sync --skills-dir <PATH>
```

The package ships four atomic agent manuals: [certification methodology](agent-skills/certifications-methodology/SKILL.md), [certification API](agent-skills/certifications-api/SKILL.md), [Domain API](agent-skills/domain-api/SKILL.md), and [Observations API](agent-skills/observations-api/SKILL.md). They come from the consumer's exact active resolved sun-observations package, not the installed binary version. Multiple active identities fail before writes. Default destination is the consumer workspace .agents/skills; custom relative paths use the invocation directory.

Package-local [examples](examples) cover runtime, scopes, routing, sinks, threads, Rayon, spans, capture and keyed statistical certification.
