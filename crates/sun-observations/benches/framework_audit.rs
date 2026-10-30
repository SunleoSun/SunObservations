mod support;
use serde::{Deserialize, Serialize};
use std::{hint::black_box, time::Instant};
use sun_observations::{certifications::*, evidence::*, *};

const SEED: u64 = 184729361;
const NAMES: [&str; 32] = [
    "audit::t00",
    "audit::t01",
    "audit::t02",
    "audit::t03",
    "audit::t04",
    "audit::t05",
    "audit::t06",
    "audit::t07",
    "audit::t08",
    "audit::t09",
    "audit::t10",
    "audit::t11",
    "audit::t12",
    "audit::t13",
    "audit::t14",
    "audit::t15",
    "audit::t16",
    "audit::t17",
    "audit::t18",
    "audit::t19",
    "audit::t20",
    "audit::t21",
    "audit::t22",
    "audit::t23",
    "audit::t24",
    "audit::t25",
    "audit::t26",
    "audit::t27",
    "audit::t28",
    "audit::t29",
    "audit::t30",
    "audit::t31",
];
#[derive(Serialize, Deserialize)]
struct Event<const TYPE: usize> {
    row: usize,
}
impl<const TYPE: usize> Observation for Event<TYPE> {
    const NAMESPACE: &'static str = NAMES[TYPE];
}
fn emit(row: usize, types: usize) -> bool {
    macro_rules! dispatch { ($($id:literal),*) => { match row % types { $($id => try_observe!(Event::<$id> { row }).unwrap(),)* _ => unreachable!() } }; }
    dispatch!(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31
    )
}
fn measure<S, R>(
    operation: &str,
    size: usize,
    width: usize,
    iterations: usize,
    mut setup: impl FnMut() -> S,
    mut run: impl FnMut(S) -> R,
    inspect: impl Fn(&R) -> usize,
) {
    let warm = run(setup());
    let expected = inspect(&warm);
    drop(warm);
    for sample in 0..3 {
        let mut elapsed = 0.;
        for _ in 0..iterations {
            let state = setup();
            let start = Instant::now();
            let output = run(black_box(state));
            elapsed += start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(inspect(&output), expected);
            black_box(output);
        }
        println!(
            "{operation},{size},{width},{sample},{iterations},{:.6},{expected}",
            elapsed / iterations as f64
        );
    }
}
fn emission() {
    for active in [false, true] {
        for types in [1, 4, 8, 16, 32] {
            let scope = ScopeConfig::new("many types");
            let _guard = start_scope(if active { scope.activate_all() } else { scope });
            for row in 0..types {
                assert_eq!(emit(row, types), active);
            }
            measure(
                if active {
                    "sinkless_types"
                } else {
                    "inactive_types"
                },
                1_000_000,
                types,
                3,
                || (),
                |_| {
                    for row in 0..1_000_000 {
                        black_box(emit(row, types));
                    }
                },
                |_| 1_000_000,
            );
        }
    }
}
fn captures() {
    for n in [100usize, 10_000, 100_000, 1_000_000] {
        for types in [1, 8, 32] {
            let config = CaptureConfig::new().activate_all();
            measure(
                "capture_many_types",
                n,
                types,
                if n == 100 {
                    1000
                } else if n == 10_000 {
                    10
                } else {
                    1
                },
                || config.clone(),
                |config| {
                    capture_observations(config, || {
                        for row in 0..n {
                            assert!(emit(row, types));
                        }
                    })
                },
                |run| {
                    assert!(run.status().is_complete());
                    assert_eq!(run.dataset().records().len(), n);
                    n
                },
            );
            let run = capture_observations(config, || {
                for row in 0..n {
                    emit(row, types);
                }
            });
            for repeats in [1, 16] {
                measure(
                    "capture_and_query",
                    n,
                    types * repeats,
                    if n == 100 {
                        1000
                    } else if n == 10_000 {
                        10
                    } else {
                        1
                    },
                    || (),
                    |_| {
                        let run = capture_observations(CaptureConfig::new().activate_all(), || {
                            for row in 0..n {
                                emit(row, types);
                            }
                        });
                        for _ in 0..repeats {
                            assert_eq!(
                                black_box(
                                    run.dataset()
                                        .query(
                                            ObservationsQuery::new().namespace(NAMES[0]).points()
                                        )
                                        .iter()
                                        .count()
                                ),
                                n.div_ceil(types)
                            );
                        }
                        run
                    },
                    |run| {
                        assert!(run.status().is_complete());
                        n
                    },
                );
            }
            measure(
                "query_only",
                n,
                types,
                if n == 100 {
                    10000
                } else if n == 10_000 {
                    100
                } else {
                    3
                },
                || (),
                |_| {
                    black_box(
                        run.dataset()
                            .query(ObservationsQuery::new().namespace(NAMES[0]).points())
                            .iter()
                            .count(),
                    )
                },
                |count| {
                    assert_eq!(*count, n.div_ceil(types));
                    *count
                },
            );
            measure(
                "decode_only",
                n,
                types,
                if n == 100 {
                    10000
                } else if n == 10_000 {
                    100
                } else {
                    3
                },
                || (),
                |_| run.to_typed::<Event<0>>().unwrap(),
                |view| {
                    assert_eq!(view.values().len(), n.div_ceil(types));
                    view.values().len()
                },
            );
        }
    }
}
#[derive(Clone, DomainInput)]
struct Scalar {
    value: i64,
}
fn combined() {
    for (name, work, observations) in [
        ("combined_cheap", 0, 0),
        ("combined_moderate", 4096, 0),
        ("combined_capture", 0, 100),
    ] {
        measure(
            name,
            64,
            observations,
            50,
            || (),
            |_| {
                let mut c =
                    certification("combined audit").executor_with_seed(move |i: &Scalar, seed| {
                        capture_observations(CaptureConfig::new().activate_all(), || {
                            if !(0..=1000).contains(&i.value) {
                                return Err(());
                            }
                            let mut state = seed;
                            for _ in 0..black_box(work) {
                                state = black_box(state)
                                    .wrapping_mul(6364136223846793005)
                                    .wrapping_add(1);
                            }
                            for row in 0..observations {
                                try_observe!(Event::<0> { row }).unwrap();
                            }
                            black_box(state);
                            Ok(i.value)
                        })
                    });
                c.domain(domain!(Scalar { value.in_range(0..=1000) }));
                c.seed(SEED).cases_budget(64).report_mode(false);
                let case = c.case();
                c.check(fact!(*case.result()).expect(fact!(case.input().value)));
                c.report()
            },
            |r| {
                assert!(r.is_pass(), "{r}");
                r.cases_executed()
            },
        );
    }
}
fn concurrent_capture() {
    for n in [100usize, 100_000] {
        for threads in [1, 2, 4, 8] {
            measure(
                "concurrent_capture",
                n,
                threads,
                1,
                || (),
                |_| {
                    capture_observations(CaptureConfig::new().activate_all(), || {
                        let workers: Vec<_> = (0..threads)
                            .map(|thread| {
                                context::thread::spawn(move || {
                                    for row in (thread..n).step_by(threads) {
                                        assert!(emit(row, 1));
                                    }
                                })
                            })
                            .collect();
                        for worker in workers {
                            worker.join().unwrap();
                        }
                    })
                },
                |run| {
                    assert!(run.status().is_complete());
                    let typed = run.to_typed::<Event<0>>().unwrap();
                    let mut rows: Vec<_> = typed.values().iter().map(|e| e.row).collect();
                    rows.sort_unstable();
                    assert_eq!(rows, (0..n).collect::<Vec<_>>());
                    n
                },
            );
        }
    }
}

fn lag_scaling() {
    for n in [64usize, 1024, 16384, 32768, 65536, 262144] {
        let values: Vec<_> = (0..n)
            .map(|i| (i as i64 * 2, ((i * 17 % 991) as f64).sin()))
            .collect();
        let config = LagConfig {
            range: LagRange::new(-32, 32).unwrap(),
            measure: LagMeasure::Pearson,
        };
        let reference = lag_relation_by(&values, &values, |v| *v, |v| *v, config).unwrap();
        for threads in [1, 2, 4, 8] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            measure(
                "lag_threads",
                n,
                threads,
                2,
                || (),
                |_| {
                    pool.install(|| {
                        lag_relation_by(&values, &values, |v| *v, |v| *v, config).unwrap()
                    })
                },
                |report| {
                    assert_eq!(report, &reference);
                    report.candidates.len()
                },
            );
        }
    }
}
#[no_mangle]
#[inline(never)]
pub fn audit_finite_count(values: &[f64]) -> usize {
    NumericSeries::new(values.iter().copied())
        .finite_count()
        .finite_count
}
fn numeric_scan() {
    for n in [64usize, 4096, 1_000_000] {
        let values: Vec<_> = (0..n)
            .map(|i| if i % 97 == 0 { f64::INFINITY } else { i as f64 })
            .collect();
        let expected = values.iter().filter(|v| v.is_finite()).count();
        for candidate in [false, true] {
            measure(
                if candidate {
                    "finite_simple"
                } else {
                    "finite_production"
                },
                n,
                1,
                100,
                || (),
                |_| {
                    if candidate {
                        black_box(values.as_slice())
                            .iter()
                            .filter(|v| v.is_finite())
                            .count()
                    } else {
                        audit_finite_count(black_box(&values))
                    }
                },
                |count| {
                    assert_eq!(*count, expected);
                    *count
                },
            );
        }
    }
}

fn benchmark() {
    println!("operation,size,width,sample,iterations,milliseconds,observed");
    let group = std::env::var("SUN_AUDIT_GROUP").unwrap_or_default();
    if group.is_empty() || group == "emission" {
        emission();
    }
    if group.is_empty() || group == "capture" {
        captures();
    }
    if group.is_empty() || group == "combined" {
        combined();
    }
    if group.is_empty() || group == "threads" {
        concurrent_capture();
    }
    if group.is_empty() || group == "lag" {
        lag_scaling();
    }
    if group.is_empty() || group == "simd" {
        numeric_scan();
    }
}
fn main() {
    if std::env::args().any(|v| v == "--measure") {
        support::run(benchmark);
    }
}
