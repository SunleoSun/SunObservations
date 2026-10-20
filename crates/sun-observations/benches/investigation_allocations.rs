//! Allocation counts only. Kept in a separate executable so timed benches use System directly.
mod support;
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
use sun_observations::{ObservationLevel, ObservationRecord};
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
    count("writer_encode_reused_vec", || {
        bytes.clear();
        serde_json::to_writer(&mut bytes, &r).unwrap();
        black_box(&bytes);
    });
}
