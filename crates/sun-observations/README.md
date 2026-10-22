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

Both [runtime/evidence](agent-skills/sun-observations/SKILL.md) and [certification](agent-skills/certifications/SKILL.md) manuals come from the consumer’s exact active resolved `sun-observations` package, not the installed binary version. Multiple active identities fail before writes. Default destination is the consumer workspace `.agents/skills`; custom relative paths use the invocation directory.

Package-local [examples](examples) cover runtime, scopes, routing, sinks, threads, Rayon, spans, capture and keyed statistical certification.
