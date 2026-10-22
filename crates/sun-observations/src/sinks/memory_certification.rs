use super::*;
use crate::test_support::record;
use sun_certifications::*;
#[test]
fn bounded_oldest_batch_retention_preserves_identity_order_and_lifetime_stats() {
    let memory = MemorySink::with_config(MemorySinkConfig::new(10, 0.3).unwrap());
    let mut c = certify("bounded memory history");
    for i in 0..30 {
        let mut r = record("memory::event");
        r.proc_ms = i;
        memory.write(&r).unwrap();
        c.check_fact_expect("capacity after write", memory.len() <= 10, true);
        if i == 8 {
            c.check_fact_expect(
                "below limit",
                memory.stats(),
                MemoryStats {
                    retained: 9,
                    evicted: 0,
                    compactions: 0,
                },
            );
        }
    }
    let snapshot = memory.shared_records();
    c.check_fact_expect(
        "retention",
        memory.stats(),
        MemoryStats {
            retained: 9,
            evicted: 21,
            compactions: 7,
        },
    );
    c.check_fact_expect(
        "newest order",
        snapshot.iter().map(|v| v.proc_ms).collect::<Vec<_>>(),
        (21..30).collect::<Vec<_>>(),
    );
    let drained = memory.drain_shared();
    c.check_fact_expect(
        "snapshot identity",
        Arc::ptr_eq(&snapshot[0], &drained[0]),
        true,
    );
    c.check_fact_expect("drained membership", memory.len(), 0usize);
    c.check_fact_expect("durable eviction stats", memory.stats().evicted, 21u64);
    c.check_fact_expect("snapshot membership", snapshot.len(), 9usize);
    c.assert_certify();
}
#[test]
fn concurrent_retention_is_bounded_with_exact_accounting() {
    let memory = MemorySink::with_config(MemorySinkConfig::new(127, 0.1).unwrap());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let memory = &memory;
            scope.spawn(move || {
                for _ in 0..1000 {
                    memory.write(&record("memory::event")).unwrap();
                }
            });
        }
    });
    let stats = memory.stats();
    certify("concurrent bounded retention")
        .check_fact_expect("bound", stats.retained <= 127, true)
        .check_fact_expect("accounting", stats.retained as u64 + stats.evicted, 8000u64)
        .check_fact_expect("batch accounting", stats.evicted, stats.compactions * 13)
        .check_fact_expect("snapshot", memory.records().len(), stats.retained)
        .assert_certify();
}
#[test]
fn retention_configuration_rejects_invalid_bounds() {
    let mut c = certify("validated retention");
    for (max, fraction) in [
        (0, 0.1),
        (1, 0.0),
        (1, -0.1),
        (1, 1.1),
        (1, f64::NAN),
        (1, f64::INFINITY),
    ] {
        c.check_fact_expect(
            "invalid configuration",
            MemorySinkConfig::new(max, fraction).is_err(),
            true,
        );
    }
    c.check_fact_expect(
        "finite default",
        MemorySinkConfig::default().max_items(),
        1_000_000usize,
    )
    .assert_certify();
}

mod snapshots {
    use super::super::*;
    use crate::test_support::record;
    use std::sync::Arc;

    #[test]
    fn concurrent_writes_and_independent_snapshots() {
        let mut certification =
            sun_certifications::certify("concurrent writes and independent snapshots");
        let memory = Arc::new(MemorySink::new());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let memory = memory.clone();
                scope.spawn(move || {
                    for _ in 0..100 {
                        memory.write(&record("memory::event")).unwrap();
                    }
                });
            }
        });
        certification.check_fact_expect("memory.len()", &(memory.len()), &(800));
        let mut snapshot = memory.records();
        snapshot.clear();
        certification.check_fact_expect("memory.len()", &(memory.len()), &(800));
        certification.check_fact_expect("memory.drain().len()", &(memory.drain().len()), &(800));
        certification.check_fact_expect("memory.is_empty()", memory.is_empty(), true);
        certification.assert_certify();
    }
}
