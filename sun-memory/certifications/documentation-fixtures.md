---
description: Executable documentation fixtures, process isolation, and safe reuse of Cargo build artifacts.
---
# Documentation and downstream fixtures

certifications/documentation_certification.rs compiles canonical skill and README Rust examples together in one downstream library fixture. README modules preserve hidden Rustdoc lines, item/statement forms, explicit tests, and no_run type checking. Runnable README tests execute in individually filtered child processes so global runtime configuration cannot leak between examples. The harness verifies child success and exactly one executed test; returned errors, failing children, missing filters, and a panicking compile-only example are independent controls. Unsupported API bindings remain compile-failure controls. Structural documentation checks separately own receiver/type/arity, API ownership, DSL completeness, and canonical/mirror equality.

Documentation and downstream derive fixtures reuse the surrounding Cargo target directory rather than rebuilding dependencies in each temporary project. Downstream invalid Observation declarations compile together, but each expected diagnostic must have a primary span inside its own declaration.

The skill installation certification uses a persistent skill-install target below the shared target directory, not the live workspace target itself. Cargo install may replace its debug CLI executable; sharing that path with parallel tests running the workspace CLI causes Windows executable-lock failures. Installed tool state and consumer projects remain fresh per test.
