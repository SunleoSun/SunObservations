use super::*;
use crate::support_test::record;
use std::sync::Arc;

#[test]
fn concurrent_writes_and_independent_snapshots() {
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
    assert_eq!(memory.len(), 800);
    let mut snapshot = memory.records();
    snapshot.clear();
    assert_eq!(memory.len(), 800);
    assert_eq!(memory.drain().len(), 800);
    assert!(memory.is_empty());
}
