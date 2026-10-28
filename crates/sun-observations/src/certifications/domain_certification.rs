use super::domain::compilation::coordinate_key;
use super::*;

#[derive(Clone, Debug, DomainInput)]
struct RelationConditional {
    a: i32,
    b: i32,
    c: i32,
}

#[derive(Clone, Debug, DomainInput)]
struct LengthConditionalRegression {
    items: Vec<i32>,
}

#[test]
fn conditional_preflight_honors_relations_and_every_reachable_length_region() {
    let domain = domain!(RelationConditional {
        a.in_range(0..=2), b.in_range(0..=2), a.less_than(b),
        when(a.equal(1)) { c.fixed_value(5) },
        when(a.not_equal(1)) { c.fixed_value(6) },
    })
    .compile()
    .unwrap();
    for input in [
        RelationConditional { a: 0, b: 1, c: 6 },
        RelationConditional { a: 1, b: 2, c: 5 },
    ] {
        assert!(domain.classify(&input).unwrap().violated.is_empty());
    }
    let witnessed = std::cell::RefCell::new(std::collections::BTreeSet::new());
    let mut runner = certification("reachable relation branches materialize").executor(
        |input: &RelationConditional| {
            let valid = (0..=2).contains(&input.a)
                && (0..=2).contains(&input.b)
                && input.a < input.b
                && input.c == if input.a == 1 { 5 } else { 6 };
            if valid {
                witnessed.borrow_mut().insert((input.a, input.c));
                Ok::<_, ()>(())
            } else {
                Err(())
            }
        },
    );
    runner
        .domain(domain!(RelationConditional {
            a.in_range(0..=2), b.in_range(0..=2), a.less_than(b),
            when(a.equal(1)) { c.fixed_value(5) },
            when(a.not_equal(1)) { c.fixed_value(6) },
        }))
        .cases_budget(40);
    let report = runner.report();
    assert!(report.is_pass(), "{report}");
    assert!(witnessed.borrow().contains(&(0, 6)));
    assert!(witnessed.borrow().contains(&(1, 5)));

    let incomplete = domain!(LengthConditionalRegression {
        items { length().selected_values([0, 1, 2]) },
        when(items.length().less_equal(1)) { items { each(fixed_value(0)) } },
    });
    assert!(incomplete.compile().is_err());
    for budget in [1, 20] {
        let calls = std::cell::Cell::new(0);
        let mut runner =
            certification("incomplete branch").executor(|_: &LengthConditionalRegression| {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(())
            });
        runner
            .domain(domain!(LengthConditionalRegression {
                items { length().selected_values([0, 1, 2]) },
                when(items.length().less_equal(1)) { items { each(fixed_value(0)) } },
            }))
            .cases_budget(budget);
        let report = runner.report();
        assert!(!report.is_pass());
        assert!(report.evaluation_error().is_some());
        assert_eq!(calls.get(), 0);
    }
    let complete = domain!(LengthConditionalRegression {
        items { length().selected_values([0, 1, 2]) },
        when(items.length().less_equal(1)) { items { each(fixed_value(0)) } },
        when(items.length().greater_than(1)) { items { each(fixed_value(1)) } },
    })
    .compile()
    .unwrap();
    assert!(complete
        .classify(&LengthConditionalRegression { items: vec![1, 1] })
        .unwrap()
        .violated
        .is_empty());
}

#[derive(Clone, Debug, DomainInput)]
struct WideLengthConditional {
    n: usize,
    items: Vec<i32>,
}

#[test]
fn conditional_length_preflight_groups_equivalent_reachable_regions() {
    let domain = domain!(WideLengthConditional {
        n.selected_values(0..30),
        items { length().selected_values(0..200) },
        when(n.less_equal(29)) {
            when(items.length().less_equal(1)) { items { each(fixed_value(0)) } },
            when(items.length().greater_than(1)) { items { each(fixed_value(1)) } },
        },
    })
    .compile()
    .unwrap();
    for input in [
        WideLengthConditional {
            n: 0,
            items: vec![],
        },
        WideLengthConditional {
            n: 15,
            items: vec![0],
        },
        WideLengthConditional {
            n: 29,
            items: vec![1; 199],
        },
    ] {
        assert!(domain.classify(&input).unwrap().violated.is_empty());
    }
}

#[test]
fn large_selected_length_compilation_has_bounded_semantic_work() {
    for size in [41, 4100] {
        compilation::take_compilation_work();
        compilation::reset_solve_calls();
        let domain = domain!(WideLengthConditional {
            n.selected_values(0..30),
            items { length().selected_values((0..size).chain(0..size)) },
            when(n.less_equal(29)) {
                when(items.length().less_equal(1)) { items { each(fixed_value(0)) } },
                when(items.length().greater_than(1)) { items { each(fixed_value(1)) } },
            },
        })
        .compile()
        .unwrap();
        let work = compilation::take_compilation_work();
        let solves = compilation::solve_calls();
        assert!(work.candidates <= 16, "{work:?}");
        assert!(work.signatures <= size * 3 + 128, "{work:?}");
        assert!(work.anchor_values <= size * 32 + 1024, "{work:?}");
        assert!(solves <= 16, "{solves}");
        eprintln!("selected lengths={size}; {work:?}; solves={solves}");
        for n in [0, 15, 29] {
            for length in [0, 1, 2, size - 1] {
                let input = WideLengthConditional {
                    n,
                    items: vec![i32::from(length > 1); length],
                };
                assert!(domain.classify(&input).unwrap().violated.is_empty());
                if length > 0 {
                    let wrong = WideLengthConditional {
                        n,
                        items: vec![i32::from(length <= 1); length],
                    };
                    assert!(!domain.classify(&wrong).unwrap().violated.is_empty());
                }
            }
        }
    }
}

#[test]
fn fixed_field_operands_partition_large_selected_values_before_context_limits() {
    compilation::reset_solve_calls();
    let compiled = domain!(WideLengthConditional {
        n.selected_values(0..4100),
        items { length().fixed_value(1) },
        when(n.equal(items.length())) { items { each(fixed_value(0)) } },
        when(n.not_equal(items.length())) { items { each(fixed_value(1)) } },
    })
    .compile()
    .unwrap();
    assert!(compilation::solve_calls() <= 16);
    for n in [0, 1, 2, 4099] {
        assert!(compiled
            .classify(&WideLengthConditional {
                n,
                items: vec![i32::from(n != 1)],
            })
            .unwrap()
            .violated
            .is_empty());
    }
    assert!(domain!(WideLengthConditional {
        n.selected_values(0..4100),
        items { length().fixed_value(1) },
        when(n.equal(items.length())) { items { each(fixed_value(0)) } },
    })
    .compile()
    .is_err());
}

#[test]
fn unresolved_distinct_field_operands_still_fail_closed_at_the_context_bound() {
    let error = domain!(WideLengthConditional {
        n.selected_values(0..4100),
        items { length().selected_values(0..4100) },
        when(n.equal(items.length())) { items { each(fixed_value(0)) } },
        when(n.not_equal(items.length())) { items { each(fixed_value(1)) } },
    })
    .compile()
    .unwrap_err();
    assert!(
        error
            .message
            .contains("conditional preflight exceeds bounded solver capacity"),
        "{error:?}"
    );
}

#[test]
fn indexed_selections_preserve_first_source_coordinates_and_full_structural_identity() {
    let compiled = domain!(f64 { selected_values([-0.0, 0.0, 2.0, 2.0, 3.0]) })
        .compile()
        .unwrap();
    for (value, region, slot) in [(0.0, 0, 0), (2.0, 2, 1), (3.0, 4, 2)] {
        let coordinate = compiled.classify(&value).unwrap();
        assert_eq!(
            coordinate.entries[0].sector.region,
            DomainRegion::Discrete(region)
        );
        assert_eq!(
            coordinate.entries[0].slot,
            CaseDomainCoordinateSlot::Anchor(slot)
        );
    }
    let compiled = domain!(Vec<i32> { selected_values([vec![1], vec![2], vec![1]]) })
        .compile()
        .unwrap();
    assert_eq!(compiled.coverage_catalog().positive_anchors.len(), 2);
    assert!(compiled.classify(&vec![1]).unwrap().violated.is_empty());
    assert!(compiled.classify(&vec![2]).unwrap().violated.is_empty());
    assert!(!compiled.classify(&vec![3]).unwrap().violated.is_empty());
    let compiled = domain!(ConstrainedVariant { selected_values([
        ConstrainedVariant::Impossible { n: 1 },
        ConstrainedVariant::Impossible { n: 2 },
        ConstrainedVariant::Possible,
    ]) })
    .compile()
    .unwrap();
    for n in [1, 2, 3] {
        let coordinate = compiled
            .classify(&ConstrainedVariant::Impossible { n })
            .unwrap();
        assert_eq!(
            coordinate.entries[0].slot,
            CaseDomainCoordinateSlot::Anchor(0)
        );
        assert_eq!(coordinate.violated.is_empty(), n != 3);
    }
}

#[derive(Clone, Debug)]
struct CatalogChoice(usize);
impl DomainInput for CatalogChoice {
    type Fields = DomainField<Self>;
    fn schema() -> DomainSchema {
        DomainSchema::Enum((0..128).map(|_| ("choice", vec![])).collect())
    }
    fn domain_fields(field: DomainField<Self>) -> Self::Fields {
        field
    }
    fn to_domain_value(&self) -> DomainValue {
        DomainValue::Enum(self.0, vec![])
    }
    fn from_domain_value(value: DomainValue) -> Result<Self, AnalysisError> {
        match value {
            DomainValue::Enum(tag, fields) if tag < 128 && fields.is_empty() => Ok(Self(tag)),
            _ => Err(super::domain::domain_error("invalid catalog choice")),
        }
    }
}
impl DomainScalar for CatalogChoice {}
impl DomainEnum for CatalogChoice {}
impl DomainOrdered for CatalogChoice {}
#[derive(Clone, Debug, DomainInput)]
struct SymmetricEnums {
    a: CatalogChoice,
    b: CatalogChoice,
    c: CatalogChoice,
}
fn enum_family(size: usize) -> Domain<SymmetricEnums> {
    domain!(SymmetricEnums {
        a.selected_enum_values((0..size).map(CatalogChoice)),
        b.selected_enum_values((0..size).map(CatalogChoice)),
        c.selected_enum_values((0..size).map(CatalogChoice)),
    })
}

#[test]
fn independent_enum_pairs_require_no_feasibility_solver_validations() {
    for size in [8, 128] {
        compilation::reset_enum_pair_context_checks();
        let compiled = enum_family(size).compile().unwrap();
        assert_eq!(compiled.coverage_catalog().pairwise_total, 3 * size * size);
        assert_eq!(compilation::enum_pair_solves(), 0);
    }
}

#[test]
fn direct_enum_equalities_reuse_positive_feasibility_regions() {
    for size in [8, 128] {
        let fields = SymmetricEnums::domain_fields(DomainField::root());
        let mut spec = enum_family(size);
        spec.add(fields.a.equal(&fields.b), &[]);
        compilation::reset_enum_pair_context_checks();
        compilation::take_compilation_work();
        let compiled = spec.compile().unwrap();
        let work = compilation::take_compilation_work();
        assert!(work.signatures <= size * 4 + 128, "{work:?}");
        eprintln!(
            "enum values={size}; {work:?}; feasibility solves={}",
            compilation::enum_pair_solves()
        );
        assert_eq!(
            compiled.coverage_catalog().pairwise_total,
            size + 2 * size * size
        );
        assert!(
            compilation::enum_pair_solves() <= 16,
            "{}",
            compilation::enum_pair_solves()
        );
        assert!(!compiled
            .classify(&SymmetricEnums {
                a: CatalogChoice(0),
                b: CatalogChoice(1),
                c: CatalogChoice(0)
            })
            .unwrap()
            .violated
            .is_empty());
    }
}

#[test]
fn transitive_enum_equalities_reuse_permutation_invariant_regions() {
    for size in [8, 128] {
        let fields = SymmetricEnums::domain_fields(DomainField::root());
        let mut spec = enum_family(size);
        spec.add(fields.a.equal(&fields.c), &[]);
        spec.add(fields.c.equal(&fields.b), &[]);
        compilation::reset_enum_pair_context_checks();
        compilation::take_compilation_work();
        let compiled = spec.compile().unwrap();
        let work = compilation::take_compilation_work();
        assert!(work.signatures <= size * 4 + 128, "{work:?}");
        eprintln!(
            "enum values={size}; {work:?}; feasibility solves={}",
            compilation::enum_pair_solves()
        );
        assert_eq!(compiled.coverage_catalog().pairwise_total, 3 * size);
        assert!(
            compilation::enum_pair_solves() <= 16,
            "{}",
            compilation::enum_pair_solves()
        );
        for tag in [0, size - 1] {
            assert!(compiled
                .classify(&SymmetricEnums {
                    a: CatalogChoice(tag),
                    b: CatalogChoice(tag),
                    c: CatalogChoice(tag)
                })
                .unwrap()
                .violated
                .is_empty());
        }
        assert!(!compiled
            .classify(&SymmetricEnums {
                a: CatalogChoice(0),
                b: CatalogChoice(1),
                c: CatalogChoice(0)
            })
            .unwrap()
            .violated
            .is_empty());
    }
}

#[test]
fn conditional_enum_relations_keep_constant_distinctions_during_symmetry_reduction() {
    for size in [8, 128] {
        let fields = SymmetricEnums::domain_fields(DomainField::root());
        let mut spec = enum_family(size);
        spec.add(
            fields.b.equal(&fields.c),
            &[fields.a.equal(CatalogChoice(0))],
        );
        compilation::reset_enum_pair_context_checks();
        compilation::take_compilation_work();
        let compiled = spec.compile().unwrap();
        let work = compilation::take_compilation_work();
        assert!(work.signatures <= size * 4 + 128, "{work:?}");
        eprintln!(
            "enum values={size}; {work:?}; feasibility solves={}",
            compilation::enum_pair_solves()
        );
        assert_eq!(compiled.coverage_catalog().pairwise_total, 3 * size * size);
        assert!(
            compilation::enum_pair_solves() <= 40,
            "{}",
            compilation::enum_pair_solves()
        );
        for tag in [0, 1, size - 1] {
            let coordinate = compiled
                .classify(&SymmetricEnums {
                    a: CatalogChoice(tag),
                    b: CatalogChoice(0),
                    c: CatalogChoice(1),
                })
                .unwrap();
            assert_eq!(coordinate.violated.is_empty(), tag != 0);
        }
    }
}

#[derive(Clone, Debug, DomainInput)]
struct MixedEnumComponents {
    a: CatalogChoice,
    b: CatalogChoice,
    c: CatalogChoice,
    other: ConstrainedVariant,
}

#[test]
fn unrelated_payload_structure_does_not_expand_unit_enum_feasibility_work() {
    let spec = domain!(MixedEnumComponents {
        a.selected_enum_values((0..32).map(CatalogChoice)),
        b.selected_enum_values((0..32).map(CatalogChoice)),
        c.selected_enum_values((0..32).map(CatalogChoice)),
        other.selected_enum_values([ConstrainedVariant::Possible]),
        a.equal(c), c.equal(b),
    });
    compilation::reset_enum_pair_context_checks();
    let compiled = spec.compile().unwrap();
    assert_eq!(compiled.coverage_catalog().pairwise_total, 6 * 32);
    assert!(
        compilation::enum_pair_solves() <= 64,
        "{}",
        compilation::enum_pair_solves()
    );
}

#[test]
fn ordered_enum_relations_preserve_distinct_order_boundaries() {
    let fields = SymmetricEnums::domain_fields(DomainField::root());
    let mut spec = enum_family(8);
    spec.add(fields.a.less_than(&fields.b), &[]);
    let compiled = spec.compile().unwrap();
    assert_eq!(compiled.coverage_catalog().pairwise_total, 28 + 2 * 7 * 8);
    assert_eq!(compiled.coverage_catalog().enum_values.len(), 22);
    for (a, b) in [(0, 1), (1, 0), (6, 7), (7, 7)] {
        assert_eq!(
            compiled
                .classify(&SymmetricEnums {
                    a: CatalogChoice(a),
                    b: CatalogChoice(b),
                    c: CatalogChoice(0)
                })
                .unwrap()
                .violated
                .is_empty(),
            a < b
        );
    }
}

#[test]
fn conditional_pair_feasibility_preserves_fixed_source_activation() {
    for size in [8, 128] {
        let spec = domain!(SymmetricEnums {
            a.selected_enum_values([CatalogChoice(0)]),
            b.selected_enum_values((0..size).map(CatalogChoice)),
            c.selected_enum_values((0..size).map(CatalogChoice)),
            when(a.equal(CatalogChoice(0))) { b.equal(c) },
        });
        compilation::reset_enum_pair_context_checks();
        let compiled = spec.compile().unwrap();
        assert_eq!(compiled.coverage_catalog().pairwise_total, 3 * size);
        assert_eq!(compiled.coverage_catalog().enum_values.len(), 1 + 2 * size);
        assert!(compilation::enum_pair_solves() <= 16);
    }
}

#[test]
fn enum_source_membership_partitions_feasibility_symmetries() {
    let spec = domain!(SymmetricEnums {
        a.selected_enum_values((0..8).map(CatalogChoice)),
        b.selected_enum_values((0..4).map(CatalogChoice)),
        c.selected_enum_values((0..8).map(CatalogChoice)),
        a.equal(b),
    });
    let compiled = spec.compile().unwrap();
    assert_eq!(compiled.coverage_catalog().pairwise_total, 4 + 2 * 4 * 8);
    assert_eq!(compiled.coverage_catalog().enum_values.len(), 16);
}

#[test]
fn large_f32_selections_prepare_normalized_coordinates_once() {
    compilation::take_compilation_work();
    let compiled = domain!(f32 { selected_values((0..4100).map(|n| n as f32)) })
        .compile()
        .unwrap();
    let work = compilation::take_compilation_work();
    assert!(work.anchor_values <= 4100 * 16, "{work:?}");
    assert_eq!(compiled.coverage_catalog().positive_anchors.len(), 4100);
    for value in [0.0f32, 1.0, 4099.0] {
        let coordinate = compiled.classify(&value).unwrap();
        assert!(coordinate.violated.is_empty());
        assert_eq!(
            coordinate.entries[0].slot,
            CaseDomainCoordinateSlot::Anchor(value as usize)
        );
    }
}

#[derive(Clone, Debug, DomainInput)]
struct ReversedLengthConditional {
    items: Vec<i32>,
    n: usize,
}

#[test]
fn conditional_length_field_comparisons_group_reachable_regions() {
    let domain = domain!(WideLengthConditional {
        n.selected_values(0..30),
        items { length().selected_values(0..200) },
        when(items.length().less_equal(n)) { items { each(fixed_value(0)) } },
        when(items.length().greater_than(n)) { items { each(fixed_value(1)) } },
    })
    .compile()
    .unwrap();
    for input in [
        WideLengthConditional {
            n: 15,
            items: vec![0; 15],
        },
        WideLengthConditional {
            n: 15,
            items: vec![1; 16],
        },
    ] {
        assert!(domain.classify(&input).unwrap().violated.is_empty());
    }
    assert!(domain!(WideLengthConditional {
        n.selected_values(0..30),
        items { length().selected_values(0..200) },
        when(items.length().less_equal(n)) { items { each(fixed_value(0)) } },
    })
    .compile()
    .is_err());
    let reversed = domain!(ReversedLengthConditional {
        n.selected_values(0..30),
        items { length().selected_values(0..200) },
        when(items.length().less_equal(n)) { items { each(fixed_value(0)) } },
        when(items.length().greater_than(n)) { items { each(fixed_value(1)) } },
    })
    .compile()
    .unwrap();
    assert!(reversed
        .classify(&ReversedLengthConditional {
            n: 15,
            items: vec![1; 16]
        })
        .unwrap()
        .violated
        .is_empty());
}

#[derive(Clone, Debug, DomainInput)]
struct FieldConditional {
    a: i32,
    b: i32,
    c: i32,
}
#[test]
fn conditional_field_relations_cover_reachable_equality_regions() {
    let incomplete = domain!(FieldConditional {
        a.in_range(0..=100), b.in_range(1..=2),
        when(a.not_equal(b)) { c.fixed_value(0) },
    });
    assert!(incomplete.compile().is_err());
    let complete = domain!(FieldConditional {
        a.in_range(0..=100), b.in_range(1..=2),
        when(a.not_equal(b)) { c.fixed_value(0) },
        when(a.equal(b)) { c.fixed_value(1) },
    })
    .compile()
    .unwrap();
    assert!(complete
        .classify(&FieldConditional { a: 1, b: 1, c: 1 })
        .unwrap()
        .violated
        .is_empty());
    assert!(complete
        .classify(&FieldConditional { a: 0, b: 1, c: 0 })
        .unwrap()
        .violated
        .is_empty());
}

#[derive(Clone, Debug, PartialEq, DomainInput)]
enum CoverageBit {
    A,
    B,
}
#[derive(Clone, Debug, DomainInput)]
struct RelationPair {
    a: CoverageBit,
    b: CoverageBit,
}
#[derive(Clone, Debug, DomainInput)]
struct UnreachableEnumBranch {
    gate: CoverageBit,
    left: CoverageBit,
    right: CoverageBit,
}
#[test]
fn relation_unreachable_enum_sources_do_not_create_coverage_obligations() {
    let mut runner =
        certification("unreachable enum branch").executor(|v: &UnreachableEnumBranch| {
            if v.gate == CoverageBit::A {
                Ok::<_, ()>(())
            } else {
                Err(())
            }
        });
    runner.domain(domain!(UnreachableEnumBranch {
        gate.all_enum_values(), gate.equal(CoverageBit::A),
        when(gate.equal(CoverageBit::A)) { left.all_enum_values() },
        when(gate.equal(CoverageBit::B)) { left.all_enum_values() },
        right.all_enum_values(),
    }));
    runner.cases_budget(40).require_pairwise_coverage();
    let report = runner.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage().enum_values, (5, 5));
    assert_eq!(report.coverage().pairwise_combinations, (8, 8));
}

#[derive(Clone, Debug, DomainInput)]
enum ConstrainedVariant {
    Impossible { n: i32 },
    Possible,
}
#[derive(Clone, Debug, DomainInput)]
struct VariantRelationPair {
    gate: ConstrainedVariant,
    side: CoverageBit,
}
#[test]
fn enum_payload_relations_restrict_parent_variant_pair_feasibility() {
    let mut runner =
        certification("payload relation feasibility").executor(|v: &VariantRelationPair| {
            match v.gate {
                ConstrainedVariant::Possible => Ok::<_, ()>(()),
                ConstrainedVariant::Impossible { .. } => Err(()),
            }
        });
    runner.domain(domain!(VariantRelationPair {
        gate { all_enum_values(), ConstrainedVariant::Impossible { n.fixed_value(0), n.greater_than(0) } },
        side.all_enum_values(),
    }));
    runner.cases_budget(20).require_pairwise_coverage();
    let report = runner.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage().enum_values, (3, 3));
    assert_eq!(report.coverage().pairwise_combinations, (2, 2));
}

#[test]
fn ordinary_relation_limits_required_enum_pairs() {
    let mut runner = certification("feasible pairs").executor(|v: &RelationPair| {
        if v.a == v.b {
            Ok::<_, ()>(())
        } else {
            Err(())
        }
    });
    runner.domain(domain!(RelationPair { a.all_enum_values(), b.all_enum_values(), a.equal(b) }));
    runner.cases_budget(30).require_pairwise_coverage();
    let report = runner.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage().pairwise_combinations, (2, 2));
}

#[test]
fn streamed_negative_distances_match_ordered_pair_extrema_with_duplicates_and_overflow() {
    use super::domain::{DomainOperand, DomainRelation, DomainValue, RelationKind};
    let sets = [
        vec![
            DomainValue::Int(-9),
            DomainValue::Int(0),
            DomainValue::Int(0),
            DomainValue::Int(23),
        ],
        vec![
            DomainValue::UInt(0),
            DomainValue::UInt(1),
            DomainValue::UInt(u128::MAX),
        ],
        vec![
            DomainValue::Real(-f64::MAX),
            DomainValue::Real(-0.0),
            DomainValue::Real(0.0),
            DomainValue::Real(f64::MAX),
        ],
        vec![DomainValue::Bool(false), DomainValue::Bool(true)],
        vec![DomainValue::Text("a".into()), DomainValue::Text("z".into())],
    ];
    let distance = |a: &DomainValue, b: &DomainValue| match (a, b) {
        (DomainValue::Int(a), DomainValue::Int(b)) => a.abs_diff(*b) as f64,
        (DomainValue::UInt(a), DomainValue::UInt(b)) => a.abs_diff(*b) as f64,
        (DomainValue::Real(a), DomainValue::Real(b)) => (a - b).abs(),
        _ => 0.,
    };
    for values in sets {
        for kind in [
            RelationKind::LessThan,
            RelationKind::LessEqual,
            RelationKind::GreaterThan,
            RelationKind::GreaterEqual,
            RelationKind::Equal,
            RelationKind::NotEqual,
        ] {
            let relation = DomainRelation {
                left: DomainOperand::Value(values[0].clone()),
                right: DomainOperand::Value(values[0].clone()),
                kind,
            };
            let pairs = relation.invalid_pairs(&values, &values);
            let mut reference = Vec::new();
            for a in &values {
                for b in &values {
                    let comparison = DomainRelation {
                        left: DomainOperand::Value(a.clone()),
                        right: DomainOperand::Value(b.clone()),
                        kind,
                    };
                    if comparison.evaluate(&[]) == Some(false)
                        && !reference.contains(&(a.clone(), b.clone()))
                    {
                        reference.push((a.clone(), b.clone()));
                    }
                }
            }
            reference.sort_by(|(a, b), (c, d)| {
                distance(a, b)
                    .total_cmp(&distance(c, d))
                    .then_with(|| (a.key(), b.key()).cmp(&(c.key(), d.key())))
            });
            assert_eq!(format!("{pairs:?}"), format!("{reference:?}"));
            for (a, b) in &pairs {
                let low = distance(&pairs.first().unwrap().0, &pairs.first().unwrap().1);
                let high = distance(&pairs.last().unwrap().0, &pairs.last().unwrap().1);
                let value = distance(a, b);
                let expected = if high <= low || value <= low + (high - low) / 3. {
                    DomainNegativeDistance::Near
                } else if value < low + 2. * ((high - low) / 3.) {
                    DomainNegativeDistance::Mid
                } else {
                    DomainNegativeDistance::Far
                };
                assert_eq!(relation.negative_distance(a, b, &values, &values), expected);
            }
            assert_eq!(
                relation.negative_distance(&values[0], &values[0], &[], &values),
                DomainNegativeDistance::Near
            );
        }
    }
}

#[derive(Clone, Debug, DomainInput)]
struct SolverNumbers {
    a: i64,
    b: i64,
    c: i64,
    d: i64,
}
#[test]
fn relation_pair_witnesses_preserve_coordinates_and_exhaustive_feasibility() {
    for (topology, relations, end) in [
        (0, 0, 1000),
        (0, 1, 1000),
        (0, 3, 1000),
        (0, 6, 8),
        (1, 3, 1000),
    ] {
        let f = DomainField::<SolverNumbers>::root().fields();
        let mut domain = domain!(SolverNumbers { a.in_range(0..=end), b.in_range(0..=end), c.in_range(0..=end), d.in_range(0..=end) });
        let rules = if topology == 1 {
            vec![
                f.a.less_equal(&f.b),
                f.a.less_equal(&f.c),
                f.a.less_equal(&f.d),
            ]
        } else {
            vec![
                f.a.less_equal(&f.b),
                f.b.less_equal(&f.c),
                f.c.less_equal(&f.d),
                f.a.less_equal(&f.c),
                f.a.less_equal(&f.d),
                f.b.less_equal(&f.d),
            ]
        };
        for rule in rules.into_iter().take(relations) {
            domain.add(rule, &[]);
        }
        let compiled = domain.compile().unwrap();
        let mut signatures = Vec::new();
        for seed in [1, 719, 184729361] {
            let mut occupied = BTreeSet::new();
            let mut cursor = 0;
            let mut pairs = BTreeSet::new();
            let mut witnesses = Vec::new();
            while let Some((input, coordinate)) =
                compiled.next(2, &mut cursor, seed, &occupied).unwrap()
            {
                let actual = compiled.classify(&input).unwrap();
                assert_eq!(actual.violated, coordinate.violated);
                assert_eq!(coordinate.violated.len(), 2);
                pairs.insert(coordinate.violated.clone());
                occupied.insert(coordinate_key(&coordinate));
                witnesses.push((
                    [input.a, input.b, input.c, input.d],
                    coordinate_key(&coordinate),
                ));
            }
            if end == 8 {
                let mut oracle = BTreeSet::new();
                for a in -1..=9 {
                    for b in -1..=9 {
                        for c in -1..=9 {
                            for d in -1..=9 {
                                let flags = [
                                    !(0..=end).contains(&a),
                                    !(0..=end).contains(&b),
                                    !(0..=end).contains(&c),
                                    !(0..=end).contains(&d),
                                    a > b,
                                    b > c,
                                    c > d,
                                    a > c,
                                    a > d,
                                    b > d,
                                ];
                                let violated: Vec<_> = flags
                                    .into_iter()
                                    .enumerate()
                                    .filter_map(|(id, bad)| bad.then_some(id))
                                    .collect();
                                if violated.len() == 2 {
                                    oracle.insert(violated);
                                }
                            }
                        }
                    }
                }
                assert_eq!(pairs, oracle);
            }
            let portable = format!("{witnesses:?}")
                .bytes()
                .fold(0xcbf29ce484222325u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                });
            signatures.push(portable);
        }
        let expected = match (topology, relations, end) {
            (0, 0, 1000) => 3947585941243040917,
            (0, 1, 1000) => 15794665842306307549,
            (0, 3, 1000) => 17098276074675806099,
            (0, 6, 8) => 10467569229425296998,
            (1, 3, 1000) => 9536504339960619074,
            _ => unreachable!(),
        };
        assert_eq!(signatures, [expected; 3]);
    }
}
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};
#[derive(DomainInput, Debug, Clone)]
struct Pair {
    left: i32,
    right: i32,
}
#[test]
fn all_scalar_relations_use_their_own_exact_negative_semantics() {
    let domains = [
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.less_than(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.less_equal(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.greater_than(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.greater_equal(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.equal(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.not_equal(right)}),
    ];
    for (kind, domain) in domains.into_iter().enumerate() {
        let d = domain.compile().unwrap();
        let mut cursor = 0;
        let mut occupied = BTreeSet::new();
        let mut relation_negative = false;
        while let Some((i, c)) = d.next(1, &mut cursor, 1, &occupied).unwrap() {
            if c.violated == [2] {
                let valid = match kind {
                    0 => i.left < i.right,
                    1 => i.left <= i.right,
                    2 => i.left > i.right,
                    3 => i.left >= i.right,
                    4 => i.left == i.right,
                    _ => i.left != i.right,
                };
                assert!(!valid);
                assert!((0..=9).contains(&i.left));
                assert!((0..=9).contains(&i.right));
                relation_negative = true;
            }
            occupied.insert(coordinate_key(&c));
        }
        assert!(relation_negative);
    }
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
enum Choice {
    A,
    B,
    C,
}
#[derive(DomainInput, Debug, Clone)]
struct Choices {
    a: Choice,
    b: Choice,
    c: Choice,
}
#[test]
fn enums_cover_individual_then_pairwise_then_higher_order_without_cartesian_eager_cases() {
    let d = domain!(Choices{a.all_enum_values(),b.all_enum_values(),c.all_enum_values()})
        .compile()
        .unwrap();
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut sectors = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut singles = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut count = 0;
    while let Some((_, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
        for (i, e) in c.entries.iter().enumerate() {
            sectors[i].insert(e.sector.clone());
            singles[i].insert(e.slot.clone());
        }
        occupied.insert(coordinate_key(&c));
        count += 1;
        if count == 7 {
            assert!(sectors.iter().all(|s| s.len() == 1));
            assert!(singles.iter().all(|s| s.len() == 3));
        }
    }
    assert!(count >= 19);
    assert!(count <= 27);
}
#[derive(DomainInput, Debug)]
struct Discrete {
    choice: Choice,
    retries: u8,
    text: String,
}
#[test]
fn excluded_enum_and_selected_scalar_cases_use_the_real_rejection_path() {
    let seen = RefCell::new(vec![]);
    let mut c = certification("discrete rejection").executor(|i: &Discrete| {
        seen.borrow_mut().push((i.choice.clone(), i.retries));
        if matches!(i.choice, Choice::A | Choice::C)
            && [0, 3, 5].contains(&i.retries)
            && i.text == "input"
        {
            Ok(())
        } else {
            Err(())
        }
    });
    c.domain(domain!(Discrete{choice.selected_enum_values([Choice::A,Choice::C]),retries.selected_values([0,3,5]),text.fixed_value("input".into())}));
    c.cases_budget(30);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert!(seen.borrow().iter().any(|(choice, _)| *choice == Choice::B));
}
#[derive(DomainInput, Debug)]
struct Single {
    value: i32,
}
#[test]
fn disconnected_ranges_keep_gap_and_branch_sectors_separate() {
    let d = domain!(Single{value.one_of_ranges([1..=3,10..=12])})
        .compile()
        .unwrap();
    let gap = d.classify(&Single { value: 7 }).unwrap();
    assert_eq!(gap.violated, [0]);
    assert_eq!(gap.entries[0].sector.region, DomainRegion::Gap(0));
    assert_ne!(
        d.classify(&Single { value: 1 }).unwrap().entries[0].sector,
        d.classify(&Single { value: 10 }).unwrap().entries[0].sector
    );
}
#[test]
fn data_source_prefill_skips_an_anchor_even_when_other_dimensions_differ() {
    let seen = RefCell::new(vec![]);
    let mut c = certification("semantic prefill").executor(|i: &Pair| {
        seen.borrow_mut().push((i.left, i.right));
        if (1..=9).contains(&i.left) && (1..=9).contains(&i.right) {
            Ok(())
        } else {
            Err(())
        }
    });
    c.data_source([Pair { left: 1, right: 9 }]);
    c.domain(domain!(Pair{left.in_range(1..=9),right.in_range(1..=9)}));
    c.cases_budget(2);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert_eq!(seen.borrow()[1], (1, 1));
    let d = domain!(Pair{left.in_range(1..=9),right.in_range(1..=9)})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    occupied.insert(coordinate_key(
        &d.classify(&Pair { left: 1, right: 9 }).unwrap(),
    ));
    let mut cursor = 0;
    d.next(0, &mut cursor, 1, &occupied).unwrap().unwrap();
    assert!(cursor > 1);
}
#[test]
fn independent_checks_continue_after_a_failure_and_eager_checks_register() {
    let checked = Cell::new(0);
    let mut c = certification("independent checks").executor(|_: &Single| Ok::<_, ()>(1));
    c.data_source([Single { value: 1 }]);
    let case = c.case();
    c.check(fact!(*case.result()).expect(2));
    c.check(
        fact!({
            checked.set(checked.get() + 1);
            *case.result()
        })
        .expect(1),
    );
    c.check(fact!(4).expect(4));
    let r = c.report();
    assert!(!r.is_pass());
    assert_eq!(checked.get(), 1);
}
#[test]
fn lower_categories_are_explored_and_unused_failure_capacity_is_redistributed() {
    let mut c = certification("only positive fails").executor(|i: &Pair| {
        if (0..=9).contains(&i.left) && (0..=9).contains(&i.right) {
            Ok(0)
        } else {
            Err(())
        }
    });
    c.domain(domain!(Pair{left.in_range(0..=9),right.in_range(0..=9)}));
    c.cases_budget(200);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1));
    let r = c.report();
    assert!(r.stopped, "{r}");
    assert_eq!(r.failures.len(), 5);
    assert!(r.categories[1] > 0 && r.categories[2] > 0);
    assert!(r
        .failures
        .iter()
        .all(|f| f.category == CaseCategory::Positive));
}
#[test]
fn multi_negative_failures_are_sought_when_single_negatives_reject_correctly() {
    let mut c = certification("double failures").executor(|i: &Pair| {
        let invalid =
            usize::from(!(0..=9).contains(&i.left)) + usize::from(!(0..=9).contains(&i.right));
        if invalid == 1 {
            Err(())
        } else {
            Ok(0)
        }
    });
    c.domain(domain!(Pair{left.in_range(0..=9),right.in_range(0..=9)}));
    c.cases_budget(100);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1));
    let r = c.report();
    assert!(
        r.failures
            .iter()
            .any(|f| f.category == CaseCategory::MultiNegative),
        "{r}"
    );
    assert!(r
        .failures
        .iter()
        .all(|f| f.category != CaseCategory::SingleNegative));
}
#[derive(DomainInput, Debug)]
struct Conditional {
    mode: Choice,
    count: usize,
}
#[test]
fn missing_and_overlapping_conditional_branches_are_preflight_errors() {
    assert!(domain!(Conditional{mode.all_enum_values(),when(mode.equal(Choice::A)){count.fixed_value(1)}}).compile().is_err());
    assert!(domain!(Conditional{mode.all_enum_values(),count.fixed_value(1),when(mode.equal(Choice::A)){count.fixed_value(2)}}).compile().is_err());
}
#[test]
fn real_inputs_outside_domain_are_classified_and_rejected_by_the_same_executor() {
    let calls = Cell::new(0);
    let mut c = certification("real negative").executor(|i: &Single| {
        calls.set(calls.get() + 1);
        if (1..=9).contains(&i.value) {
            Ok(())
        } else {
            Err(())
        }
    });
    c.data_source([Single { value: 0 }]);
    c.domain(domain!(Single{value.in_range(1..=9)}));
    c.cases_budget(5);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert!(r.categories[1] > 0);
    assert_eq!(r.executed[0], 1);
    assert_eq!(calls.get(), r.cases_executed());
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
enum Octet {
    V0,
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
}
#[derive(DomainInput, Debug, Clone)]
struct WideDomain {
    e0: Octet,
    e1: Octet,
    e2: Octet,
    e3: Octet,
    e4: Octet,
    e5: Octet,
    e6: Octet,
    e7: Octet,
    n0: i32,
    n1: i32,
    n2: i32,
    n3: i32,
    n4: i32,
    n5: i32,
}
#[test]
fn high_cardinality_domain_compiles_lazily_and_bounds_solver_work_by_cases_budget() {
    let spec = domain!(WideDomain {
        e0.all_enum_values(),
        e1.all_enum_values(),
        e2.all_enum_values(),
        e3.all_enum_values(),
        e4.all_enum_values(),
        e5.all_enum_values(),
        e6.all_enum_values(),
        e7.all_enum_values(),
        n0.in_range(1..=100),
        n1.in_range(1..=100),
        n2.in_range(1..=100),
        n3.in_range(1..=100),
        n4.in_range(1..=100),
        n5.in_range(1..=100)
    });
    compilation::reset_solve_calls();
    compilation::reset_enum_pair_context_checks();
    let compiled = spec.clone().compile().unwrap();
    let compile_solves = compilation::solve_calls();
    let pair_context_checks = compilation::enum_pair_context_checks();
    assert!(compile_solves <= 2, "{compile_solves}");
    assert!(pair_context_checks <= 32, "{pair_context_checks}");
    assert_eq!(compiled.coverage_catalog().enum_values.len(), 64);
    assert_eq!(compiled.coverage_catalog().pairwise_total, 28 * 64);

    let mut c = certification("lazy wide domain").executor(|i: &WideDomain| {
        if (1..=100).contains(&i.n0)
            && (1..=100).contains(&i.n1)
            && (1..=100).contains(&i.n2)
            && (1..=100).contains(&i.n3)
            && (1..=100).contains(&i.n4)
            && (1..=100).contains(&i.n5)
        {
            Ok(1usize)
        } else {
            Err(())
        }
    });
    c.domain(spec);
    c.cases_budget(5);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    compilation::reset_solve_calls();
    let r = c.report();
    let run_solves = compilation::solve_calls();
    assert!(r.is_pass(), "{r}");
    assert_eq!(r.cases_executed(), 5);
    assert!(run_solves <= 25, "{run_solves}");
}
#[derive(DomainInput, Debug, Clone)]
struct FloatSingle {
    value: f64,
}
#[test]
fn stratified_range_exploration_is_breadth_first_across_sectors_and_seeded_random_in_depth() {
    let d = domain!(Single {
        value.in_range(1..=10_000)
    })
    .compile()
    .unwrap();
    let sample_sequence = |seed: u64, count: usize| {
        let mut cursor = 0;
        let mut occupied = BTreeSet::new();
        let mut out = vec![];
        while out.len() < count {
            let (input, coord) = d.next(0, &mut cursor, seed, &occupied).unwrap().unwrap();
            occupied.insert(coordinate_key(&coord));
            out.push((input.value, coord.entries[0].clone()));
        }
        out
    };
    let anchor_count = d.coverage_catalog().positive_anchors.len();
    let seq_a = sample_sequence(7, anchor_count + 30);
    let seq_a_repeat = sample_sequence(7, anchor_count + 30);
    let seq_b = sample_sequence(99, anchor_count + 30);
    assert_eq!(
        seq_a.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
        seq_a_repeat.iter().map(|(v, _)| *v).collect::<Vec<_>>()
    );
    for (idx, (_, entry)) in seq_a[..anchor_count].iter().enumerate() {
        assert_eq!(entry.slot, CaseDomainCoordinateSlot::Anchor(idx));
    }
    assert_eq!(
        seq_a[..anchor_count]
            .iter()
            .map(|(v, _)| *v)
            .collect::<Vec<_>>(),
        seq_b[..anchor_count]
            .iter()
            .map(|(v, _)| *v)
            .collect::<Vec<_>>()
    );
    for chunk in seq_a[anchor_count..].chunks_exact(3) {
        let terciles: Vec<_> = chunk
            .iter()
            .map(|(_, e)| match e.sector.region {
                DomainRegion::Range { tercile, .. } => tercile,
                ref other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            terciles,
            vec![DomainTercile::Low, DomainTercile::Mid, DomainTercile::High]
        );
    }
    let low_strata_a: Vec<u64> = seq_a[anchor_count..]
        .iter()
        .step_by(3)
        .map(|(_, e)| match e.slot {
            CaseDomainCoordinateSlot::Stratum(bin) => bin % 64,
            ref other => panic!("{other:?}"),
        })
        .collect();
    let low_strata_b: Vec<u64> = seq_b[anchor_count..]
        .iter()
        .step_by(3)
        .map(|(_, e)| match e.slot {
            CaseDomainCoordinateSlot::Stratum(bin) => bin % 64,
            ref other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        low_strata_a.iter().copied().collect::<BTreeSet<_>>().len(),
        low_strata_a.len()
    );
    assert_ne!(
        low_strata_a,
        (0..low_strata_a.len() as u64).collect::<Vec<_>>()
    );
    assert_ne!(low_strata_a, low_strata_b);

    let full_seq = sample_sequence(7, anchor_count + 64 * 3);
    let all_low: BTreeSet<u64> = full_seq[anchor_count..]
        .iter()
        .step_by(3)
        .map(|(_, e)| match e.slot {
            CaseDomainCoordinateSlot::Stratum(bin) => bin % 64,
            ref other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(all_low.len(), 64);

    let disconnected = domain!(Single {
        value.one_of_ranges([0..=300, 1000..=1300])
    })
    .compile()
    .unwrap();
    let disc_anchors = disconnected.coverage_catalog().positive_anchors.len();
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut multi = vec![];
    while multi.len() < disc_anchors + 6 {
        let (_, coord) = disconnected
            .next(0, &mut cursor, 11, &occupied)
            .unwrap()
            .unwrap();
        occupied.insert(coordinate_key(&coord));
        multi.push(coord.entries[0].clone());
    }
    let branch_regions: Vec<_> = multi[disc_anchors..]
        .iter()
        .map(|e| e.sector.region.clone())
        .collect();
    assert_eq!(
        branch_regions,
        vec![
            DomainRegion::Range {
                branch: 0,
                tercile: DomainTercile::Low
            },
            DomainRegion::Range {
                branch: 0,
                tercile: DomainTercile::Mid
            },
            DomainRegion::Range {
                branch: 0,
                tercile: DomainTercile::High
            },
            DomainRegion::Range {
                branch: 1,
                tercile: DomainTercile::Low
            },
            DomainRegion::Range {
                branch: 1,
                tercile: DomainTercile::Mid
            },
            DomainRegion::Range {
                branch: 1,
                tercile: DomainTercile::High
            },
        ]
    );

    let df = domain!(FloatSingle {
        value.in_range(0.0..=300.0)
    })
    .compile()
    .unwrap();
    let mut f_cursor = 0;
    let mut f_occupied = BTreeSet::new();
    let mut f_entries = vec![];
    while f_entries.len() < 7 + 12 {
        let (_, coord) = df.next(0, &mut f_cursor, 23, &f_occupied).unwrap().unwrap();
        f_occupied.insert(coordinate_key(&coord));
        f_entries.push(coord.entries[0].clone());
    }
    for chunk in f_entries[7..].chunks_exact(3) {
        let terciles: Vec<_> = chunk
            .iter()
            .map(|e| match e.sector.region {
                DomainRegion::Range { tercile, .. } => tercile,
                ref other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            terciles,
            vec![DomainTercile::Low, DomainTercile::Mid, DomainTercile::High]
        );
    }
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct ChoiceVec {
    items: Vec<Choice>,
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct IntVec {
    items: Vec<i32>,
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct ConditionalItem {
    mode: Choice,
    value: i32,
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct ConditionalBatch {
    items: Vec<ConditionalItem>,
}
#[test]
fn generated_collections_produce_heterogeneous_compositions_and_preserve_negative_invariants() {
    let d_enum = domain!(ChoiceVec {
        items {
            length().in_range(2..=4),
            each(all_enum_values())
        }
    })
    .compile()
    .unwrap();
    let sample_enum_vec = |seed: u64| {
        let mut cursor = 0;
        let mut occupied = BTreeSet::new();
        let mut cases = vec![];
        while let Some((input, coord)) = d_enum.next(0, &mut cursor, seed, &occupied).unwrap() {
            assert_eq!(
                coordinate_key(&d_enum.classify(&input).unwrap()),
                coordinate_key(&coord)
            );
            occupied.insert(coordinate_key(&coord));
            cases.push((input.items, coord));
            if cases.len() >= 10 {
                break;
            }
        }
        cases
    };
    let enum_cases = sample_enum_vec(42);
    let enum_cases_repeat = sample_enum_vec(42);
    assert_eq!(
        enum_cases
            .iter()
            .map(|(v, _)| v.clone())
            .collect::<Vec<_>>(),
        enum_cases_repeat
            .iter()
            .map(|(v, _)| v.clone())
            .collect::<Vec<_>>()
    );
    assert!(enum_cases
        .iter()
        .any(|(items, _)| items.len() >= 2 && items.iter().all(|x| x == &items[0])));
    assert!(enum_cases
        .iter()
        .any(|(items, _)| items.windows(2).any(|w| w[0] != w[1])));
    assert!(enum_cases.iter().any(|(_, coord)| {
        let variant_slots: BTreeSet<_> = coord
            .entries
            .iter()
            .filter_map(|e| match e.slot {
                CaseDomainCoordinateSlot::EnumVariant(tag) => Some(tag),
                _ => None,
            })
            .collect();
        variant_slots.len() == 3
    }));

    let prefilled_coord = d_enum
        .classify(&ChoiceVec {
            items: vec![Choice::A, Choice::B, Choice::A],
        })
        .unwrap();
    let mut occupied_prefill = BTreeSet::from([coordinate_key(&prefilled_coord)]);
    let mut cursor_prefill = 0;
    let mut saw_ab_only_mix = false;
    while let Some((input, coord)) = d_enum
        .next(0, &mut cursor_prefill, 42, &occupied_prefill)
        .unwrap()
    {
        occupied_prefill.insert(coordinate_key(&coord));
        let distinct: BTreeSet<_> = input
            .items
            .iter()
            .map(|c| match c {
                Choice::A => 0usize,
                Choice::B => 1usize,
                Choice::C => 2usize,
            })
            .collect();
        if distinct == BTreeSet::from([0usize, 1usize]) {
            saw_ab_only_mix = true;
        }
    }
    assert!(!saw_ab_only_mix);

    let d_int = domain!(IntVec {
        items {
            length().in_range(3..=5),
            each(in_range(1..=30))
        }
    })
    .compile()
    .unwrap();
    let mut cursor_int = 0;
    let mut occupied_int = BTreeSet::new();
    let mut int_cases = vec![];
    while let Some((input, coord)) = d_int.next(0, &mut cursor_int, 17, &occupied_int).unwrap() {
        occupied_int.insert(coordinate_key(&coord));
        int_cases.push((input.items, coord));
        if int_cases.len() >= 20 {
            break;
        }
    }
    assert!(int_cases
        .iter()
        .any(|(items, _)| items.windows(2).any(|w| w[0] != w[1])));
    assert!(int_cases.iter().any(|(_, coord)| {
        let terciles: BTreeSet<_> = coord
            .entries
            .iter()
            .filter_map(|e| match e.sector.region {
                DomainRegion::Range { tercile, .. } if e.sector.constraint == 1 => Some(tercile),
                _ => None,
            })
            .collect();
        terciles.len() == 3
    }));
    let mut neg_cursor = 0;
    let mut neg_occupied = BTreeSet::new();
    while let Some((_, coord)) = d_int.next(1, &mut neg_cursor, 17, &neg_occupied).unwrap() {
        assert_eq!(coord.violated.len(), 1);
        neg_occupied.insert(coordinate_key(&coord));
    }
    assert!(!neg_occupied.is_empty());

    let d_cond = domain!(ConditionalBatch {
        items {
            length().in_range(2..=4),
            each {
                mode.all_enum_values(),
                when(mode.equal(Choice::A)) { value.in_range(1..=3) },
                when(mode.equal(Choice::B)) { value.in_range(10..=12) },
                when(mode.equal(Choice::C)) { value.in_range(20..=22) }
            }
        }
    })
    .compile()
    .unwrap();
    let mut cond_cursor = 0;
    let mut cond_occupied = BTreeSet::new();
    let mut saw_mixed_modes = false;
    while let Some((batch, coord)) = d_cond.next(0, &mut cond_cursor, 5, &cond_occupied).unwrap() {
        cond_occupied.insert(coordinate_key(&coord));
        for item in &batch.items {
            match item.mode {
                Choice::A => assert!((1..=3).contains(&item.value)),
                Choice::B => assert!((10..=12).contains(&item.value)),
                Choice::C => assert!((20..=22).contains(&item.value)),
            }
        }
        if batch.items.windows(2).any(|w| w[0].mode != w[1].mode) {
            saw_mixed_modes = true;
        }
        if cond_occupied.len() >= 18 {
            break;
        }
    }
    assert!(saw_mixed_modes);

    for max_len in [0i32, 1i32] {
        let d_short = domain!(ChoiceVec {
            items {
                length().in_range(0..=max_len as usize),
                each(all_enum_values())
            }
        })
        .compile()
        .unwrap();
        let mut short_cursor = 0;
        let short_occupied = BTreeSet::new();
        let (first, coord) = d_short
            .next(0, &mut short_cursor, 1, &short_occupied)
            .unwrap()
            .unwrap();
        assert!(first.items.len() <= max_len as usize);
        assert!(coord.violated.is_empty());
    }
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
enum QuadChoice {
    A,
    B,
    C,
    D,
}
#[derive(DomainInput, Debug, Clone)]
struct QuadHolder {
    choice: QuadChoice,
}
#[test]
fn enum_coordinates_use_dimension_sector_and_variant_slot_for_positive_and_excluded_variants() {
    let d = domain!(Choices {
        a.all_enum_values(),
        b.all_enum_values(),
        c.all_enum_values()
    })
    .compile()
    .unwrap();
    let c0 = d
        .classify(&Choices {
            a: Choice::A,
            b: Choice::B,
            c: Choice::C,
        })
        .unwrap();
    let c1 = d
        .classify(&Choices {
            a: Choice::B,
            b: Choice::C,
            c: Choice::A,
        })
        .unwrap();
    let c2 = d
        .classify(&Choices {
            a: Choice::C,
            b: Choice::A,
            c: Choice::B,
        })
        .unwrap();
    assert_eq!(c0.entries[0].sector.region, DomainRegion::Enum);
    assert_eq!(c0.entries[0].sector, c1.entries[0].sector);
    assert_eq!(c1.entries[0].sector, c2.entries[0].sector);
    assert_eq!(c0.entries[0].slot, CaseDomainCoordinateSlot::EnumVariant(0));
    assert_eq!(c1.entries[0].slot, CaseDomainCoordinateSlot::EnumVariant(1));
    assert_eq!(c2.entries[0].slot, CaseDomainCoordinateSlot::EnumVariant(2));

    let mut cert = certification("enum prefill").executor(|_: &Choices| Ok::<usize, ()>(1usize));
    cert.data_source([Choices {
        a: Choice::A,
        b: Choice::A,
        c: Choice::A,
    }]);
    cert.domain(domain!(Choices {
        a.all_enum_values(),
        b.all_enum_values(),
        c.all_enum_values()
    }));
    cert.cases_budget(32);
    let case = cert.case();
    cert.check(fact!(*case.result()).expect(1usize));
    let report = cert.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (9, 9));
    assert_eq!(report.coverage.pairwise_combinations, (27, 27));

    let dq = domain!(QuadHolder {
        choice.selected_enum_values([QuadChoice::A, QuadChoice::C])
    })
    .compile()
    .unwrap();
    let ex_b = dq
        .classify(&QuadHolder {
            choice: QuadChoice::B,
        })
        .unwrap();
    let ex_d = dq
        .classify(&QuadHolder {
            choice: QuadChoice::D,
        })
        .unwrap();
    assert_eq!(ex_b.violated, vec![0]);
    assert_eq!(ex_d.violated, vec![0]);
    assert_eq!(ex_b.entries[0].sector.region, DomainRegion::ExcludedVariant);
    assert_eq!(ex_b.entries[0].sector, ex_d.entries[0].sector);
    assert_eq!(
        ex_b.entries[0].slot,
        CaseDomainCoordinateSlot::ExcludedVariant(1)
    );
    assert_eq!(
        ex_d.entries[0].slot,
        CaseDomainCoordinateSlot::ExcludedVariant(3)
    );
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum NestedChoice {
    A,
    B,
    C,
    D { inner: QuadChoice },
}
#[derive(DomainInput, Debug, Clone)]
struct NestedChoiceHolder {
    choice: NestedChoice,
}
#[test]
fn nested_enum_pairwise_coverage_counts_only_feasible_combinations() {
    let mut c = certification("nested enum pair coverage")
        .executor(|_: &NestedChoiceHolder| Ok::<usize, ()>(1));
    c.domain(domain!(NestedChoiceHolder {
        choice {
            all_enum_values(),
            NestedChoice::D { inner.all_enum_values() }
        }
    }));
    c.cases_budget(32).require_pairwise_coverage();
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (8, 8));
    assert_eq!(report.coverage.pairwise_combinations, (4, 4));
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum ConditionalBackend {
    X,
    Y,
    Z,
}
#[derive(DomainInput, Debug, Clone)]
struct ConditionalEnumPair {
    mode: BinaryMode,
    backend: ConditionalBackend,
}
#[test]
fn conditional_enum_pairwise_coverage_counts_only_active_combinations() {
    let mut c =
        certification("conditional enum pair coverage").executor(|input: &ConditionalEnumPair| {
            match (&input.mode, &input.backend) {
                (BinaryMode::A, ConditionalBackend::X)
                | (BinaryMode::B, ConditionalBackend::Y | ConditionalBackend::Z) => Ok(1usize),
                _ => Err(()),
            }
        });
    c.domain(domain!(ConditionalEnumPair {
        mode.all_enum_values(),
        when(mode.equal(BinaryMode::A)) {
            backend.selected_enum_values([ConditionalBackend::X])
        },
        when(mode.equal(BinaryMode::B)) {
            backend.selected_enum_values([ConditionalBackend::Y, ConditionalBackend::Z])
        }
    }));
    c.cases_budget(32).require_pairwise_coverage();
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (5, 5));
    assert_eq!(report.coverage.pairwise_combinations, (3, 3));
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum BinaryMode {
    A,
    B,
}
#[derive(DomainInput, Debug, Clone)]
struct OverlapItem {
    mode: BinaryMode,
    value: i32,
}
#[derive(DomainInput, Debug, Clone)]
struct OverlapBatch {
    items: Vec<OverlapItem>,
}
#[test]
fn conditional_collection_compositions_are_generated_in_each_active_branch() {
    let d = domain!(OverlapBatch {
        items {
            length().fixed_value(4),
            each {
                mode.all_enum_values(),
                when(mode.equal(BinaryMode::A)) { value.in_range(1..=30) },
                when(mode.equal(BinaryMode::B)) { value.in_range(1..=30) }
            }
        }
    })
    .compile()
    .unwrap();
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut saw_a = false;
    let mut saw_b = false;
    while let Some((batch, coordinate)) = d.next(0, &mut cursor, 41, &occupied).unwrap() {
        occupied.insert(coordinate_key(&coordinate));
        let heterogeneous = batch.items.windows(2).any(|w| w[0].value != w[1].value);
        if !heterogeneous {
            continue;
        }
        if batch.items.iter().all(|item| item.mode == BinaryMode::A) {
            saw_a = true;
        }
        if batch.items.iter().all(|item| item.mode == BinaryMode::B) {
            saw_b = true;
        }
        if saw_a && saw_b {
            break;
        }
    }
    assert!(saw_a && saw_b, "saw_a={saw_a}, saw_b={saw_b}");
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum RepeatedChoice {
    One,
    Two,
}
#[derive(DomainInput, Debug, Clone)]
struct RepeatedConditionalEnums {
    mode: BinaryMode,
    left: RepeatedChoice,
    right: RepeatedChoice,
}
#[test]
fn pairwise_enum_coverage_distinguishes_same_slots_in_different_conditional_sectors() {
    let mut c = certification("conditional enum sector pairs")
        .executor(|_: &RepeatedConditionalEnums| Ok::<usize, ()>(1));
    c.domain(domain!(RepeatedConditionalEnums {
        mode.all_enum_values(),
        when(mode.equal(BinaryMode::A)) {
            left.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two]),
            right.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two])
        },
        when(mode.equal(BinaryMode::B)) {
            left.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two]),
            right.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two])
        }
    }));
    c.cases_budget(22);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (10, 10));
    assert_eq!(report.coverage.pairwise_combinations, (16, 16));
}

mod edge_cases {
    use super::*;
    use std::{cell::Cell, collections::BTreeSet};

    #[test]
    fn two_enum_dimensions_finish_pairwise_gaps_after_individual_slots() {
        #[derive(DomainInput)]
        enum Choice {
            A,
            B,
            C,
        }
        #[derive(DomainInput)]
        struct Input {
            a: Choice,
            b: Choice,
        }
        let d = domain!(Input{a.all_enum_values(),b.all_enum_values()})
            .compile()
            .unwrap();
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        while let Some((_, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
            occupied.insert(coordinate_key(&c));
        }
        assert_eq!(occupied.len(), 9);
    }

    #[test]
    fn mandatory_real_cases_execute_before_no_generation_capacity_is_reported() {
        let calls = Cell::new(0);
        let mut c = certification("mandatory real inputs").executor(|i: &usize| {
            calls.set(calls.get() + 1);
            if (1..=9).contains(i) {
                Ok(())
            } else {
                Err(())
            }
        });
        c.domain(domain!(usize{in_range(1..=9)}));
        c.data_source([1usize, 2, 3]);
        c.cases_budget(1);
        let r = c.report();
        assert_eq!(calls.get(), 3);
        assert_eq!(r.executed, [3, 0]);
        assert!(r.evaluation_error.unwrap().message.contains("No budget"));
    }

    #[test]
    fn unsigned_128_values_preserve_materialization_and_structural_identity() {
        let start = i128::MAX as u128 + 10;
        let d = domain!(u128{in_range(start..=start+8)}).compile().unwrap();
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        let mut values = vec![];
        while let Some((input, c)) = d.next(0, &mut cursor, 42, &occupied).unwrap() {
            assert!((start..=start + 8).contains(&input));
            assert_eq!(d.classify(&input).unwrap(), c);
            values.push(input);
            occupied.insert(coordinate_key(&c));
        }
        assert_eq!(values.len(), 9);
        assert_eq!(&values[..3], &[start, start + 8, start + 4]);
    }

    #[test]
    fn certification_identity_has_no_shared_rng_or_test_order_state() {
        fn run() -> Vec<i32> {
            let seen = std::cell::RefCell::new(vec![]);
            let mut c = certification("stable definition").executor(|i: &Scalar| {
                seen.borrow_mut().push(i.value);
                if (1..=1000).contains(&i.value) {
                    Ok(())
                } else {
                    Err(())
                }
            });
            c.domain(domain!(Scalar{value.in_range(1..=1000)}));
            c.cases_budget(30);
            c.seed(77);
            c.assert_certify();
            seen.into_inner()
        }
        let first = run();
        let mut other = certification("unrelated").executor(|i: &usize| Ok::<_, ()>(*i));
        other.data_source([3usize]);
        other.assert_certify();
        assert_eq!(first, run());
        assert_eq!(first, std::thread::spawn(run).join().unwrap());
    }

    #[derive(DomainInput, Debug)]
    struct Scalar {
        value: i32,
    }
    thread_local! {static MATERIALIZED:Cell<usize>=const{Cell::new(0)};}
    struct Counted {
        value: i32,
    }
    impl DomainInput for Counted {
        type Fields = ScalarDomainFields;
        fn schema() -> DomainSchema {
            Scalar::schema()
        }
        fn domain_fields(field: DomainField<Self>) -> Self::Fields {
            ScalarDomainFields {
                value: field.child(DomainPathStep::Field(0)),
            }
        }
        fn to_domain_value(&self) -> DomainValue {
            Scalar { value: self.value }.to_domain_value()
        }
        fn from_domain_value(value: DomainValue) -> Result<Self, AnalysisError> {
            MATERIALIZED.with(|n| n.set(n.get() + 1));
            Scalar::from_domain_value(value).map(|v| Self { value: v.value })
        }
    }

    #[test]
    fn hard_stop_prevents_even_materializing_another_case() {
        MATERIALIZED.with(|n| n.set(0));
        let executed = Cell::new(0);
        let mut c = certification("materialization boundary").executor(|_: &Counted| {
            executed.set(executed.get() + 1);
            Err::<(), _>(())
        });
        c.domain(domain!(Counted{value.in_range(1..=1000)}));
        c.failure_budget(1);
        c.cases_budget(100);
        let r = c.report();
        assert!(r.stopped);
        assert_eq!(r.cases_executed(), 1);
        assert_eq!(executed.get(), 1);
        assert_eq!(MATERIALIZED.with(Cell::get), 1);
    }

    #[derive(DomainInput, Debug, PartialEq)]
    enum Mode {
        A,
        B,
    }
    #[derive(DomainInput, Debug)]
    struct Row {
        mode: Mode,
        value: i32,
    }
    #[derive(DomainInput, Debug)]
    struct Batch {
        rows: Vec<Row>,
    }
    fn batch_domain() -> Domain<Batch> {
        domain!(Batch{rows{length().in_range(2..=3),each{mode.all_enum_values(),when(mode.equal(Mode::A)){value.in_range(1..=3)},when(mode.equal(Mode::B)){value.in_range(5..=7)}}}})
    }

    #[test]
    fn heterogeneous_collection_rows_classify_their_own_conditions_and_slots() {
        let d = batch_domain().compile().unwrap();
        let good = Batch {
            rows: vec![
                Row {
                    mode: Mode::A,
                    value: 1,
                },
                Row {
                    mode: Mode::B,
                    value: 7,
                },
            ],
        };
        let c = d.classify(&good).unwrap();
        assert!(c.violated.is_empty());
        assert_eq!(
            c.entries
                .iter()
                .filter(|e| e.name.ends_with("mode"))
                .count(),
            2
        );
        let bad = Batch {
            rows: vec![
                Row {
                    mode: Mode::A,
                    value: 1,
                },
                Row {
                    mode: Mode::B,
                    value: 1,
                },
            ],
        };
        assert_eq!(d.classify(&bad).unwrap().violated.len(), 1);
    }

    #[derive(DomainInput, Debug)]
    struct Float {
        value: f32,
    }

    #[test]
    fn f32_materialization_round_trips_coordinates_and_negative_near_is_adjacent() {
        let d = domain!(Float{value.in_range(0.0..=0.001)})
            .compile()
            .unwrap();
        for category in [0, 1] {
            let mut cursor = 0;
            let mut occupied = BTreeSet::new();
            let mut count = 0;
            while let Some((input, c)) = d.next(category, &mut cursor, 42, &occupied).unwrap() {
                assert_eq!(d.classify(&input).unwrap(), c);
                if category == 1
                    && input.value < 0.
                    && matches!(
                        c.entries[0].slot,
                        CaseDomainCoordinateSlot::Negative(DomainNegativeDistance::Near)
                    )
                {
                    assert_eq!(input.value, -f32::from_bits(1));
                }
                occupied.insert(coordinate_key(&c));
                count += 1;
                if count == 20 {
                    break;
                }
            }
            assert!(count > 2);
        }
    }

    #[test]
    fn a_redundant_relation_can_still_have_a_pairwise_negative() {
        #[derive(DomainInput)]
        struct Pair {
            left: i32,
            right: i32,
        }
        let d = domain!(Pair{left.in_range(0..=1),right.in_range(2..=3),left.less_equal(right)})
            .compile()
            .unwrap();
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        let mut found = false;
        while let Some((_, c)) = d.next(2, &mut cursor, 1, &occupied).unwrap() {
            found |= c.violated.contains(&2);
            occupied.insert(coordinate_key(&c));
        }
        assert!(found);
    }

    #[test]
    fn lazy_higher_order_enum_exploration_fills_the_remaining_triple() {
        #[derive(DomainInput, PartialEq)]
        enum Bit {
            Zero,
            One,
        }
        #[derive(DomainInput)]
        struct Bits {
            a: Bit,
            b: Bit,
            c: Bit,
        }
        let d = domain!(Bits{a.all_enum_values(),b.all_enum_values(),c.all_enum_values()})
            .compile()
            .unwrap();
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        let mut triple = false;
        while let Some((i, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
            triple |= i.a == Bit::One && i.b == Bit::One && i.c == Bit::One;
            occupied.insert(coordinate_key(&c));
        }
        assert!(triple);
        assert_eq!(occupied.len(), 8);
    }

    #[test]
    fn relation_negative_slots_have_a_separate_severity_axis() {
        #[derive(DomainInput)]
        struct Pair {
            a: i32,
            b: i32,
        }
        let d = domain!(Pair{a.in_range(0..=90),b.in_range(0..=90),a.less_equal(b)})
            .compile()
            .unwrap();
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        let mut distances = BTreeSet::new();
        while let Some((_, c)) = d.next(1, &mut cursor, 1, &occupied).unwrap() {
            if c.violated == [2] {
                for e in &c.entries {
                    if e.sector.constraint == 2 {
                        distances.insert(e.slot.clone());
                    }
                }
            }
            occupied.insert(coordinate_key(&c));
        }
        assert!(distances.contains(&CaseDomainCoordinateSlot::Negative(
            DomainNegativeDistance::Near
        )));
        assert!(distances.contains(&CaseDomainCoordinateSlot::Negative(
            DomainNegativeDistance::Far
        )));
    }

    #[test]
    fn unmaterializable_and_nonfinite_domains_fail_closed() {
        assert!(domain!(Float{value.in_range(f32::NEG_INFINITY..=1.0)})
            .compile()
            .is_err());
        assert!(domain!(i128{in_range(i128::MIN..=i128::MAX)})
            .compile()
            .is_err());
        #[derive(DomainInput)]
        struct Large {
            values: Vec<u8>,
        }
        assert!(
            domain!(Large{values{length().in_range(1..=2_000_000),each(fixed_value(1))}})
                .compile()
                .is_err()
        );
    }

    #[test]
    fn disconnected_large_finite_ranges_preserve_the_global_center_anchor() {
        let d = domain!(f64{one_of_ranges([-1.7e308..=-1.0e307,1.0e307..=1.7e308])})
            .compile()
            .unwrap();
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        let mut values = vec![];
        for _ in 0..3 {
            let (value, coordinate) = d.next(0, &mut cursor, 1, &occupied).unwrap().unwrap();
            occupied.insert(coordinate_key(&coordinate));
            values.push(value);
        }
        assert_eq!(values, vec![-1.7e308, 1.7e308, 1.0e307]);
    }

    #[test]
    fn empty_named_struct_has_one_materializable_positive_case() {
        #[derive(DomainInput)]
        struct Empty {}
        let mut c = certification("empty input").executor(|_: &Empty| Ok::<_, ()>(()));
        c.domain(domain!(Empty {}));
        let r = c.report();
        assert!(r.is_pass(), "{r}");
        assert_eq!(r.cases_executed(), 1);
    }

    #[test]
    fn coverage_counts_only_feasible_positive_opportunities() {
        #[derive(DomainInput)]
        struct Pair {
            a: i32,
            b: i32,
        }
        let mut c = certification("feasible coverage").executor(|i: &Pair| {
            if (0..=1).contains(&i.a) && (0..=9).contains(&i.b) && i.a == i.b {
                Ok(())
            } else {
                Err(())
            }
        });
        c.domain(domain!(Pair{a.in_range(0..=1),b.in_range(0..=9),a.equal(b)}));
        c.cases_budget(64);
        let r = c.report();
        assert!(r.is_pass(), "{r}");
        assert!(
            r.coverage.positive_anchors.0 <= r.coverage.positive_anchors.1,
            "{:?}",
            r.coverage
        );
    }

    #[test]
    fn real_triple_negative_does_not_replace_exact_pairwise_cases() {
        #[derive(DomainInput)]
        struct Triple {
            a: i32,
            b: i32,
            c: i32,
        }
        let d = domain!(Triple{a.in_range(1..=3),b.in_range(1..=3),c.in_range(1..=3)})
            .compile()
            .unwrap();
        let real = d.classify(&Triple { a: 0, b: 0, c: 0 }).unwrap();
        assert_eq!(real.violated, vec![0, 1, 2]);
        let mut occupied = BTreeSet::from([coordinate_key(&real)]);
        let mut cursor = 0;
        let mut pairs = BTreeSet::new();
        while let Some((_, coordinate)) = d.next(2, &mut cursor, 1, &occupied).unwrap() {
            pairs.insert(coordinate.violated.clone());
            occupied.insert(coordinate_key(&coordinate));
        }
        assert_eq!(pairs, BTreeSet::from([vec![0, 1], vec![0, 2], vec![1, 2]]));
    }

    #[test]
    fn heterogeneous_collection_rows_do_not_fake_enum_pairwise_coverage() {
        #[derive(DomainInput, Clone, Debug, PartialEq)]
        enum Choice {
            A,
            B,
        }
        #[derive(DomainInput, Debug)]
        struct Row {
            left: Choice,
            right: Choice,
        }
        #[derive(DomainInput, Debug)]
        struct Batch {
            rows: Vec<Row>,
        }
        let domain = domain!(Batch {
        rows {
        length().fixed_value(2),
        each {
        left.all_enum_values(),
        right.all_enum_values(),
        },
        },
        })
        .compile()
        .unwrap();
        let real = Batch {
            rows: vec![
                Row {
                    left: Choice::A,
                    right: Choice::A,
                },
                Row {
                    left: Choice::B,
                    right: Choice::B,
                },
            ],
        };
        let mut occupied = BTreeSet::from([coordinate_key(&domain.classify(&real).unwrap())]);
        let mut cursor = 0;
        let mut generated = BTreeSet::new();
        while let Some((input, coordinate)) = domain.next(0, &mut cursor, 1, &occupied).unwrap() {
            let row = &input.rows[0];
            generated.insert((
                matches!(row.left, Choice::B),
                matches!(row.right, Choice::B),
            ));
            occupied.insert(coordinate_key(&coordinate));
        }
        assert!(generated.contains(&(false, true)));
        assert!(generated.contains(&(true, false)));
    }
}
