use crate::{ObservationRecord, Sink};
use parking_lot::Mutex;
use std::{collections::VecDeque, io, sync::Arc};

/// Validated bounded history limits. Compaction evicts an oldest batch at capacity.
#[derive(Clone, Copy, Debug)]
pub struct MemorySinkConfig {
    max_items: usize,
    compact_fraction: f64,
}
impl Default for MemorySinkConfig {
    fn default() -> Self {
        Self {
            max_items: 1_000_000,
            compact_fraction: 0.10,
        }
    }
}
impl MemorySinkConfig {
    pub fn new(max_items: usize, compact_fraction: f64) -> io::Result<Self> {
        if max_items == 0
            || !compact_fraction.is_finite()
            || !(0.0..=1.0).contains(&compact_fraction)
            || compact_fraction == 0.0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "positive capacity and finite compaction fraction in (0,1] required",
            ));
        }
        Ok(Self {
            max_items,
            compact_fraction,
        })
    }
    pub fn max_items(self) -> usize {
        self.max_items
    }
    pub fn compact_fraction(self) -> f64 {
        self.compact_fraction
    }
}
/// Lifetime retention counters; draining does not erase evidence-loss history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryStats {
    pub retained: usize,
    pub evicted: u64,
    pub compactions: u64,
}
#[derive(Default)]
struct History {
    records: VecDeque<Arc<ObservationRecord>>,
    evicted: u64,
    compactions: u64,
}
/// Synchronous bounded immutable record history in delivery order.
pub struct MemorySink {
    config: MemorySinkConfig,
    history: Mutex<History>,
}
impl Default for MemorySink {
    fn default() -> Self {
        Self::with_config(MemorySinkConfig::default())
    }
}
impl MemorySink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_config(config: MemorySinkConfig) -> Self {
        Self {
            config,
            history: Mutex::new(History::default()),
        }
    }
    pub fn config(&self) -> MemorySinkConfig {
        self.config
    }
    pub fn stats(&self) -> MemoryStats {
        let h = self.history.lock();
        MemoryStats {
            retained: h.records.len(),
            evicted: h.evicted,
            compactions: h.compactions,
        }
    }
    pub fn records(&self) -> Vec<ObservationRecord> {
        self.history
            .lock()
            .records
            .iter()
            .map(|r| (**r).clone())
            .collect()
    }
    pub fn shared_records(&self) -> Vec<Arc<ObservationRecord>> {
        self.history.lock().records.iter().cloned().collect()
    }
    pub fn drain(&self) -> Vec<ObservationRecord> {
        self.drain_shared()
            .into_iter()
            .map(Arc::unwrap_or_clone)
            .collect()
    }
    pub fn drain_shared(&self) -> Vec<Arc<ObservationRecord>> {
        self.history.lock().records.drain(..).collect()
    }
    pub fn len(&self) -> usize {
        self.history.lock().records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl Sink for MemorySink {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.write_shared(Arc::new(record.clone()))
    }
    fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()> {
        let mut h = self.history.lock();
        if h.records.len() == self.config.max_items {
            let batch = ((self.config.max_items as f64 * self.config.compact_fraction).ceil()
                as usize)
                .clamp(1, self.config.max_items);
            h.records.drain(..batch);
            h.evicted += batch as u64;
            h.compactions += 1;
        }
        h.records.push_back(record);
        Ok(())
    }
}
#[cfg(test)]
#[path = "memory_certification.rs"]
mod certification;
