use crate::{
    start_scope, JsonlSink, MemorySink, Observation, ObservationDelivery, ObservationEngine,
    ObservationRecord, Policy, ScopeConfig, Selection, Sink,
};
use parking_lot::Mutex;
use serde::{Serialize, Serializer};
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct Event {
    serializes: Arc<AtomicUsize>,
    renders: Arc<AtomicUsize>,
}
impl Serialize for Event {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serializes.fetch_add(1, Ordering::Relaxed);
        17u64.serialize(serializer)
    }
}
impl Observation for Event {
    const NAMESPACE: &'static str = "human::event";
    fn message(&self) -> Option<&str> {
        Some("semantic message")
    }
    fn render_text(&self) -> Option<String> {
        self.renders.fetch_add(1, Ordering::Relaxed);
        Some("THIS MUST NEVER ENTER STRUCTURED OUTPUT".into())
    }
}
#[derive(Default)]
struct Human {
    deliveries: Mutex<Vec<ObservationDelivery>>,
    capabilities: AtomicUsize,
}
impl Sink for Human {
    fn flush(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn write(&self, _: &ObservationRecord) -> io::Result<()> {
        panic!("human delivery required")
    }
    fn wants_render_text(&self) -> bool {
        self.capabilities.fetch_add(1, Ordering::Relaxed);
        true
    }
    fn write_delivery(&self, delivery: &ObservationDelivery) -> io::Result<()> {
        self.deliveries.lock().push(delivery.clone());
        Ok(())
    }
}
fn emit(engine: &ObservationEngine, counters: &[Arc<AtomicUsize>; 3]) -> bool {
    engine
        .observe_if_active(|| {
            counters[0].fetch_add(1, Ordering::Relaxed);
            Event {
                serializes: counters[1].clone(),
                renders: counters[2].clone(),
            }
        })
        .unwrap()
}

#[test]
fn exact_render_laziness_and_real_structured_sink_isolation() {
    let mut certification = sun_observations::certifications::certification(
        "exact render laziness and real structured sink isolation",
    );
    for (active, excluded, policy, memory, jsonl, humans) in [
        (false, false, Policy::Always, true, true, 1),
        (true, true, Policy::Always, true, true, 1),
        (true, false, Policy::Never, true, true, 1),
        (true, false, Policy::Always, false, false, 0),
        (true, false, Policy::Always, true, false, 0),
        (true, false, Policy::Always, false, true, 0),
        (true, false, Policy::Always, true, true, 0),
        (true, false, Policy::Always, false, false, 1),
        (true, false, Policy::Always, false, true, 1),
        (true, false, Policy::Always, false, false, 2),
        (true, false, Policy::Always, true, true, 2),
    ] {
        let engine = ObservationEngine::isolated();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("records.jsonl");
        let capture = Arc::new(MemorySink::new());
        if memory {
            engine.add_channel_sink("all", capture.clone());
        }
        if jsonl {
            engine.add_channel_sink("all", Arc::new(JsonlSink::open_append(&path).unwrap()));
        }
        let human: Vec<_> = (0..humans).map(|_| Arc::new(Human::default())).collect();
        for sink in &human {
            engine.add_channel_sink("all", sink.clone());
        }
        engine.register_namespace_channel("human", "all").unwrap();
        let mut selection = Selection::new();
        if active {
            selection = selection.activate_all();
        }
        if excluded {
            selection = selection.exclude_namespace("human").unwrap();
        }
        engine.set_selection(selection);
        let _scope = start_scope(ScopeConfig::new("laziness").override_policy::<Event>(policy));
        let counters = std::array::from_fn(|_| Arc::new(AtomicUsize::new(0)));
        let admitted = active && !excluded && policy != Policy::Never;
        certification.check_fact_expect(
            "emit(&engine, &counters)",
            &(emit(&engine, &counters)),
            &(admitted),
        );
        engine.flush_channels().unwrap();
        certification.check_fact_expect(
            "counters[0].load(Ordering::Relaxed)",
            &(counters[0].load(Ordering::Relaxed)),
            &(usize::from(admitted)),
        );
        certification.check_fact_expect(
            "counters[1].load(Ordering::Relaxed)",
            &(counters[1].load(Ordering::Relaxed)),
            &(usize::from(admitted)),
        );
        certification.check_fact_expect(
            "counters[2].load(Ordering::Relaxed)",
            &(counters[2].load(Ordering::Relaxed)),
            &(usize::from(admitted && humans > 0)),
        );
        if admitted && memory {
            let record = &capture.records()[0];
            certification.check_fact_expect(
                "record.data",
                &(record.data),
                &(serde_json::json!(17)),
            );
            certification.check_fact_expect(
                "record.message.as_deref()",
                &(record.message.as_deref()),
                &(Some("semantic message")),
            );
            certification.check_fact_expect(
                "serde_json::to_string(record).unwrap().contains(\"THIS MUST\")",
                serde_json::to_string(record).unwrap().contains("THIS MUST"),
                false,
            );
        }
        if admitted && jsonl {
            let text = std::fs::read_to_string(&path).unwrap();
            certification.check_fact_expect(
                "text.contains(\"THIS MUST\")",
                text.contains("THIS MUST"),
                false,
            );
            let record: ObservationRecord = serde_json::from_str(text.trim()).unwrap();
            certification.check_fact_expect(
                "record.data",
                &(record.data),
                &(serde_json::json!(17)),
            );
            certification.check_fact_expect(
                "record.message.as_deref()",
                &(record.message.as_deref()),
                &(Some("semantic message")),
            );
        }
        if admitted && humans > 0 {
            let first = human[0].deliveries.lock()[0].clone();
            certification.check_fact_expect(
                "first.render_text()",
                &(first.render_text()),
                &(Some("THIS MUST NEVER ENTER STRUCTURED OUTPUT")),
            );
            for sink in &human {
                let deliveries = sink.deliveries.lock();
                certification.check_fact_expect(
                    "Arc::ptr_eq( first.shared_record(), deliveries[0].shared_record() )",
                    Arc::ptr_eq(first.shared_record(), deliveries[0].shared_record()),
                    true,
                );
                certification.check_fact_expect(
                    "rendering shared across human sinks",
                    Arc::ptr_eq(
                        first.shared_render_text().unwrap(),
                        deliveries[0].shared_render_text().unwrap(),
                    ),
                    true,
                );
            }
            if memory {
                certification.check_fact_expect(
                    "Arc::ptr_eq( first.shared_record(), &capture.shared_records()[0] )",
                    Arc::ptr_eq(first.shared_record(), &capture.shared_records()[0]),
                    true,
                );
            }
        }
    }
    certification.assert_certify();
}

#[test]
fn serialization_failure_never_renders_or_delivers_and_spans_never_render() {
    let mut certification = sun_observations::certifications::certification(
        "serialization failure never renders or delivers and spans never render",
    );
    struct Bad(Arc<AtomicUsize>);
    impl Serialize for Bad {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("invalid payload"))
        }
    }
    impl Observation for Bad {
        const NAMESPACE: &'static str = "human::bad";
        fn render_text(&self) -> Option<String> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Some("must not render".into())
        }
    }
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("human", "all").unwrap();
    let human = Arc::new(Human::default());
    engine.add_channel_sink("all", human.clone());
    let renders = Arc::new(AtomicUsize::new(0));
    certification.check_fact_expect(
        "engine.observe_if_active(|| Bad(renders.clone())).is_err()",
        engine.observe_if_active(|| Bad(renders.clone())).is_err(),
        true,
    );
    certification.check_fact_expect(
        "renders.load(Ordering::Relaxed)",
        &(renders.load(Ordering::Relaxed)),
        &(0),
    );
    certification.check_fact_expect(
        "human.deliveries.lock().is_empty()",
        human.deliveries.lock().is_empty(),
        true,
    );
    let span = engine
        .span_if_active(|| Event {
            serializes: Arc::new(AtomicUsize::new(0)),
            renders: renders.clone(),
        })
        .unwrap();
    engine.clear_channel_sinks("all");
    span.finish().unwrap();
    certification.check_fact_expect(
        "renders.load(Ordering::Relaxed)",
        &(renders.load(Ordering::Relaxed)),
        &(0),
    );
    let deliveries = human.deliveries.lock();
    certification.check_fact_expect("deliveries.len()", &(deliveries.len()), &(2));
    certification.check_fact_expect(
        "deliveries[0].render_text().is_none()",
        deliveries[0].render_text().is_none(),
        true,
    );
    certification.check_fact_expect(
        "deliveries[1].render_text().is_none()",
        deliveries[1].render_text().is_none(),
        true,
    );
    certification.check_fact_expect(
        "deliveries[1].record().data",
        &(deliveries[1].record().data),
        &(serde_json::Value::Null),
    );
    certification.assert_certify();
}

#[test]
fn attachment_capability_can_reenter_configuration_without_engine_or_scope_locks() {
    struct Reentrant(std::sync::Weak<ObservationEngine>);
    impl Sink for Reentrant {
        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }
        fn write(&self, _: &ObservationRecord) -> io::Result<()> {
            Ok(())
        }
        fn wants_render_text(&self) -> bool {
            let engine = self.0.upgrade().unwrap();
            engine.set_selection(Selection::new().activate_all());
            engine
                .observe_if_active(|| Event {
                    serializes: Arc::new(AtomicUsize::new(0)),
                    renders: Arc::new(AtomicUsize::new(0)),
                })
                .unwrap();
            true
        }
    }
    let engine = Arc::new(ObservationEngine::isolated());
    let sink = Arc::new(Reentrant(Arc::downgrade(&engine)));
    let (done, observed) = std::sync::mpsc::channel();
    let child = std::thread::spawn(move || {
        engine.add_channel_sink("human", sink);
        done.send(()).unwrap();
    });
    observed
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    child.join().unwrap();
}

#[test]
fn human_capability_is_cold_and_route_mutations_preserve_counting_policy() {
    let mut certification = sun_observations::certifications::certification(
        "human capability is cold and route mutations preserve counting policy",
    );
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("structured", memory.clone());
    engine
        .register_namespace_channel("human", "structured")
        .unwrap();
    let _scope = start_scope(
        ScopeConfig::new("mutation").override_policy::<Event>(Policy::every_n(2).unwrap()),
    );
    let counters = std::array::from_fn(|_| Arc::new(AtomicUsize::new(0)));
    let human = Arc::new(Human::default());
    certification.check_fact_expect("emit(&engine, &counters)", emit(&engine, &counters), false);
    engine.add_channel_sink("human", human.clone());
    certification.check_fact_expect("emit(&engine, &counters)", emit(&engine, &counters), true);
    certification.check_fact_expect(
        "counters[2].load(Ordering::Relaxed)",
        &(counters[2].load(Ordering::Relaxed)),
        &(0),
    );
    engine.register_namespace_channel("human", "human").unwrap();
    certification.check_fact_expect("emit(&engine, &counters)", emit(&engine, &counters), false);
    for _ in 0..5 {
        certification.check_fact_expect("emit(&engine, &counters)", emit(&engine, &counters), true);
        certification.check_fact_expect(
            "emit(&engine, &counters)",
            emit(&engine, &counters),
            false,
        );
    }
    certification.check_fact_expect(
        "human.capabilities.load(Ordering::Relaxed)",
        &(human.capabilities.load(Ordering::Relaxed)),
        &(1),
    );
    certification.check_fact_expect(
        "counters[2].load(Ordering::Relaxed)",
        &(counters[2].load(Ordering::Relaxed)),
        &(5),
    );
    engine.clear_channel_sinks("human");
    certification.check_fact_expect("emit(&engine, &counters)", emit(&engine, &counters), true);
    certification.check_fact_expect(
        "counters[2].load(Ordering::Relaxed)",
        &(counters[2].load(Ordering::Relaxed)),
        &(5),
    );
    engine.add_channel_sink("human", human.clone());
    certification.check_fact_expect("emit(&engine, &counters)", emit(&engine, &counters), false);
    certification.check_fact_expect("emit(&engine, &counters)", emit(&engine, &counters), true);
    certification.check_fact_expect(
        "counters[2].load(Ordering::Relaxed)",
        &(counters[2].load(Ordering::Relaxed)),
        &(6),
    );
    certification.check_fact_expect("memory.len()", &(memory.len()), &(8));
    certification.assert_certify();
}
