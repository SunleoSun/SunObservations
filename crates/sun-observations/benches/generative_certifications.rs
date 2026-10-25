extern crate self as sun_observations;
extern crate sun_observations as runtime;
pub use runtime::*;

// Compile the canonical framework here to reach crate-private stage boundaries without a public benchmark API.
#[allow(dead_code, unused_imports)]
#[path = "../src/certifications/mod.rs"]
pub mod certifications;
#[path = "support/mod.rs"]
mod support;
use certifications::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, hint::black_box, time::Instant};

const SEED: u64 = 184729361;
const ITERATIONS: usize = 5;
#[derive(Clone, Copy, Default)]
struct Shape {
    budget: usize,
    dimensions: usize,
    cardinality: usize,
    potential: u128,
    constraints: usize,
    negative_pairs: usize,
    length: usize,
    records: usize,
    checks: usize,
}
#[derive(Clone, Copy, Default)]
struct Counts {
    executed: usize,
    positive: usize,
    single: usize,
    multi: usize,
    stopped: bool,
}
fn counts(r: &PlannedCertificationReport) -> Counts {
    Counts {
        executed: r.cases_executed(),
        positive: r.categories[0],
        single: r.categories[1],
        multi: r.categories[2],
        stopped: r.stopped,
    }
}
fn measure<S, O>(
    operation: &str,
    shape_name: &str,
    shape: Shape,
    mut setup: impl FnMut() -> S,
    mut op: impl FnMut(S) -> O,
    inspect: impl Fn(&O) -> Counts,
) {
    let warm = op(setup());
    black_box(inspect(&warm));
    drop(warm);
    for sample in 0..3 {
        let mut elapsed = 0.;
        let mut observed = Counts::default();
        for _ in 0..ITERATIONS {
            let input = setup();
            let start = Instant::now();
            let output = op(black_box(input));
            elapsed += start.elapsed().as_secs_f64() * 1000.;
            observed = inspect(&output);
            black_box(output);
        }
        println!("{operation},{shape_name},{},{},{},{},{},{},{},{},{},{sample},{ITERATIONS},{:.6},{},{},{},{},{}",shape.budget,shape.dimensions,shape.cardinality,shape.potential,shape.constraints,shape.negative_pairs,shape.length,shape.records,shape.checks,elapsed/ITERATIONS as f64,observed.executed,observed.positive,observed.single,observed.multi,observed.stopped);
    }
}
fn checked(r: &PlannedCertificationReport) -> Counts {
    assert!(r.is_pass(), "{r}");
    counts(r)
}
fn compile<T: DomainInput + Clone>(name: &str, domain: &Domain<T>, shape: Shape) {
    measure(
        "domain_compile",
        name,
        shape,
        || domain.clone(),
        |d| d.compile().unwrap(),
        |_| Counts::default(),
    );
}
#[derive(Clone, Copy, Debug, PartialEq, DomainInput)]
enum Choice {
    V0,
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
    V9,
    V10,
    V11,
    V12,
    V13,
    V14,
    V15,
    V16,
    V17,
    V18,
    V19,
    V20,
    V21,
    V22,
    V23,
    V24,
    V25,
    V26,
    V27,
    V28,
    V29,
    V30,
    V31,
}
fn choices(n: usize) -> Vec<Choice> {
    (0..n)
        .map(|i| Choice::from_domain_value(DomainValue::Enum(i, vec![])).unwrap())
        .collect()
}
#[derive(Clone, Debug, DomainInput)]
struct Wide {
    a: Choice,
    b: Choice,
    c: Choice,
    d: Choice,
    e: Choice,
    f: Choice,
    g: Choice,
    h: Choice,
}
fn wide(dimensions: usize, cardinality: usize) -> Domain<Wide> {
    let f = DomainField::<Wide>::root().fields();
    let mut domain = Domain::new();
    for (i, field) in [f.a, f.b, f.c, f.d, f.e, f.f, f.g, f.h].iter().enumerate() {
        domain.add(
            field.selected_enum_values(choices(if i < dimensions { cardinality } else { 1 })),
            &[],
        );
    }
    domain
}
fn wide_valid(v: &Wide, dimensions: usize, cardinality: usize) -> bool {
    [v.a, v.b, v.c, v.d, v.e, v.f, v.g, v.h]
        .iter()
        .enumerate()
        .all(|(i, v)| (*v as usize) < if i < dimensions { cardinality } else { 1 })
}
fn explore<T: DomainInput>(compiled: &CompiledDomain<T>, budget: usize) -> Counts {
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut executed = 0;
    while executed < budget {
        let Some((input, coordinate)) = compiled.next(0, &mut cursor, SEED, &occupied).unwrap()
        else {
            break;
        };
        black_box(input);
        occupied.insert(
            coordinate
                .entries
                .iter()
                .map(|e| (e.sector.clone(), e.slot.clone()))
                .collect(),
        );
        executed += 1;
    }
    Counts {
        executed,
        positive: executed,
        ..Counts::default()
    }
}
fn lazy() {
    for (dimensions, cardinality) in [
        (4, 2usize),
        (4, 4),
        (4, 8),
        (4, 16),
        (4, 32),
        (2, 8),
        (6, 8),
        (8, 8),
    ] {
        let domain = wide(dimensions, cardinality);
        let compiled = domain.clone().compile().unwrap();
        for budget in [8, 64, 256] {
            let shape = Shape {
                budget,
                dimensions,
                cardinality,
                potential: (cardinality as u128).pow(dimensions as u32),
                constraints: 8,
                negative_pairs: {
                    let n = if cardinality < 32 { 8 } else { 8 - dimensions };
                    n * n.saturating_sub(1) / 2
                },
                ..Shape::default()
            };
            compile("enum_product", &domain, shape);
            measure(
                "planner_next_materialize",
                "enum_product",
                shape,
                || (),
                |_| explore(&compiled, budget),
                |c| {
                    assert!(c.executed > 0);
                    *c
                },
            );
            let n = budget.min(compiled.enum_pair_target_count());
            measure(
                "planner_pair_target_selection",
                "enum_product",
                shape,
                || (),
                |_| {
                    (0..n)
                        .map(|i| compiled.enum_pair_target(i).unwrap())
                        .collect::<Vec<_>>()
                },
                |v| Counts {
                    executed: v.len(),
                    ..Counts::default()
                },
            );
            let targets: Vec<_> = (0..n)
                .map(|i| compiled.enum_pair_target(i).unwrap())
                .collect();
            measure(
                "target_solve",
                "enum_product",
                shape,
                || &targets,
                |targets| {
                    targets
                        .iter()
                        .map(|t| compiled.solve_target(t).unwrap().unwrap())
                        .collect::<Vec<_>>()
                },
                |v| Counts {
                    executed: v.len(),
                    ..Counts::default()
                },
            );
            measure(
                "planner_higher_order_target_selection",
                "enum_product",
                shape,
                || (),
                |_| {
                    (0..budget)
                        .filter_map(|i| compiled.higher_order_enum_target(i))
                        .collect::<Vec<_>>()
                },
                |v| Counts {
                    executed: v.len(),
                    ..Counts::default()
                },
            );
            let solved: Vec<_> = targets
                .iter()
                .map(|t| compiled.solve_target(t).unwrap().unwrap())
                .collect();
            measure(
                "input_materialize",
                "enum_product",
                shape,
                || (),
                |_| {
                    targets
                        .iter()
                        .zip(&solved)
                        .map(|(t, env)| {
                            Wide::from_domain_value(
                                compiled.materialize_value(env, t, SEED).unwrap(),
                            )
                            .unwrap()
                        })
                        .collect::<Vec<_>>()
                },
                |v| Counts {
                    executed: v.len(),
                    ..Counts::default()
                },
            );
            measure(
                "planned_report",
                "enum_product",
                shape,
                || {
                    let mut c = certification("lazy benchmark").executor(move |i: &Wide| {
                        if wide_valid(i, dimensions, cardinality) {
                            Ok(())
                        } else {
                            Err(())
                        }
                    });
                    c.domain(domain.clone());
                    c.seed(SEED).cases_budget(budget).report_mode(false);
                    c
                },
                |c| c.report(),
                checked,
            );
        }
    }
}
#[derive(Clone, DomainInput)]
struct Scalar {
    value: i64,
}
fn scalar(end: i64) -> Domain<Scalar> {
    domain!(Scalar{value.in_range(0..=end)})
}
fn scalar_runner(
    domain: Domain<Scalar>,
    end: i64,
    budget: usize,
    checks: usize,
) -> PlannedCertification<'static, Scalar, Result<i64, ()>, impl FnMut(&Scalar) -> Result<i64, ()>>
{
    let mut c = certification("scalar benchmark").executor(move |i: &Scalar| {
        if (0..=end).contains(&i.value) {
            Ok(i.value)
        } else {
            Err(())
        }
    });
    c.domain(domain);
    c.seed(SEED).cases_budget(budget).report_mode(false);
    let case = c.case();
    for _ in 0..checks {
        c.check(fact!(*case.result()).expect(fact!(case.input().value)));
    }
    c
}
fn ranges_and_execution() {
    for end in [9, 1_000_000_000_000] {
        let domain = scalar(end);
        let compiled = domain.clone().compile().unwrap();
        compile(
            "scalar_range",
            &domain,
            Shape {
                constraints: 1,
                potential: (end + 1) as u128,
                ..Shape::default()
            },
        );
        for budget in [2, 8, 64] {
            let shape = Shape {
                budget,
                constraints: 1,
                potential: (end + 1) as u128,
                ..Shape::default()
            };
            measure(
                "range_explore",
                "anchors_then_strata",
                shape,
                || (),
                |_| explore(&compiled, budget),
                |c| *c,
            );
            measure(
                "range_target_selection",
                "post_anchor",
                shape,
                || (),
                |_| {
                    (0..budget)
                        .map(|i| compiled.free_target(i, SEED).unwrap())
                        .collect::<Vec<_>>()
                },
                |v| Counts {
                    executed: v.iter().filter(|t| t.is_some()).count(),
                    ..Counts::default()
                },
            );
        }
    }
    let end = 1_000_000_000_000;
    let domain = scalar(end);
    for budget in [8, 64, 256, 1024] {
        let shape = Shape {
            budget,
            constraints: 1,
            potential: (end + 1) as u128,
            ..Shape::default()
        };
        measure(
            "full_planned",
            "scalar_frontier",
            shape,
            || domain.clone(),
            |d| scalar_runner(d, end, budget, 0).report(),
            checked,
        );
    }
    let full_domain = wide(8, 32);
    for budget in [8, 64, 256, 1024] {
        measure(
            "full_planned",
            "enum_noop",
            Shape {
                budget,
                dimensions: 8,
                cardinality: 32,
                potential: 32u128.pow(8),
                constraints: 8,
                ..Shape::default()
            },
            || full_domain.clone(),
            |d| {
                let mut c =
                    certification("full noop benchmark").executor(|_: &Wide| Ok::<_, ()>(()));
                c.domain(d);
                c.seed(SEED).cases_budget(budget).report_mode(false);
                c.report()
            },
            |r| {
                assert_eq!(r.cases_executed(), budget);
                checked(r)
            },
        );
    }
    for checks in [0, 1, 5, 20] {
        let shape = Shape {
            budget: 256,
            checks,
            constraints: 1,
            ..Shape::default()
        };
        measure(
            "planned_check_evaluation",
            "cheap_scalar",
            shape,
            || scalar_runner(domain.clone(), end, 256, checks),
            |c| c.report(),
            checked,
        );
        let inputs: Vec<_> = (0..256).map(|value| Scalar { value }).collect();
        measure(
            "checks_real_cases",
            "without_generation",
            Shape {
                constraints: 0,
                ..shape
            },
            || {
                let mut c = certification("check evaluation benchmark")
                    .executor(|i: &Scalar| Ok::<_, ()>(i.value));
                c.data_source(inputs.clone());
                c.seed(SEED).cases_budget(256).report_mode(false);
                let case = c.case();
                for _ in 0..checks {
                    c.check(fact!(*case.result()).expect(fact!(case.input().value)));
                }
                c
            },
            |c| c.report(),
            checked,
        );
    }
}
#[derive(Clone, DomainInput)]
enum Nested {
    Off,
    On { count: usize },
}
#[derive(Clone, DomainInput)]
struct Conditional {
    mode: Choice,
    payload: Nested,
    window: usize,
}
#[derive(Clone, DomainInput)]
struct Numbers {
    a: i64,
    b: i64,
    c: i64,
    d: i64,
}
fn numbers(relations: usize) -> Domain<Numbers> {
    let f = DomainField::<Numbers>::root().fields();
    let mut domain = domain!(Numbers{a.in_range(0..=1000),b.in_range(0..=1000),c.in_range(0..=1000),d.in_range(0..=1000)});
    for r in [
        f.a.less_equal(&f.b),
        f.b.less_equal(&f.c),
        f.c.less_equal(&f.d),
        f.a.less_equal(&f.c),
        f.a.less_equal(&f.d),
        f.b.less_equal(&f.d),
    ]
    .into_iter()
    .take(relations)
    {
        domain.add(r, &[]);
    }
    domain
}
fn numbers_valid(i: &Numbers, relations: usize) -> bool {
    [i.a, i.b, i.c, i.d].iter().all(|v| (0..=1000).contains(v))
        && [
            i.a <= i.b,
            i.b <= i.c,
            i.c <= i.d,
            i.a <= i.c,
            i.a <= i.d,
            i.b <= i.d,
        ]
        .into_iter()
        .take(relations)
        .all(|v| v)
}
#[derive(Clone, DomainInput)]
struct Collection {
    values: Vec<Choice>,
}
#[derive(Clone, DomainInput)]
struct Heterogeneous {
    values: Vec<Nested>,
}
fn domains_and_negatives() {
    let conditional = domain!(Conditional{mode.all_enum_values(),payload{all_enum_values(),Nested::On{count.in_range(1..=8)}},when(mode.equal(Choice::V0)){window.in_range(1..=2)},when(mode.not_equal(Choice::V0)){window.in_range(3..=8)}});
    compile(
        "conditional_nested_enum",
        &conditional,
        Shape {
            dimensions: 2,
            cardinality: 32,
            constraints: 5,
            ..Shape::default()
        },
    );
    for relations in [0, 1, 3, 6] {
        let domain = numbers(relations);
        let constraints = 4 + relations;
        let shape = Shape {
            budget: 256,
            constraints,
            negative_pairs: constraints * (constraints - 1) / 2,
            ..Shape::default()
        };
        compile("relations", &domain, shape);
        if relations == 6 {
            continue;
        }
        measure(
            "negative_exploration",
            "single_and_pair",
            shape,
            || {
                let mut c = certification("negative benchmark").executor(move |i: &Numbers| {
                    if numbers_valid(i, relations) {
                        Ok(())
                    } else {
                        Err(())
                    }
                });
                c.domain(domain.clone());
                c.seed(SEED).cases_budget(256).report_mode(false);
                c
            },
            |c| c.report(),
            |r| {
                assert!(r.categories[1] > 0 && r.categories[2] > 0);
                checked(r)
            },
        );
    }
    let single = scalar(1_000_000);
    measure(
        "negative_exploration",
        "single_only",
        Shape {
            budget: 256,
            constraints: 1,
            ..Shape::default()
        },
        || scalar_runner(single.clone(), 1_000_000, 256, 0),
        |c| c.report(),
        |r| {
            assert!(r.categories[1] > 0);
            assert_eq!(r.categories[2], 0);
            checked(r)
        },
    );
    let positive = wide(8, 32);
    measure(
        "negative_exploration",
        "positive_only",
        Shape {
            budget: 256,
            dimensions: 8,
            cardinality: 32,
            constraints: 8,
            potential: 32u128.pow(8),
            ..Shape::default()
        },
        || {
            let mut c = certification("positive benchmark").executor(|_: &Wide| Ok::<_, ()>(()));
            c.domain(positive.clone());
            c.seed(SEED).cases_budget(256).report_mode(false);
            c
        },
        |c| c.report(),
        |r| {
            assert_eq!(r.categories[1..], [0, 0]);
            checked(r)
        },
    );
    for length in [1, 8, 64, 256] {
        for cardinality in [2, 8, 32] {
            let f = DomainField::<Collection>::root().fields();
            let mut domain = Domain::new();
            domain.add(f.values.length().fixed_value(length), &[]);
            domain.add(
                f.values.each().selected_enum_values(choices(cardinality)),
                &[],
            );
            let shape = Shape {
                budget: 64,
                length,
                cardinality,
                constraints: 2,
                potential: (cardinality as u128).saturating_pow(length as u32),
                negative_pairs: usize::from(cardinality < 32),
                ..Shape::default()
            };
            compile("collection_each", &domain, shape);
            let compiled = domain.clone().compile().unwrap();
            for budget in [2, 16, 64] {
                measure(
                    "collection_explore",
                    "uniform_mixed_spread",
                    Shape { budget, ..shape },
                    || (),
                    |_| explore(&compiled, budget),
                    |c| *c,
                );
            }
            let pair_count = compiled.collection_composition_pair_count();
            let spread_count = compiled.collection_composition_spread_count();
            for (name, start, count) in [
                ("pairwise_mixed", 0, pair_count.min(64)),
                ("spread", pair_count, spread_count),
            ] {
                if count == 0 {
                    continue;
                }
                let targets: Vec<_> = (start..start + count)
                    .map(|i| compiled.collection_composition_target(i).unwrap())
                    .collect();
                measure(
                    "collection_target_solve",
                    name,
                    Shape {
                        budget: count,
                        ..shape
                    },
                    || &targets,
                    |targets| {
                        targets
                            .iter()
                            .map(|t| compiled.solve_target(t).unwrap())
                            .collect::<Vec<_>>()
                    },
                    |v| Counts {
                        executed: v.iter().filter(|v| v.is_some()).count(),
                        ..Counts::default()
                    },
                );
                let solved: Vec<_> = targets
                    .into_iter()
                    .filter_map(|t| compiled.solve_target(&t).unwrap().map(|env| (t, env)))
                    .collect();
                if solved.is_empty() {
                    continue;
                }
                measure(
                    "collection_materialize",
                    name,
                    Shape {
                        budget: solved.len(),
                        ..shape
                    },
                    || (),
                    |_| {
                        solved
                            .iter()
                            .map(|(t, env)| {
                                Collection::from_domain_value(
                                    compiled.materialize_value(env, t, SEED).unwrap(),
                                )
                                .unwrap()
                            })
                            .collect::<Vec<_>>()
                    },
                    |v| Counts {
                        executed: v.len(),
                        ..Counts::default()
                    },
                );
            }
            let input = Collection {
                values: vec![Choice::V0; length],
            };
            let uniform_value = input.to_domain_value();
            measure(
                "collection_materialize",
                "uniform_typed_value",
                shape,
                || uniform_value.clone(),
                |v| Collection::from_domain_value(v).unwrap(),
                |_| Counts {
                    executed: 1,
                    ..Counts::default()
                },
            );
            measure(
                "classification",
                "collection_each",
                shape,
                || (),
                |_| compiled.classify(&input).unwrap(),
                |_| Counts {
                    executed: 1,
                    ..Counts::default()
                },
            );
        }
    }
    let hetero = domain!(Heterogeneous{values{length().in_range(1..=32),each{all_enum_values(),Nested::On{count.in_range(1..=8)}}}});
    compile(
        "heterogeneous_collection",
        &hetero,
        Shape {
            length: 32,
            cardinality: 2,
            constraints: 3,
            ..Shape::default()
        },
    );
    let compiled = hetero.compile().unwrap();
    measure(
        "collection_explore",
        "heterogeneous_payloads",
        Shape {
            budget: 64,
            length: 32,
            ..Shape::default()
        },
        || (),
        |_| explore(&compiled, 64),
        |c| *c,
    );
}
#[derive(Clone, DomainInput)]
struct Stopping {
    a: Choice,
    b: Choice,
    c: Choice,
    d: Choice,
    value: i64,
}
fn real_sources_and_stops() {
    let domain = scalar(1_000_000);
    let compiled = domain.clone().compile().unwrap();
    for n in [10, 100, 10_000] {
        let inputs: Vec<_> = (0..n).map(|v| Scalar { value: v as i64 }).collect();
        let shape = Shape {
            budget: n + 1,
            records: n,
            constraints: 1,
            ..Shape::default()
        };
        measure(
            "classification",
            "real_inputs",
            shape,
            || (),
            |_| {
                inputs
                    .iter()
                    .map(|i| compiled.classify(i).unwrap())
                    .collect::<Vec<_>>()
            },
            |v| Counts {
                executed: v.len(),
                ..Counts::default()
            },
        );
        measure(
            "data_source_prefill",
            "classify_execute_then_one_generated",
            shape,
            || {
                let mut c = scalar_runner(domain.clone(), 1_000_000, n + 1, 0);
                c.data_source(inputs.clone());
                c
            },
            |c| c.report(),
            |r| {
                assert_eq!(r.executed[0], n);
                checked(r)
            },
        );
    }
    let domain = domain!(Stopping{a.all_enum_values(),b.all_enum_values(),c.all_enum_values(),d.all_enum_values(),value.in_range(0..=1_000_000)});
    for failure_budget in [1, 5, usize::MAX] {
        let fail = failure_budget != usize::MAX;
        let name = if failure_budget == 1 {
            "budget_1"
        } else if fail {
            "budget_5"
        } else {
            "no_failures"
        };
        measure(
            "failure_hard_stop",
            name,
            Shape {
                budget: 4096,
                dimensions: 4,
                cardinality: 32,
                potential: 32u128.pow(4) * 1_000_001,
                constraints: 5,
                ..Shape::default()
            },
            || {
                let mut c = certification("hard stop benchmark").executor(move |i: &Stopping| {
                    if fail || (0..=1_000_000).contains(&i.value) {
                        Ok(true)
                    } else {
                        Err(())
                    }
                });
                c.domain(domain.clone());
                let case = c.case();
                c.check(fact!(*case.result()).expect(!fail));
                c.seed(SEED)
                    .cases_budget(4096)
                    .failure_budget(if fail { failure_budget } else { 5 })
                    .report_mode(false);
                c
            },
            |c| c.report(),
            |r| {
                if fail {
                    assert!(r.stopped);
                    assert_eq!(r.failures.len(), failure_budget);
                    assert!(r.cases_executed() >= failure_budget && r.cases_executed() < 4096);
                } else {
                    assert!(r.is_pass());
                    assert_eq!(r.cases_executed(), 4096);
                }
                counts(r)
            },
        );
    }
}
#[derive(Serialize, Deserialize, runtime::Observation)]
#[observation(namespace = "generative_benchmark::row")]
struct Observed {
    value: i64,
}
fn capture_integration() {
    for budget in [8, 64, 256] {
        let inputs: Vec<_> = (0..budget).map(|value| value as i64).collect();
        let shape = Shape {
            budget,
            records: budget,
            ..Shape::default()
        };
        measure(
            "planned_plain_result",
            "real_cases",
            shape,
            || {
                let mut c = runtime::certifications::certification("plain benchmark")
                    .executor(|value: &i64| Ok::<_, ()>(*value));
                c.data_source(inputs.clone());
                c.seed(SEED).cases_budget(budget).report_mode(false);
                c
            },
            |c| c.report(),
            |r| {
                assert!(r.is_pass());
                Counts {
                    executed: r.cases_executed(),
                    positive: r.categories[0],
                    ..Counts::default()
                }
            },
        );
        measure(
            "planned_captured_run",
            "real_cases_one_observation",
            shape,
            || {
                let mut c = runtime::certifications::certification("capture benchmark").executor(
                    |value: &i64| {
                        runtime::evidence::capture_observations(
                            runtime::evidence::CaptureConfig::new().activate_all(),
                            || {
                                runtime::observe!(Observed { value: *value }).map_err(|_| ())?;
                                Ok::<_, ()>(*value)
                            },
                        )
                    },
                );
                c.data_source(inputs.clone());
                c.seed(SEED).cases_budget(budget).report_mode(false);
                c
            },
            |c| c.report(),
            |r| {
                assert!(r.is_pass());
                Counts {
                    executed: r.cases_executed(),
                    positive: r.categories[0],
                    ..Counts::default()
                }
            },
        );
    }
    measure(
        "capture_only",
        "one_observation",
        Shape {
            records: 1,
            ..Shape::default()
        },
        || (),
        |_| {
            runtime::evidence::capture_observations(
                runtime::evidence::CaptureConfig::new().activate_all(),
                || {
                    runtime::observe!(Observed { value: 7 }).unwrap();
                    Ok::<_, ()>(7)
                },
            )
        },
        |r| {
            assert!(r.status().is_complete());
            assert_eq!(r.to_typed::<Observed>().unwrap().values().len(), 1);
            Counts {
                executed: 1,
                ..Counts::default()
            }
        },
    );
}
fn seed_audit() {
    let domain = wide(8, 32);
    for budget in [64, 1024] {
        for seeded in [false, true] {
            measure(
                if seeded {
                    "seeded_report"
                } else {
                    "plain_report"
                },
                "cheap_enum",
                Shape {
                    budget,
                    dimensions: 8,
                    cardinality: 32,
                    ..Shape::default()
                },
                || (),
                |_| {
                    macro_rules! report {
                        ($c:expr) => {{
                            let mut c = $c;
                            c.domain(domain.clone());
                            c.seed(SEED).cases_budget(budget).report_mode(false);
                            c.report()
                        }};
                    }
                    if seeded {
                        report!(certification("seed audit")
                            .executor_with_seed(|_: &Wide, seed| Ok::<_, ()>(black_box(seed))))
                    } else {
                        report!(certification("seed audit")
                            .executor(|_: &Wide| Ok::<_, ()>(black_box(0u64))))
                    }
                },
                checked,
            );
        }
    }
    let compiled = domain.compile().unwrap();
    let mut cursor = 0;
    let (_, coordinate) = compiled
        .next(0, &mut cursor, SEED, &BTreeSet::new())
        .unwrap()
        .unwrap();
    measure(
        "coordinate_seed",
        "eight_entries",
        Shape {
            budget: 10000,
            dimensions: 8,
            ..Shape::default()
        },
        || (),
        |_| {
            for _ in 0..10000 {
                black_box(compilation::local_seed(
                    SEED,
                    &compilation::coordinate_key(black_box(&coordinate)),
                ));
            }
        },
        |_| Counts {
            executed: 10000,
            ..Counts::default()
        },
    );
}

fn relation_audit() {
    for (name, relations, end) in [
        ("chain", 0, 1000),
        ("chain", 1, 1000),
        ("chain", 3, 1000),
        ("dense_small", 6, 8),
        ("star", 3, 1000),
    ] {
        let f = DomainField::<Numbers>::root().fields();
        let mut domain = domain!(Numbers { a.in_range(0..=end), b.in_range(0..=end), c.in_range(0..=end), d.in_range(0..=end) });
        let rules = if name == "star" {
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
        let constraints = 4 + relations;
        let shape = Shape {
            constraints,
            negative_pairs: constraints * (constraints - 1) / 2,
            budget: 256,
            ..Shape::default()
        };
        measure(
            "multi_negative",
            name,
            shape,
            || (),
            |_| {
                let mut cursor = 0;
                let mut occupied = BTreeSet::new();
                let mut witnesses = Vec::new();
                while let Some((input, coordinate)) =
                    compiled.next(2, &mut cursor, SEED, &occupied).unwrap()
                {
                    assert_eq!(
                        compiled.classify(&input).unwrap().violated,
                        coordinate.violated
                    );
                    assert_eq!(coordinate.violated.len(), 2);
                    occupied.insert(compilation::coordinate_key(&coordinate));
                    witnesses.push((input.to_domain_value(), coordinate));
                }
                witnesses
            },
            |w| Counts {
                executed: w.len(),
                multi: w.len(),
                ..Counts::default()
            },
        );
    }
}

fn main() {
    support::run(benchmark);
}
fn benchmark() {
    if !std::env::args().any(|v| v == "--measure") {
        return;
    }
    println!("operation,shape,budget,dimensions,cardinality,potential,constraints,negative_pairs,length,records,checks,sample,iterations,milliseconds,executed,positive,single_negative,multi_negative,stopped");
    if std::env::args().any(|v| v == "--audit") {
        relation_audit();
        seed_audit();
        return;
    }
    lazy();
    ranges_and_execution();
    domains_and_negatives();
    real_sources_and_stops();
    capture_integration();
}
