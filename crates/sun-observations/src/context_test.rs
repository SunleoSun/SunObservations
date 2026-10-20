use super::*;
use crate::{start_scope, ScopeConfig};

#[test]
fn nested_scopes_restore_parent_and_then_empty_context() {
    assert_eq!(Context::capture().scope_name(), None);
    let outer = start_scope(ScopeConfig::new("outer"));
    assert_eq!(Context::capture().scope_name(), Some("outer"));
    {
        let _inner = start_scope(ScopeConfig::new("inner"));
        assert_eq!(Context::capture().scope_name(), Some("inner"));
    }
    assert_eq!(Context::capture().scope_name(), Some("outer"));
    drop(outer);
    assert_eq!(Context::capture().scope_name(), None);
}

#[test]
fn empty_context_masks_scope_and_unwind_restores_it() {
    let _scope = start_scope(ScopeConfig::new("parent"));
    let result = std::panic::catch_unwind(|| {
        Context::default().run(|| {
            assert_eq!(Context::capture().scope_name(), None);
            panic!("unwind");
        })
    });
    assert!(result.is_err());
    assert_eq!(Context::capture().scope_name(), Some("parent"));
}

#[test]
fn out_of_order_drop_does_not_resurrect_old_scope() {
    let outer = start_scope(ScopeConfig::new("outer"));
    let inner = start_scope(ScopeConfig::new("inner"));
    drop(outer);
    assert_eq!(Context::capture().scope_name(), Some("inner"));
    drop(inner);
    assert_eq!(Context::capture().scope_name(), None);
}

#[test]
fn retired_sink_destructors_run_without_tls_borrows() {
    use crate::{Observation, ObservationEngine, Selection, Sink};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    struct Probe(Arc<AtomicBool>);
    impl Sink for Probe {
        fn write(&self, _: &crate::ObservationRecord) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = Context::capture();
            self.0.store(true, Ordering::Relaxed);
        }
    }
    #[derive(serde::Serialize, Observation)]
    #[observation(namespace = "context_tests::retired")]
    struct Event;
    let engine = ObservationEngine::new();
    engine.set_selection(Selection::new().activate_all());
    let dropped = Arc::new(AtomicBool::new(false));
    engine.add_channel_sink("probe", Arc::new(Probe(dropped.clone())));
    engine
        .register_namespace_channel("context_tests", "probe")
        .unwrap();
    engine.observe_if_active(|| Event).unwrap();
    engine.clear_channel_sinks("probe");
    engine.observe_if_active(|| Event).unwrap();
    assert!(!dropped.load(Ordering::Relaxed));
    let _guard = Context::default().enter();
    assert!(dropped.load(Ordering::Relaxed));
}

#[test]
fn snapshot_can_outlive_original_guard() {
    let scope = start_scope(ScopeConfig::new("captured"));
    let context = Context::capture();
    drop(scope);
    context.run(|| assert_eq!(Context::capture().scope_name(), Some("captured")));
    assert_eq!(Context::capture().scope_name(), None);
}
