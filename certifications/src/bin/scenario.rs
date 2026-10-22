use serde::Serialize;
use std::{
    path::Path,
    sync::{Arc, Barrier},
};
use sun_observations::certifications::{certification, fact, CountRule};
use sun_observations::*;
use sun_observations_certifications::pair;

#[derive(Serialize, Observation)]
#[observation(namespace = "cert::point", tags("detail"))]
struct Event {
    parent: usize,
    child: usize,
    sequence: usize,
}
#[derive(Serialize, Observation)]
#[observation(namespace = "cert::limited", level = WARN)]
struct Limited {
    parent: usize,
    child: usize,
    sequence: usize,
}
#[derive(Serialize, Observation)]
#[observation(namespace = "cert::sample", level = ERROR)]
struct Sample {
    parent: usize,
    sequence: usize,
}
#[derive(Serialize, Observation)]
#[observation(namespace = "cert::blocked", level = ERROR, tags("excluded"))]
struct Blocked;
#[derive(Serialize, Observation)]
#[observation(namespace = "cert::work")]
struct Work {
    parent: usize,
    child: usize,
}

fn setup(directory: &Path) -> Arc<MemorySink> {
    let engine = global();
    let memory = Arc::new(MemorySink::new());
    engine
        .register_namespace_channel("cert", "primary")
        .unwrap();
    engine.add_channel_sink("primary", memory.clone());
    engine.add_channel_sink(
        "primary",
        Arc::new(JsonlSink::open_append(directory.join("events.jsonl")).unwrap()),
    );
    memory
}
fn save(directory: &Path, memory: &MemorySink) {
    global().flush_channels().unwrap();
    std::fs::write(
        directory.join("memory.json"),
        serde_json::to_vec(&memory.records()).unwrap(),
    )
    .unwrap();
}
fn threads() {
    let barrier = Arc::new(Barrier::new(6));
    std::thread::scope(|parents| {
        for parent in 0..6 {
            let barrier = barrier.clone();
            parents.spawn(move || {
                let name = format!("thread-{parent}");
                let _scope = start_scope(
                    ScopeConfig::new(&name)
                        .activate_namespace("cert")
                        .unwrap()
                        .override_policy::<Limited>(Policy::first_n(257).unwrap()),
                );
                barrier.wait();
                let mut children = Vec::new();
                for child in 0..4 {
                    children.push(context::thread::spawn(move || {
                        context::thread::spawn(move || {
                            for sequence in 0..200 {
                                observe!(Event {
                                    parent,
                                    child,
                                    sequence
                                })
                                .unwrap();
                                observe!(Limited {
                                    parent,
                                    child,
                                    sequence
                                })
                                .unwrap();
                            }
                        })
                        .join()
                        .unwrap();
                    }));
                }
                for child in children {
                    child.join().unwrap();
                }
                let mut c = certification(format!("thread {parent} context"));
                c.check_fact_expect(
                    "parent restored",
                    Context::capture().scope_name(),
                    Some(name.as_str()),
                );
                c.check_fact_expect(
                    "ordinary thread isolated",
                    std::thread::spawn(|| {
                        observe!(Event {
                            parent: 99,
                            child: 0,
                            sequence: 0
                        })
                    })
                    .join()
                    .unwrap()
                    .unwrap(),
                    false,
                );
                c.assert_certify();
            });
        }
    });
}
fn concurrent() {
    let barrier = Arc::new(Barrier::new(8));
    std::thread::scope(|parents| {
        for parent in 0..8 {
            let barrier = barrier.clone();
            parents.spawn(move || {
                let _scope = start_scope(
                    ScopeConfig::new(format!("run-{parent}"))
                        .activate_all()
                        .override_policy::<Limited>(Policy::first_n(431).unwrap())
                        .override_policy::<Sample>(Policy::every_n(5).unwrap()),
                );
                barrier.wait();
                let mut children = Vec::new();
                for child in 0..4 {
                    children.push(context::thread::spawn(move || {
                        for sequence in 0..250 {
                            observe!(Event {
                                parent,
                                child,
                                sequence
                            })
                            .unwrap();
                            observe!(Limited {
                                parent,
                                child,
                                sequence
                            })
                            .unwrap();
                            observe!(Sample {
                                parent,
                                sequence: child * 250 + sequence
                            })
                            .unwrap();
                        }
                    }));
                }
                for child in children {
                    child.join().unwrap();
                }
            });
        }
    });
}
fn rayon() {
    use ::rayon::prelude::*;
    use context::rayon::ContextThreadPool;
    let make = |name| {
        let _s = start_scope(
            ScopeConfig::new(name)
                .activate_all()
                .override_policy::<Limited>(Policy::first_n(311).unwrap()),
        );
        ContextThreadPool::with_threads(std::num::NonZeroUsize::new(4).unwrap()).unwrap()
    };
    let a = make("rayon-a");
    let b = make("rayon-b");
    let _caller = start_scope(ScopeConfig::new("caller"));
    std::thread::scope(|threads| {
        for (parent, pool) in [(0, &a), (1, &b)] {
            threads.spawn(move || {
                pool.install(|| {
                    (0..16).into_par_iter().for_each(|child| {
                        (0..64).into_par_iter().for_each(|sequence| {
                            observe!(Event {
                                parent,
                                child,
                                sequence
                            })
                            .unwrap();
                            observe!(Limited {
                                parent,
                                child,
                                sequence
                            })
                            .unwrap();
                        })
                    })
                })
            });
        }
    });
    let mut c = certification("Rayon caller context");
    c.check_fact_expect(
        "caller restored",
        Context::capture().scope_name(),
        Some("caller"),
    );
    c.assert_certify();
}
fn fanout(memory: &Arc<MemorySink>) {
    global().set_selection(Selection::new().activate_all());
    let secondary = Arc::new(MemorySink::new());
    let sibling = Arc::new(MemorySink::new());
    global()
        .register_namespace_channel("cert::point", "overlap")
        .unwrap();
    global()
        .register_namespace_channel("cert::point", "overlap")
        .unwrap();
    global()
        .register_namespace_channel("cert::points", "sibling")
        .unwrap();
    global().add_channel_sink("overlap", secondary.clone());
    global()
        .register_namespace_channel("cert", "second")
        .unwrap();
    global().add_channel_sink("second", secondary.clone());
    global().add_channel_sink("sibling", sibling.clone());
    for sequence in 0..100 {
        observe!(Event {
            parent: 0,
            child: 0,
            sequence
        })
        .unwrap();
    }
    let mut c = certification("channel fanout");
    c.check_fact_expect("two matching channels", secondary.len(), 200);
    c.check_fact_expect("one primary delivery", memory.len(), 100);
    c.check_fact_expect("sibling namespace excluded", sibling.is_empty(), true);
    for (index, records) in secondary.shared_records().chunks_exact(2).enumerate() {
        c.check_fact_expect(
            format!("shared record {index}"),
            Arc::ptr_eq(&records[0], &records[1]),
            true,
        );
    }
    c.assert_certify();
}
#[derive(Serialize)]
struct Rich {
    nested: serde_json::Value,
    message: Option<String>,
}
impl Observation for Rich {
    const NAMESPACE: &'static str = "cert::rich";
    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}
fn roundtrip() {
    global().set_selection(Selection::new().activate_all());
    for sequence in 0..1000 {
        observe!(Rich { nested: serde_json::json!({"sequence": sequence, "array": [true, null, 1.25], "object": {"text": "λ 日本語\n\"\\"}, "tags": "payload", "channel": 1, "channels": [1,2]}), message: (sequence % 2 == 0).then(|| "message\nλ".into()) }).unwrap();
    }
    {
        let _scope = start_scope(ScopeConfig::new("span-run"));
        let _a = observe_span!(Rich {
            nested: serde_json::json!([1, 2, 3]),
            message: Some("span".into())
        })
        .unwrap();
    }
    observe!(Limited {
        parent: 0,
        child: 0,
        sequence: 0
    })
    .unwrap();
    observe!(Sample {
        parent: 0,
        sequence: 0
    })
    .unwrap();
}
fn runtime() {
    let mut c = certification("runtime reconfiguration");
    global().set_selection(
        Selection::new()
            .activate_tags(["detail"])
            .unwrap()
            .exclude_tags(["excluded"])
            .unwrap(),
    );
    let extra = Arc::new(MemorySink::new());
    global()
        .register_namespace_channel("cert::point", "overlap")
        .unwrap();
    for parent in 0..4 {
        global().add_channel_sink("overlap", extra.clone());
        let scope = start_scope(
            ScopeConfig::new(format!("release-{parent}"))
                .activate_namespace("cert")
                .unwrap()
                .override_policy::<Limited>(Policy::first_n(73).unwrap())
                .override_policy::<Sample>(Policy::every_n(3).unwrap()),
        );
        for phase in 0..2 {
            let mut children = Vec::new();
            for child in 0..4 {
                children.push(context::thread::spawn(move || {
                    let _span = observe_span!(Work {
                        parent,
                        child: phase * 4 + child
                    })
                    .unwrap();
                    let mut rejected = 0usize;
                    for sequence in phase * 60..(phase + 1) * 60 {
                        observe!(Event {
                            parent,
                            child,
                            sequence
                        })
                        .unwrap();
                        observe!(Limited {
                            parent,
                            child,
                            sequence
                        })
                        .unwrap();
                        observe!(Sample {
                            parent,
                            sequence: child * 120 + sequence
                        })
                        .unwrap();
                        rejected += usize::from(
                            !observe!({
                                panic!("excluded payload");
                                #[allow(unreachable_code)]
                                Blocked
                            })
                            .unwrap(),
                        );
                    }
                    rejected
                }));
            }
            for (child, handle) in children.into_iter().enumerate() {
                c.check_fact_expect(
                    format!("excluded attempts {parent}/{phase}/{child}"),
                    handle.join().unwrap(),
                    60usize,
                );
            }
            if phase == 0 {
                global().clear_channel_sinks("overlap");
                global().set_selection(
                    Selection::new()
                        .activate_all()
                        .exclude_tags(["excluded"])
                        .unwrap(),
                );
            }
        }
        {
            use ::rayon::prelude::*;
            let pool = context::rayon::ContextThreadPool::with_threads(
                std::num::NonZeroUsize::new(3).unwrap(),
            )
            .unwrap();
            pool.install(|| {
                (0..30).into_par_iter().for_each(|sequence| {
                    observe!(Event {
                        parent,
                        child: 99,
                        sequence
                    })
                    .unwrap();
                })
            });
        }
        drop(scope);
    }
    let records = extra.records();
    c.check_fact_expect("extra route before detach", records.len(), 4 * 240);
    c.check(
        "no delivery after detach",
        fact(
            "late records",
            records
                .iter()
                .filter(|r| r.data["sequence"].as_u64().unwrap() >= 60)
                .count(),
        )
        .expect(CountRule::Exact(0)),
    );
    c.assert_certify();
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let directory = Path::new(&args[2]);
    if args[1] == "local-drop" {
        let sink = JsonlSink::open_append(directory.join("events.jsonl")).unwrap();
        let engine = ObservationEngine::isolated();
        let memory = Arc::new(MemorySink::new());
        engine.set_selection(Selection::new().activate_all());
        engine.register_namespace_channel("cert", "memory").unwrap();
        engine.add_channel_sink("memory", memory.clone());
        for sequence in 0..40000 {
            engine
                .observe_if_active(|| Event {
                    parent: 0,
                    child: 0,
                    sequence,
                })
                .unwrap();
        }
        for record in memory.shared_records() {
            sink.write_shared(record).unwrap();
        }
        drop(sink);
        std::fs::write(
            directory.join("memory.json"),
            serde_json::to_vec(&memory.records()).unwrap(),
        )
        .unwrap();
        return;
    }
    let memory = setup(directory);
    match args[1].as_str() {
        "threads" => threads(),
        "concurrent" => concurrent(),
        "fanout" => fanout(&memory),
        "roundtrip" => roundtrip(),
        "runtime" => runtime(),
        "rayon" => rayon(),
        "global-flush" => {
            global().set_selection(Selection::new().activate_all());
            for sequence in 0..40000 {
                observe!(Event {
                    parent: 0,
                    child: 0,
                    sequence
                })
                .unwrap();
            }
        }
        other => panic!("unknown scenario {other}"),
    }
    save(directory, &memory);
    if args[1] == "runtime" {
        let records: Vec<_> = memory
            .records()
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect();
        pair(&records, 32);
    }
}
