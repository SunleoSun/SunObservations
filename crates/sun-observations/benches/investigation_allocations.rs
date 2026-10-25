//! Allocation counts only. Kept in a separate executable so timed benches use System directly.
extern crate self as sun_observations;
extern crate sun_observations as runtime;
pub use runtime::*;
#[allow(dead_code, unused_imports)]
#[path = "../src/certifications/mod.rs"]
pub mod certifications;
mod support;
use certifications::*;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
        Arc,
    },
};
struct Counter;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOC: AtomicUsize = AtomicUsize::new(0);
static REALLOC: AtomicUsize = AtomicUsize::new(0);
static FREE: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            ALLOC.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ENABLED.load(Relaxed) {
            FREE.fetch_add(1, Relaxed);
        }
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.load(Relaxed) {
            REALLOC.fetch_add(1, Relaxed);
            BYTES.fetch_add(size, Relaxed);
        }
        System.realloc(ptr, layout, size)
    }
}
#[global_allocator]
static COUNTER: Counter = Counter;
#[derive(Serialize)]
struct Event {
    index: u64,
    value: f64,
}
fn record() -> ObservationRecord {
    ObservationRecord {
        ts: "2026-10-20T12:34:56.789Z".into(),
        proc: "01234567-89ab-4cde-8123-456789abcdef".into(),
        proc_ms: 1234,
        scope: Some("benchmark".into()),
        namespace: "bench::event".into(),
        level: ObservationLevel::INFO,
        data: serde_json::to_value(Event {
            index: 123456,
            value: 0.5,
        })
        .unwrap(),
        message: None,
        phase: None,
        span: None,
    }
}
fn count(name: &str, mut f: impl FnMut()) {
    f();
    ALLOC.store(0, Relaxed);
    REALLOC.store(0, Relaxed);
    FREE.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    f();
    ENABLED.store(false, Relaxed);
    println!(
        "{name},{},{},{},{}",
        ALLOC.load(Relaxed),
        REALLOC.load(Relaxed),
        FREE.load(Relaxed),
        BYTES.load(Relaxed)
    );
}
fn main() {
    support::run(benchmark);
}

#[derive(Clone, DomainInput)]
struct Numbers {
    a: i64,
    b: i64,
    c: i64,
    d: i64,
}

#[derive(Serialize)]
struct CapturedEvent<const TYPE: usize> {
    row: usize,
}
impl<const TYPE: usize> Observation for CapturedEvent<TYPE> {
    const NAMESPACE: &'static str = [
        "allocation::a",
        "allocation::b",
        "allocation::c",
        "allocation::d",
        "allocation::e",
        "allocation::f",
        "allocation::g",
        "allocation::h",
    ][TYPE];
}
fn capture_allocations(queries: usize) {
    let run = runtime::evidence::capture_observations(
        runtime::evidence::CaptureConfig::new().activate_all(),
        || {
            for row in 0..10000 {
                macro_rules! emit { ($($id:literal),*) => { match row % 8 { $($id => runtime::observe!(CapturedEvent::<$id> { row }).unwrap(),)* _ => unreachable!() } }; }
                assert!(emit!(0, 1, 2, 3, 4, 5, 6, 7));
            }
        },
    );
    assert!(run.status().is_complete());
    assert_eq!(run.dataset().records().len(), 10000);
    for _ in 0..queries {
        assert_eq!(
            run.dataset()
                .query(
                    runtime::evidence::ObservationsQuery::new()
                        .namespace(CapturedEvent::<0>::NAMESPACE)
                )
                .iter()
                .count(),
            1250
        );
    }
    black_box(run);
}

fn benchmark() {
    let p = Event {
        index: 123456,
        value: 0.5,
    };
    let r = record();
    let mut bytes = Vec::with_capacity(4096);
    println!("case,allocations,reallocations,deallocations,requested_bytes");
    count("payload_to_value", || {
        black_box(serde_json::to_value(&p).unwrap());
    });
    count("payload_to_raw", || {
        black_box(serde_json::value::to_raw_value(&p).unwrap());
    });
    count("timestamp_rfc3339", || {
        black_box(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
    });
    count("envelope_strings", || {
        black_box((
            r.ts.clone(),
            r.proc.clone(),
            r.scope.clone(),
            r.namespace.clone(),
        ));
    });
    count("record_clone", || {
        black_box(r.clone());
    });
    count("record_arc", || {
        black_box(Arc::new(record()));
    });
    for relations in [0, 1, 3] {
        let f = DomainField::<Numbers>::root().fields();
        let mut domain = domain!(Numbers { a.in_range(0..=1000), b.in_range(0..=1000), c.in_range(0..=1000), d.in_range(0..=1000) });
        for rule in [
            f.a.less_equal(&f.b),
            f.b.less_equal(&f.c),
            f.c.less_equal(&f.d),
        ]
        .into_iter()
        .take(relations)
        {
            domain.add(rule, &[]);
        }
        let compiled = domain.compile().unwrap();
        count(&format!("multi_negative_{}", 4 + relations), || {
            let mut cursor = 0;
            let mut occupied = std::collections::BTreeSet::new();
            while let Some((input, coordinate)) =
                compiled.next(2, &mut cursor, 184729361, &occupied).unwrap()
            {
                black_box(input);
                occupied.insert(compilation::coordinate_key(&coordinate));
            }
        });
    }
    count("capture_8_types_10000", || capture_allocations(0));
    count("capture_8_types_16_queries_10000", || {
        capture_allocations(16)
    });
    count("writer_encode_reused_vec", || {
        bytes.clear();
        serde_json::to_writer(&mut bytes, &r).unwrap();
        black_box(&bytes);
    });
}
