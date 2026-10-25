---
name: domain-api
description: Use the typed Domain DSL for generated certification inputs. Use when defining DomainInput types, ranges, choices, nested fields, collections, conditions, relations, or pairwise coverage inputs.
---

# Domain client API

Use with sun_observations::certifications::*. Read certifications-methodology first when deciding whether generation is appropriate, and certifications-api for the runner that executes the Domain.

## Workflow grammar

This is structure, not copyable Rust:

~~~text
DOMAIN :=
domain TYPE {
    RULE,
    RULE,
    ...
}

RULE :=
    FIELD.CONSTRUCTIVE
  | FIELD {
        RULE,
        RULE,
        ...
    }
  | ENUM_FIELD {
        ENUM_SELECTION,
        EnumType::Variant {
            RULE,
            RULE,
            ...
        },
        ...
    }
  | COLLECTION {
        length().LENGTH_SPACE,
        each(ELEMENT_SPACE)
    }
  | COLLECTION {
        length().LENGTH_SPACE,
        each {
            RULE,
            RULE,
            ...
        }
    }
  | when(RELATION) {
        RULE,
        RULE,
        ...
    }
  | RELATION

CONSTRUCTIVE :=
    fixed_value(VALUE)
  | selected_values(VALUES)
  | in_range(RANGE)
  | one_of_ranges(RANGES)
  | all_enum_values()
  | selected_enum_values(VALUES)

ENUM_SELECTION :=
    all_enum_values()
  | selected_enum_values(VALUES)

LENGTH_SPACE :=
    fixed_value(VALUE)
  | selected_values(VALUES)
  | in_range(RANGE)
  | one_of_ranges(RANGES)

ELEMENT_SPACE := CONSTRUCTIVE

RELATION :=
    FIELD.equal(OPERAND)
  | FIELD.not_equal(OPERAND)
  | FIELD.less_than(OPERAND)
  | FIELD.less_equal(OPERAND)
  | FIELD.greater_than(OPERAND)
  | FIELD.greater_equal(OPERAND)
~~~

FIELD { RULE, ... }, each { RULE, ... }, variant blocks, and when(RELATION) { RULE, ... } contain ordinary nested Domain rules. FIELD.CONSTRUCTIVE means the corresponding real call such as field.in_range(...). Inside a current-field context, ENUM_SELECTION, LENGTH_SPACE, and ELEMENT_SPACE are unqualified calls such as all_enum_values(), length().in_range(...), and each(in_range(...)); they are not opaque predicates.

## Input types

Derive DomainInput on concrete structs and enums used as generated input. Structs require named fields; generic input types and unions are not supported by the derive.

Built-in DomainInput support includes integer primitives, f32, f64, bool, String, and Vec<T: DomainInput>.

Numeric and ordered fields support ranges and ordered relations. String currently has no regex or arbitrary-string generator; use fixed_value or selected_values. Collections are generated through a length rule plus an element rule.

## Constructive rules

Every active field needs exactly one constructive value space.

- field.fixed_value(value) — one value.
- field.selected_values(values) — explicit scalar values.
- field.in_range(range) — an ordered Rust range accepted by the field type.
- field.one_of_ranges(ranges) — disconnected ordered ranges.
- field.all_enum_values() — all enum variants.
- field.selected_enum_values(values) — selected enum variants.
- field { RULE, ... } — descend into a nested struct, enum, or collection field.
- length() — the usize length field of a collection.
- each(RULE) — one constructive rule for scalar elements.
- each { RULE, ... } — nested rules for structured elements.

For payload enums, enum selection chooses variants; payload fields still need their own constructive rules in the corresponding EnumType::Variant { ... } block.

## Conditions and relations

when(relation) { RULE, ... } activates rules only when its typed relation is true.

Relations:
- equal(operand)
- not_equal(operand)
- less_than(operand)
- less_equal(operand)
- greater_than(operand)
- greater_equal(operand)

Operands must have compatible scalar types. Fields may reference nested paths such as config.window; collection length participates as values.length().

Do not encode arbitrary predicates, baseline templates, or overlapping implicit sources. Incomplete, ambiguous, contradictory, or unmaterializable domains fail closed before generated execution.

## What generation covers

The runner materializes deterministic representative cases from declared value spaces, conditions, relations, enums, and collection shapes. It explores valid behavior and constraint-breaking negative cases that are meaningful for the declared input contract. Enum choices can be covered individually and in feasible combinations; real data_source cases can prefill that coverage.

Do not manually enumerate ordinary cases that the Domain already describes. Keep concrete cases only when the specific example has independent semantic value.

Generation currently has no shrinking and no general regex/string, map/set, uniqueness, or collection-order constraint language. Keep such contracts in concrete inputs or prove them at the semantic owner that already validates them.

## Minimal example

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
enum Mode { Fast, Accurate }

#[derive(Clone, DomainInput)]
struct Request {
    mode: Mode,
    window: usize,
    values: Vec<f64>,
}

fn request_domain() -> Domain<Request> {
    domain!(Request {
        mode.all_enum_values(),
        when(mode.equal(Mode::Fast)) {
            window.in_range(1..=3)
        },
        when(mode.equal(Mode::Accurate)) {
            window.in_range(4..=6)
        },
        values {
            length().in_range(2..=8),
            each(in_range(-1.0..=1.0))
        },
        window.less_equal(values.length()),
    })
}
~~~

Register it with planned.domain(request_domain()). Generation budget, framework seed, checks, pairwise enforcement, and final assertion belong to certifications-api.