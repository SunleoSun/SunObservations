---
description: Domain compilation ownership, indexed selection coordinates, semantic preflight, and enum feasibility symmetry invariants.
---
# Domain compilation

The canonical owner is crates/sun-observations/src/certifications/domain_compilation.rs; domain.rs owns typed sources and exact selection identity, and domain_classification.rs owns coverage coordinates. Compilation prepares immutable source candidates and per-variable source/predicate indexes before solving. Candidate preparation and membership must not be rebuilt for every solve or selected coordinate.

ValueSelection preserves three distinct identities: exact structural membership, first original source position for Discrete regions, and deduplicated/normalized anchor position for slots. Enum scalar comparison ignores payloads although selected membership does not. f32 anchor normalization is prepared once against the variable schema. Signed zeros compare equal. Do not substitute the public DomainValueKey::Composite for full structural identity.

Conditional preflight groups resolved scalars by source membership, every relevant predicate outcome, and zero/nonzero collection activity. Unknown distinguishable operands retain exact identity. Unconditional, unique, non-structurally-gated singleton sources can resolve otherwise unknown operands. Enum identities remain exact in preflight because its contexts also feed generation and conditional coverage, not just completeness validation. SEARCH_LIMIT and CONTEXT_LIMIT bound genuine remaining semantic state, not the number of equivalent selected lengths.

Positive enum pair feasibility remains a canonical solver decision. Residual regions reuse results only when source membership, unresolved operands, predicate outcomes, and structural/rule activity agree. Equality-only enum permutation classes also preserve source membership, schema availability, and constant comparisons; payload/structural tags are singleton classes. Ordered enum comparisons disable permutation reduction. Bucket-local pair classes may reuse feasibility booleans, never substitute a representative's forced assignment into another pair's generation.

The Domain owner certification contains large selected-length/f32 work bounds, direct/transitive/conditional enum-pair bounds, source/payload/ordering controls, and a genuinely distinct unresolved-context bound control. Thread-local test counters measure candidate construction, anchor source visits, region evaluations, and solver work. Existing composition certifications own execution/coverage boundary guarantees.
