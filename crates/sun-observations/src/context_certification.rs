use super::*;
use crate::{start_scope, ScopeConfig};

#[test]
fn nested_scopes_restore_parent_and_then_empty_context() {
    let mut certification = sun_observations::certifications::certify(
        "nested scopes restore parent and then empty context",
    );
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(None),
    );
    let outer = start_scope(ScopeConfig::new("outer"));
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(Some("outer")),
    );
    {
        let _inner = start_scope(ScopeConfig::new("inner"));
        certification.check_fact_expect(
            "Context::capture().scope_name()",
            &(Context::capture().scope_name()),
            &(Some("inner")),
        );
    }
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(Some("outer")),
    );
    drop(outer);
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(None),
    );
    certification.assert_certify();
}

#[test]
fn empty_context_masks_scope_and_unwind_restores_it() {
    let mut certification = sun_observations::certifications::certify(
        "empty context masks scope and unwind restores it",
    );
    let _scope = start_scope(ScopeConfig::new("parent"));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Context::default().run(|| {
            certification.check_fact_expect(
                "Context::capture().scope_name()",
                &(Context::capture().scope_name()),
                &(None),
            );
            panic!("unwind");
        })
    }));
    certification.check_fact_expect("result.is_err()", result.is_err(), true);
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(Some("parent")),
    );
    certification.assert_certify();
}

#[test]
fn out_of_order_drop_does_not_resurrect_old_scope() {
    let mut certification =
        sun_observations::certifications::certify("out of order drop does not resurrect old scope");
    let outer = start_scope(ScopeConfig::new("outer"));
    let inner = start_scope(ScopeConfig::new("inner"));
    drop(outer);
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(Some("inner")),
    );
    drop(inner);
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(None),
    );
    certification.assert_certify();
}

#[test]
fn retired_sink_destructors_run_without_tls_borrows() {
    let mut certification = sun_observations::certifications::certify(
        "retired sink destructors run without tls borrows",
    );
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
    certification.check_fact_expect(
        "dropped.load(Ordering::Relaxed)",
        dropped.load(Ordering::Relaxed),
        false,
    );
    let _guard = Context::default().enter();
    certification.check_fact_expect(
        "dropped.load(Ordering::Relaxed)",
        dropped.load(Ordering::Relaxed),
        true,
    );
    certification.assert_certify();
}

#[test]
fn snapshot_can_outlive_original_guard() {
    let mut certification =
        sun_observations::certifications::certify("snapshot can outlive original guard");
    let scope = start_scope(ScopeConfig::new("captured"));
    let context = Context::capture();
    drop(scope);
    context.run(|| {
        certification.check_fact_expect(
            "Context::capture().scope_name()",
            &(Context::capture().scope_name()),
            &(Some("captured")),
        );
    });
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(None),
    );
    certification.assert_certify();
}
