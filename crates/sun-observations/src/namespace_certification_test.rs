use crate::{MemorySink, Observation, ObservationEngine, Selection};
use serde::Serialize;
use std::sync::Arc;
#[derive(Serialize, Observation)]
#[observation(namespace = "compute::train")]
struct Renamed;
#[derive(Serialize, Observation)]
#[observation(namespace = "compute::train::epoch")]
struct Child;
#[derive(Serialize, Observation)]
#[observation(namespace = "compute::trainer")]
struct Sibling;
#[test]
fn explicit_identity_selection_and_routes_share_segment_boundaries() {
    let engine = ObservationEngine::new();
    let sink = Arc::new(MemorySink::new());
    engine.add_channel_sink("train", sink.clone());
    engine
        .register_namespace_channel("compute::train", "train")
        .unwrap();
    let generation = engine.generation();
    let invalid = Arc::new(MemorySink::new());
    engine.add_channel_sink("invalid", invalid.clone());
    let attached_generation = engine.generation();
    assert!(attached_generation > generation);
    assert!(engine
        .register_namespace_channel("compute::train::", "invalid")
        .is_err());
    assert_eq!(engine.generation(), attached_generation);
    engine.set_selection(Selection::new().activate_namespace("compute::train"));
    assert!(engine.observe_if_active(|| Renamed).unwrap());
    assert!(engine.observe_if_active(|| Child).unwrap());
    assert!(!engine
        .observe_if_active::<Sibling, _>(|| panic!("lazy"))
        .unwrap());
    engine.set_selection(Selection::new().activate_all());
    assert!(engine.observe_if_active(|| Sibling).unwrap());
    let records = sink.records();
    assert_eq!(records.len(), 2);
    assert!(invalid.is_empty());
    assert_eq!(records[0].namespace, "compute::train");
    assert_eq!(records[1].namespace, "compute::train::epoch");
}
