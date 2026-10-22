use obs::*;
use serde::Serialize;
use std::sync::{Arc, Barrier};
#[derive(Serialize, Observation)]
#[observation(namespace = "churn::event")]
struct Event {
    phase: usize,
    worker: usize,
}
#[test]
fn warm_workers_switch_each_configuration_generation_without_resetting_counter() {
    let mut certification = sun_certifications::certify(
        "warm workers switch each configuration generation without resetting counter",
    );
    let engine = ObservationEngine::new();
    let a = Arc::new(MemorySink::new());
    let b = Arc::new(MemorySink::new());
    let barrier = Barrier::new(9);
    let _scope = start_scope(ScopeConfig::new("churn").override_policy::<Event>(Policy::EveryN(3)));
    let context = Context::capture();
    engine.add_channel_sink("route", a.clone());
    std::thread::scope(|threads| {
        for worker in 0..8 {
            let context = context.clone();
            let engine = &engine;
            let barrier = &barrier;
            threads.spawn(move || {
                context.run(|| {
                    for phase in 0..7 {
                        barrier.wait();
                        for _ in 0..101 {
                            engine
                                .observe_if_active(|| Event { phase, worker })
                                .unwrap();
                        }
                        barrier.wait();
                    }
                })
            });
        }
        let mut counter = 0;
        for phase in 0..7 {
            match phase {
                1 => engine.set_selection(Selection::new().activate_all()),
                2 => engine.register_namespace_channel("churn", "route").unwrap(),
                3 => engine.add_channel_sink("route", b.clone()),
                4 => engine.clear_channel_sinks("route"),
                5 => {
                    engine.add_channel_sink("route", b.clone());
                    engine.set_selection(Selection::new());
                }
                6 => engine.set_selection(Selection::new().activate_all()),
                _ => (),
            }
            barrier.wait();
            barrier.wait();
            let selected = matches!(phase, 1 | 2 | 3 | 4 | 6);
            let before = counter / 3;
            if selected {
                counter += 808;
            }
            let n = counter / 3 - before;
            let count = |sink: &MemorySink| {
                sink.records()
                    .iter()
                    .filter(|r| r.data["phase"] == phase)
                    .count()
            };
            certification.check_fact_expect(
                "count(&a)",
                &(count(&a)),
                &(if matches!(phase, 2 | 3) { n } else { 0 }),
            );
            certification.check_fact_expect(
                "count(&b)",
                &(count(&b)),
                &(if matches!(phase, 3 | 6) { n } else { 0 }),
            );
        }
    });
    certification.assert_certify();
}
