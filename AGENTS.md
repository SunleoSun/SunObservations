# Project Context & Coding Standards

SunObservations is a Rust workspace for typed structured logging, runtime diagnostics, profiling, and generic semantic certification.
The sun-observations package owns the complete framework: runtime produces evidence; evidence owns Observation access/provenance; certifications owns generic analysis, acceptance, and proof aggregation independently of runtime concepts/state. Only the derive proc macro remains a separate implementation package. The code, persisted format, examples, and certifications define the current contract.

## Code as Contract

The code is the primary contract: prefer clear names, strong types, schemas, validation, constants, and tests over explanatory prose. Do not add comments or contract prose inside tests; express test intent through names, setup, and assertions.

Use comments and Rust doc comments only when they protect non-obvious meaning that must survive future edits, such as:

- domain rules and invariants;
- data-format or serialization contracts;
- ownership, identity, lifecycle, ordering, retry, or concurrency rules;
- cross-crate or public API boundaries;
- performance-sensitive assumptions;
- previous regression traps;
- temporary workarounds with a clear removal condition.

Do not add comments that merely restate a symbol name, signature, type, or obvious local operation.
Do not add comments to unrelated helper code while editing a file.
If behavior changes, update or remove nearby comments in the same change.

All non-trivial public or domain-significant types should have a concise description of what they represent.
Document fields only when their meaning, units, constraints, or interpretation are not obvious from the type and name.

## Single Source of Behavior + Typed Contracts

Treat **Single Source of Behavior + Typed Contracts** as one architecture rule.

- Give every domain fact, invariant, and decision one authoritative semantic owner and one canonical implementation path.
- Compute shared decisions in the narrowest owning layer and pass typed results to consumers.
- Consumers, caches, projections, summaries, indexes, sinks, and adapters may expose derived representations, but must not reimplement or compete with owning behavior.
- Encode semantic distinctions in types. Prefer enums, wrappers, dedicated IDs, structs, tagged values, and validated types over booleans, magic strings, naming conventions, or repeated validation.
- When producer and consumer behavior diverge, fix the semantic owner and make consumers depend on the canonical contract instead of adding reconciliation logic.
- Before adding cross-module logic, identify the single owner of the decision and whether the contract can make invalid substitutions unrepresentable.
- Performance optimizations, batching, caching, and concurrency are allowed only when they preserve canonical behavior, ordering guarantees, and contracts.

Production source boundaries follow semantic ownership. A source file represents one semantic owner or one coherent semantic namespace; do not split files by type kind, implementation phase, helper category, or size alone. Multiple related types/functions belong together when they jointly implement one semantic contract. A large owner may use a module directory, but each child source module must represent a meaningful subordinate semantic owner rather than an arbitrary implementation slice.

## Testing Strategy

Tests must validate important behavior, invariants, edge cases, and regressions rather than implementation trivia.

Add or update focused tests when changing:
- public or cross-crate contracts;
- admission, routing, policy, scope, propagation, sink, or serialization behavior;
- validation or bounds;
- lifecycle, ordering, retry, concurrency, or failure behavior;
- code that previously caused a bug or regression.

Prefer testing non-critical helpers through the important behavior that owns them unless the helper has meaningful branching or independent semantics.

Keep implementation and certifications grouped by semantic ownership:

- Each semantic owner has at most one owner certification module, normally owner-adjacent as <owner>_certification.rs. Deterministic cases, Domain-generated cases, regressions, edge cases, oracle checks, and metamorphic checks are proof techniques inside that one owner certification; they do not create separate owner files.
- Cross-owner contracts belong under crate/src/composition_certifications/. Name files by the semantic relation and keep the _certification.rs suffix, for example prediction_to_decision_certification.rs. Subdirectories are allowed only for meaningful composition semantic namespaces.
- Existing framework bootstrap/self-tests and low-level compile/process infrastructure may use direct Rust assertions when framework self-dependence would obscure the mechanism.
- Inspect executable evidence rather than inferring suite status from filenames.

Canonical portable skills are package-owned and root .agents/skills copies are synchronized mirrors, never independent edit targets:
- crates/sun-observations/agent-skills/certifications-methodology/SKILL.md
- crates/sun-observations/agent-skills/certifications-api/SKILL.md
- crates/sun-observations/agent-skills/domain-api/SKILL.md
- crates/sun-observations/agent-skills/observations-api/SKILL.md

Skill routing is mandatory:
1. Creating, changing, reviewing, or migrating certifications: read certifications-methodology.
2. Writing or changing certification framework client code: also read certifications-api.
3. If a Domain exists or should be considered for family-wide input coverage: also read domain-api.
4. If production Observations, capture, query/decode, or provenance are involved: also read observations-api.
5. Pure Observation API work without certification proof may read observations-api alone.

After editing canonical skills, run cargo sun-observations-skills sync with the installed tool, or cargo run -p sun-observations --bin cargo-sun-observations-skills -- sync locally. Documentation certifications enforce mirror byte equality.

## Execution Discipline

When the user requests a change, investigation, validation, or other multi-step repository task, stay in execution mode until the requested outcome is complete.

Do not stop after discovery, a plan, the first successful edit, one passing check, a failed validation, or an incomplete implementation.
A failed validation or incorrect intermediate implementation means continue fixing the issue when an in-scope recovery path exists.

Reply early only when required input is missing, safety blocks work, or continuation requires a destructive or prohibited action.

## Workspace Startup

For every coding or repository-analysis task:

1. Read this `AGENTS.md`.
2. Call `list_tools` to learn the currently available SunMCP tools.
3. Call `read_memory_index`; an empty index is valid.
4. Read only relevant memory entries collection-first with `read_memory(requests=[{id}, ...])`.
5. Inspect the current source, tests, configuration, and Git state before changing behavior.
6. Use memory to narrow discovery, never as a substitute for current source, schemas, configuration, or tests.

Before the first modification, capture `run_git status` so unrelated user changes are known and preserved.

## Tool Contract

- All repository reading, searching, editing, validation, and Git inspection must use SunMCP tools. Do not fall back to built-in file/search/edit tools.
- Runtime tool discovery is canonical: call `list_tools` for the live tool set, then `get_tool_info(requests=[{name, params_verbosity, examples}, ...])` whenever an exact signature or argument rule matters.
- Never invent tool names, argument names, or enum values.
- Do not maintain or rely on a static tool catalog in this repository.
- Prefer batching SunMCP work when operations can be fully specified up front.
- Use collection-first `requests=[...]` to combine multiple files, queries, symbols, or ranges when the tool supports it.
- Preserve required dependency order. Split work only when a later call's arguments depend on an earlier result.
- Prefer semantic navigation: `list_symbols` / `search_symbols` -> semantic owner -> `read_symbol_source`.
- Use focused `read_file` only when symbol or structural tools cannot provide the needed context.
- Use `search` for text/content and `search_files` for paths.
- Treat skipped files, truncation, and partial output as incomplete evidence.
- Prefer `replace_symbol_source` for whole declarations, `replace_by_lines` for known non-overlapping original line ranges, and `replace` for focused literal/regex edits.
- After a line-based write, re-read the changed region before another line-based edit in the same file; never reuse stale line numbers.
- Prefer dedicated `run_git`, `run_cargo`, symbol, log, and transaction tools over shell wrappers when they express the task directly.
- Command `args` arrays are argv arrays: put each token in its own element.
- Use live `get_tool_info` for current regex, streaming, truncation, and parameter semantics when they matter.

## Project Memory

Read durable project knowledge through `read_memory_index` and `read_memory`.
SunMemory is ordinary workspace content under `sun-memory/` when present; edit it with the same transaction-aware writer tools as other repository files.

For implementation work, update an existing owning memory entry in the same transaction when durable architecture, ownership, lifecycle, persistence, integration, invariant, or validation knowledge becomes stale.

Create a new entry only when it materially reduces future discovery, preserves a non-obvious durable contract, or prevents a realistic implementation mistake.

Never store task progress, diffs, debugging notes, line numbers, or speculation as durable memory.

After memory edits, re-read every changed entry, call `read_memory_index`, verify descriptions and cross-references, and inspect the transaction diff.

## Engineering Discipline

Prefer the simplest correct end-to-end design.
Add complexity only when a proven constraint requires it.

Do not create entities, fields, abstractions, metadata, indirection, configuration, compatibility layers, or extensibility speculatively.
Add a concept only when it has independent semantics or lifecycle, or is required by a concrete behavior, invariant, query, or ownership boundary.

Find the semantic owner and the real production path before editing.
Consumers may expose derived views but must not shadow or reimplement owning behavior.

Across crates and process boundaries, use typed contracts.
Validate once at the correct boundary and pass the canonical form onward.

A semantic change is incomplete until it has been traced through affected typed results, persisted contracts, aggregations/projections, runtime artifacts, and consumers.
Update stale meanings and dropped fields in the same change.

Fail closed on promised effects.
Process or tool success alone is insufficient: verify the expected artifact/state, contract, and critical parameters, and propagate child-process failures.

## Transactions

Before the first modification:

1. Capture Git status.
2. Start one MCP transaction with `start_transaction`.
3. Use that transaction for every task-owned write unless recovery requires a replacement transaction.
4. Preserve unrelated user changes.

Use `revert_transaction` only when intentionally abandoning task-owned changes, not because the implementation is incomplete or validation still needs work.

If a transaction revert is partial, fails, or leaves the transaction inactive:

1. Inspect current Git status/diff and transaction state.
2. Treat the current workspace as authoritative.
3. If the task should continue, start a recovery transaction and finish or repair the implementation.
4. Do not stop merely because the original transaction can no longer be used.

Before finishing:

1. Update any affected stale SunMemory.
2. Run focused validation and broader checks only when justified by the affected contract.
3. Inspect `read_transaction_info(transaction_id, include_diff=true)` and targeted transaction file diffs when useful.
4. Inspect final `run_git status` and `run_git diff`.
5. Fix unintended edits, conflicts, broken formatting, stale meanings, or validation failures.
6. Commit the MCP transaction only when the requested implementation is complete and acceptable.

`commit_transaction` accepts MCP rollback state only.
It does not create a Git commit, stage files, or modify Git history.

## Validation and Failure Handling

On failure, read the full error and classify it as a real regression, a bad expectation, or an environment/tooling problem.
Fix the semantic owner instead of adding fallback behavior that creates a second authority, then rerun the focused check.

Validation must prove the requested effect, not merely that a command exited successfully.
Add or update focused regression tests for behavior changes and bug fixes.

Do not reset or discard unrelated work, rewrite Git history, expose secrets, or stop unrelated production processes.

Final coding replies normally report the summary, changed files, validation performed/results, MCP transaction status, and remaining limitations or risks.

## Coding Standards

### State Encapsulation

Extract logic into a Rust `struct` when it:

1. maintains internal state that changes over time;
2. has multiple functions repeatedly operating on the same group of variables;
3. must enforce strict validation, invariants, or data boundaries;
4. represents one clear isolated concept or lifecycle.

Small, locally used structs should remain close to their primary consumer.

### Scope Strictness

Never add legacy compatibility layers, fallback wrappers, or future-proofing boilerplate without a concrete current requirement.
Write only the minimum code required to satisfy the immediate contract while preserving meaningful error handling.

### Rust

- Prefer workspace dependencies with `{ workspace = true }` where applicable.
- Prefer types and compiler-enforced distinctions over stringly typed conventions.
- Keep public API changes synchronized with examples, persisted contracts, and tests that depend on them.
- Avoid pass-through wrappers that only call another function with the same parameters.
- Use descriptive names; avoid magic values when a named constant carries domain meaning.

## Build and Test Commands

Prefer SunMCP command wrappers instead of shell pipelines or redirection.

Focused Rust validation:

```text
run_cargo(subcommand="test", args=["-p", "<crate>"])
run_cargo(subcommand="check", args=["-p", "<crate>"])
run_cargo(subcommand="fmt", mode="check")
```

Run broader workspace validation only when the scope or affected contract justifies it.