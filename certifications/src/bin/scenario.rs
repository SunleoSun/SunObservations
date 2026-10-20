use obs::*;
use serde::Serialize;
use std::{
    path::Path,
    sync::{Arc, Barrier},
};
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
        Arc::new(JsonlSink::append(directory.join("events.jsonl")).unwrap()),
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
                        .override_policy::<Limited>(Policy::FirstN(257)),
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
                assert_eq!(Context::capture().scope_name(), Some(name.as_str()));
                assert!(!std::thread::spawn(|| observe!(Event {
                    parent: 99,
                    child: 0,
                    sequence: 0
                }))
                .join()
                .unwrap()
                .unwrap());
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
                        .override_policy::<Limited>(Policy::MaxPerScope(431))
                        .override_policy::<Sample>(Policy::EveryN(5)),
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
#[cfg(feature = "rayon")]
fn rayon() {
    use ::rayon::prelude::*;
    use context::rayon::ContextThreadPool;
    let make = |name| {
        let _s = start_scope(
            ScopeConfig::new(name)
                .activate_all()
                .override_policy::<Limited>(Policy::FirstN(311)),
        );
        ContextThreadPool::new(4).unwrap()
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
    assert_eq!(Context::capture().scope_name(), Some("caller"));
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
    assert_eq!(secondary.len(), 200);
    assert_eq!(memory.len(), 100);
    assert!(sibling.is_empty());
    for records in secondary.shared_records().chunks_exact(2) {
        assert!(Arc::ptr_eq(&records[0], &records[1]));
    }
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
    global().set_selection(
        Selection::new()
            .activate_tags(["detail"])
            .exclude_tags(["excluded"]),
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
                .override_policy::<Limited>(Policy::FirstN(73))
                .override_policy::<Sample>(Policy::EveryN(3)),
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
                        assert!(!observe!({
                            panic!("excluded payload");
                            #[allow(unreachable_code)]
                            Blocked
                        })
                        .unwrap());
                    }
                }));
            }
            for child in children {
                child.join().unwrap();
            }
            if phase == 0 {
                global().clear_channel_sinks("overlap");
                global().set_selection(Selection::new().activate_all().exclude_tags(["excluded"]));
            }
        }
        #[cfg(feature = "rayon")]
        {
            use ::rayon::prelude::*;
            let pool = context::rayon::ContextThreadPool::new(3).unwrap();
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
    assert_eq!(records.len(), 4 * 240);
    assert!(records
        .iter()
        .all(|r| r.data["sequence"].as_u64().unwrap() < 60));
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let directory = Path::new(&args[2]);
    if args[1] == "local-drop" {
        let sink = JsonlSink::append(directory.join("events.jsonl")).unwrap();
        let engine = ObservationEngine::new();
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
        #[cfg(feature = "rayon")]
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
