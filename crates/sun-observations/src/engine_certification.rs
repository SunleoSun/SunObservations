use super::{ProcessIdentityAllocator, PROCESS_IDENTITY_CAPACITY};
use crate::*;
use serde::{Serialize, Serializer};
use std::sync::{
    atomic::{AtomicU32, AtomicUsize, Ordering},
    Arc,
};

#[test]
fn shared_namespace_types_keep_distinct_tags_and_policy_after_reconfiguration() {
    #[derive(Serialize, Observation)]
    #[observation(namespace = "shared::identity", tags("alpha"))]
    struct Alpha;
    #[derive(Serialize, Observation)]
    #[observation(namespace = "shared::identity", tags("beta"))]
    struct Beta;
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_tags(["alpha"]).unwrap());
    let _scope = start_scope(
        ScopeConfig::new("typed policy").override_policy::<Alpha>(Policy::first_n(1).unwrap()),
    );
    assert!(engine.observe_if_active(|| Alpha).unwrap());
    assert!(!engine.observe_if_active(|| Beta).unwrap());
    engine.set_selection(Selection::new().activate_all());
    assert!(!engine.observe_if_active(|| Alpha).unwrap());
    assert!(engine.observe_if_active(|| Beta).unwrap());
    assert!(engine.observe_if_active(|| Beta).unwrap());
}

#[test]
fn admitted_sink_failure_consumes_budget_and_serialization_failure_reaches_no_sink() {
    let mut certification = sun_observations::certifications::certification(
        "admitted sink failure consumes budget and serialization failure reaches no sink",
    );
    struct Failure;
    impl Sink for Failure {
        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }
        fn write(&self, _: &ObservationRecord) -> std::io::Result<()> {
            Err(std::io::Error::other("sink"))
        }
    }
    let engine = ObservationEngine::isolated();
    let memory = Arc::new(MemorySink::new());
    engine.set_selection(Selection::new().activate_all());
    engine
        .register_namespace_channel("admission", "sinks")
        .unwrap();
    engine.add_channel_sink("sinks", Arc::new(Failure));
    engine.add_channel_sink("sinks", memory.clone());
    let _scope = start_scope(
        ScopeConfig::new("budget").override_policy::<Event>(Policy::first_n(1).unwrap()),
    );
    certification.check_fact_expect(
        "engine.observe_if_active(|| Event).is_err()",
        engine.observe_if_active(|| Event).is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine .observe_if_active::<Event, _>(|| panic!(\"consumed\")) .unwrap()",
        engine
            .observe_if_active::<Event, _>(|| panic!("consumed"))
            .unwrap(),
        false,
    );
    certification.check_fact_expect("memory.len()", &(memory.len()), &(1));
    let calls = Arc::new(AtomicUsize::new(0));
    certification.check_fact_expect(
        "serialization error surfaced",
        matches!(
            engine.observe_if_active(|| Probe { calls, fail: true }),
            Err(ObservationError::Serialize(_))
        ),
        true,
    );
    certification.check_fact_expect("memory.len()", &(memory.len()), &(1));
    let r = &memory.records()[0];
    certification.check_fact_expect("r.proc", r.proc.as_str(), engine.process_id());
    certification.check_fact_expect(
        "r.scope.as_deref()",
        &(r.scope.as_deref()),
        &(Some("budget")),
    );
    certification.check_fact_expect("r.namespace", r.namespace.as_str(), Event::NAMESPACE);
    certification.check_fact_expect("r.level", &(r.level), &(ObservationLevel::ERROR));
    certification.assert_certify();
}

#[derive(Serialize, Observation)]
#[observation(namespace = "admission::event", level = ERROR, tags("detail"))]
struct Event;
#[test]
fn activation_union_and_exclusion_veto_remain_lazy_at_every_boundary() {
    let mut certification = sun_observations::certifications::certification(
        "activation union and exclusion veto remain lazy at every boundary",
    );
    let engine = ObservationEngine::isolated();
    certification.check_fact_expect(
        "engine .observe_if_active::<Event, _>(|| panic!(\"empty\")) .unwrap()",
        engine
            .observe_if_active::<Event, _>(|| panic!("empty"))
            .unwrap(),
        false,
    );
    for global in [
        Selection::new().activate_namespace("admission").unwrap(),
        Selection::new().activate_tags(["detail"]).unwrap(),
    ] {
        engine.set_selection(global);
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event).unwrap()",
            engine.observe_if_active(|| Event).unwrap(),
            true,
        );
        for local in [
            ScopeConfig::new("veto")
                .exclude_namespace("admission")
                .unwrap(),
            ScopeConfig::new("veto").exclude_tags(["detail"]).unwrap(),
        ] {
            let _guard = start_scope(local);
            certification.check_fact_expect(
                "engine .observe_if_active::<Event, _>(|| panic!(\"veto\")) .unwrap()",
                engine
                    .observe_if_active::<Event, _>(|| panic!("veto"))
                    .unwrap(),
                false,
            );
        }
    }
    for local in [
        ScopeConfig::new("local")
            .activate_namespace("admission")
            .unwrap(),
        ScopeConfig::new("local").activate_tags(["detail"]).unwrap(),
    ] {
        engine.set_selection(Selection::new());
        let _guard = start_scope(local);
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event).unwrap()",
            engine.observe_if_active(|| Event).unwrap(),
            true,
        );
        for global in [
            Selection::new().exclude_namespace("admission").unwrap(),
            Selection::new().exclude_tags(["detail"]).unwrap(),
        ] {
            engine.set_selection(global);
            certification.check_fact_expect(
                "engine .observe_if_active::<Event, _>(|| panic!(\"global veto\")) .unwrap()",
                engine
                    .observe_if_active::<Event, _>(|| panic!("global veto"))
                    .unwrap(),
                false,
            );
        }
    }
    certification.assert_certify();
}
struct Probe {
    calls: Arc<AtomicUsize>,
    fail: bool,
}
impl Serialize for Probe {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            Err(serde::ser::Error::custom("serialization"))
        } else {
            serializer.serialize_u64(17)
        }
    }
}
impl Observation for Probe {
    const NAMESPACE: &'static str = "admission::probe";
    fn message(&self) -> Option<&str> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Some("message")
    }
}
#[test]
fn admitted_failures_consume_policy_but_rejections_do_not_and_sinkless_callbacks_run() {
    let mut certification = sun_observations::certifications::certification(
        "admitted failures consume policy but rejections do not and sinkless callbacks run",
    );
    let engine = ObservationEngine::isolated();
    let calls = Arc::new(AtomicUsize::new(0));
    let _scope = start_scope(
        ScopeConfig::new("budget").override_policy::<Probe>(Policy::first_n(2).unwrap()),
    );
    certification.check_fact_expect(
        "engine .observe_if_active::<Probe, _>(|| panic!(\"selection\")) .unwrap()",
        engine
            .observe_if_active::<Probe, _>(|| panic!("selection"))
            .unwrap(),
        false,
    );
    engine.set_selection(Selection::new().activate_all());
    certification.check_fact_expect(
        "admitted failures consume policy but rejections do not and sinkless callbacks run",
        matches!(
            engine.observe_if_active(|| Probe {
                calls: calls.clone(),
                fail: true
            }),
            Err(ObservationError::Serialize(_))
        ),
        true,
    );
    certification.check_fact_expect(
        "engine .observe_if_active(|| Probe { calls: calls.clone(), fail: false }) .unwrap()",
        engine
            .observe_if_active(|| Probe {
                calls: calls.clone(),
                fail: false,
            })
            .unwrap(),
        true,
    );
    certification.check_fact_expect(
        "calls.load(Ordering::Relaxed)",
        &(calls.load(Ordering::Relaxed)),
        &(3),
    );
    certification.check_fact_expect(
        "engine .observe_if_active::<Probe, _>(|| panic!(\"budget\")) .unwrap()",
        engine
            .observe_if_active::<Probe, _>(|| panic!("budget"))
            .unwrap(),
        false,
    );
    certification.assert_certify();
}
#[test]
fn engine_recreation_cannot_reuse_retired_tls_identity() {
    let mut certification = sun_observations::certifications::certification(
        "engine recreation cannot reuse retired tls identity",
    );
    let mut identities = std::collections::HashSet::new();
    for i in 0..100 {
        let engine = ObservationEngine::isolated();
        let identity = engine.process_id().to_owned();
        let (pid, suffix) = identity.split_once('_').unwrap();
        certification.check_fact_expect("PID prefix", pid, std::process::id().to_string().as_str());
        certification.check_fact_expect("compact suffix width", suffix.len(), 5);
        certification.check_fact_expect(
            "lowercase ASCII base36 suffix",
            suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase()),
            true,
        );
        certification.check_fact_expect(
            "identities.insert(engine.process_id().to_owned())",
            identities.insert(engine.process_id().to_owned()),
            true,
        );
        engine.set_selection(if i % 2 == 0 {
            Selection::new().activate_all()
        } else {
            Selection::new()
        });
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event).unwrap()",
            &(engine.observe_if_active(|| Event).unwrap()),
            &(i % 2 == 0),
        );
        certification.check_fact_expect(
            "stable engine identity",
            engine.process_id(),
            identity.as_str(),
        );
    }
    certification.assert_certify();
}

#[test]
fn concurrent_engines_and_singleton_have_distinct_process_identities() {
    use crate::certifications::{certification, collection};
    let workers: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                let engines: Vec<_> = (0..128).map(|_| ObservationEngine::isolated()).collect();
                engines
                    .iter()
                    .map(|engine| engine.process_id().to_owned())
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut identities = vec![global().process_id().to_owned()];
    for worker in workers {
        identities.extend(worker.join().unwrap());
    }
    let mut c = certification("concurrent engines and singleton retain distinct identities");
    let rows = collection("engine identities", &identities);
    c.check("all engines created", rows.count().expect(1025usize));
    c.check(
        "no process identity reused",
        rows.unique(|id| id.clone()).expect_valid(),
    );
    c.assert_certify();
}

#[test]
fn compact_process_identity_encoding_wraps_without_reuse_and_exhaustion_fails_closed() {
    let mut c = crate::certifications::certification("compact identity boundaries and exhaustion");
    for (offset, expected) in [
        (0, "00000"),
        (9, "00009"),
        (10, "0000a"),
        (35, "0000z"),
        (36, "00010"),
        (PROCESS_IDENTITY_CAPACITY - 1, "zzzzz"),
    ] {
        let allocator = ProcessIdentityAllocator {
            offset,
            allocated: AtomicU32::new(0),
        };
        c.check_fact_expect(
            "fixed-width base36",
            allocator.allocate(),
            Some(format!("{}_{expected}", std::process::id())),
        );
    }
    let wrapping = ProcessIdentityAllocator {
        offset: PROCESS_IDENTITY_CAPACITY - 1,
        allocated: AtomicU32::new(0),
    };
    c.check_fact_expect(
        "last suffix",
        wrapping.allocate(),
        Some(format!("{}_zzzzz", std::process::id())),
    );
    c.check_fact_expect(
        "randomized sequence wraps through zero",
        wrapping.allocate(),
        Some(format!("{}_00000", std::process::id())),
    );
    for (offset, last) in [
        (0, "zzzzz"),
        (1, "00000"),
        (PROCESS_IDENTITY_CAPACITY - 1, "zzzzy"),
    ] {
        let exhausted = ProcessIdentityAllocator {
            offset,
            allocated: AtomicU32::new(PROCESS_IDENTITY_CAPACITY - 1),
        };
        c.check_fact_expect(
            "last available identity",
            exhausted.allocate(),
            Some(format!("{}_{last}", std::process::id())),
        );
        c.check_fact_expect(
            "exhaustion never reuses an identity",
            exhausted.allocate(),
            None,
        );
        c.check_fact_expect("exhaustion remains terminal", exhausted.allocate(), None);
    }
    c.assert_certify();
}

mod admission {
    use super::super::*;
    use crate::{start_scope, test_support::capture, MemorySink, ScopeConfig};
    use serde::{Serialize, Serializer};

    #[derive(Serialize, crate::Observation)]
    #[observation(namespace = "engine_tests::train::epoch", level = WARN, tags("detail"))]
    struct Event {
        epoch: u32,
    }

    #[test]
    fn global_and_scope_selectors_union_but_either_exclusion_vetoes() {
        let mut certification = sun_observations::certifications::certification(
            "global and scope selectors union but either exclusion vetoes",
        );
        let engine = ObservationEngine::isolated();
        let sink = capture(&engine, "engine_tests");
        engine.set_selection(Selection::new().activate_tags(["detail"]).unwrap());
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { epoch: 1 }).unwrap()",
            engine.observe_if_active(|| Event { epoch: 1 }).unwrap(),
            true,
        );
        {
            let _scope = start_scope(
                ScopeConfig::new("exclude")
                    .exclude_namespace("engine_tests::train")
                    .unwrap(),
            );
            certification.check_fact_expect(
                "engine.observe_if_active(|| Event { epoch: 2 }).unwrap()",
                engine.observe_if_active(|| Event { epoch: 2 }).unwrap(),
                false,
            );
        }
        engine.set_selection(Selection::new().exclude_tags(["detail"]).unwrap());
        {
            let _scope = start_scope(
                ScopeConfig::new("active")
                    .activate_namespace("engine_tests")
                    .unwrap(),
            );
            certification.check_fact_expect(
                "engine.observe_if_active(|| Event { epoch: 3 }).unwrap()",
                engine.observe_if_active(|| Event { epoch: 3 }).unwrap(),
                false,
            );
        }
        engine.set_selection(Selection::new());
        {
            let _scope = start_scope(
                ScopeConfig::new("scope-only")
                    .activate_namespace("engine_tests")
                    .unwrap(),
            );
            certification.check_fact_expect(
                "engine.observe_if_active(|| Event { epoch: 4 }).unwrap()",
                engine.observe_if_active(|| Event { epoch: 4 }).unwrap(),
                true,
            );
        }
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(2));
        certification.check_fact_expect("records[0].scope", &(records[0].scope), &(None));
        certification.check_fact_expect(
            "records[1].scope.as_deref()",
            &(records[1].scope.as_deref()),
            &(Some("scope-only")),
        );
        certification.check_fact_expect(
            "records[1].level",
            &(records[1].level),
            &(ObservationLevel::WARN),
        );
        certification.assert_certify();
    }

    #[test]
    fn namespace_routing_fans_out_and_envelope_uses_engine_identity_and_clocks() {
        let mut certification = sun_observations::certifications::certification(
            "namespace routing fans out and envelope uses engine identity and clocks",
        );
        let engine = ObservationEngine::isolated();
        let all = capture(&engine, "engine_tests");
        let train = capture(&engine, "engine_tests::train");
        let sibling = capture(&engine, "engine_tests::infer");
        engine.set_selection(Selection::new().activate_namespace("engine_tests").unwrap());
        std::thread::sleep(std::time::Duration::from_millis(1));
        let before_ns = engine.started.elapsed().as_nanos();
        engine.observe_if_active(|| Event { epoch: 7 }).unwrap();
        let after_ns = engine.started.elapsed().as_nanos();
        let record = all.records().pop().unwrap();
        certification.check_fact_expect(
            "clock retains nanosecond units",
            (before_ns..=after_ns).contains(&u128::from(record.proc_ns)),
            true,
        );
        certification.check_fact_expect("record.proc", record.proc.as_str(), engine.process_id());
        certification.check_fact_expect(
            "chrono::DateTime::parse_from_rfc3339(&record.ts).is_ok()",
            chrono::DateTime::parse_from_rfc3339(&record.ts).is_ok(),
            true,
        );
        certification.check_fact_expect(
            "record.data",
            &(record.data),
            &(serde_json::json!({"epoch": 7})),
        );
        certification.check_fact_expect(
            "train.records()",
            train.records().as_slice(),
            std::slice::from_ref(&record),
        );
        certification.check_fact_expect("sibling.is_empty()", sibling.is_empty(), true);
        engine.observe_if_active(|| Event { epoch: 8 }).unwrap();
        certification.check_fact_expect(
            "all.records()[1].proc_ns >= record.proc_ns",
            all.records()[1].proc_ns >= record.proc_ns,
            true,
        );
        certification.check_fact_expect(
            "engine.process_id()",
            &(engine.process_id()),
            sun_observations::certifications::NotEqual(
                &(ObservationEngine::isolated().process_id()),
            ),
        );
        certification.assert_certify();
    }

    #[test]
    fn warn_does_not_bypass_selection() {
        let mut certification =
            sun_observations::certifications::certification("warn does not bypass selection");
        let engine = ObservationEngine::isolated();
        certification.check_fact_expect(
            "engine .observe_if_active::<Event, _>(|| panic!(\"must stay lazy\")) .unwrap()",
            engine
                .observe_if_active::<Event, _>(|| panic!("must stay lazy"))
                .unwrap(),
            false,
        );
        certification.assert_certify();
    }

    #[test]
    fn serialization_failure_is_returned_without_writing_a_record() {
        let mut certification = sun_observations::certifications::certification(
            "serialization failure is returned without writing a record",
        );
        struct Bad;
        impl Serialize for Bad {
            fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("deliberate failure"))
            }
        }
        impl Observation for Bad {
            const NAMESPACE: &'static str = "engine_tests::bad";
        }
        let engine = ObservationEngine::isolated();
        let sink = capture(&engine, "engine_tests");
        engine.set_selection(Selection::new().activate_all());
        certification.check_fact_expect(
            "matches!( engine.observe_if_active(|| Bad), Err(ObservationError::Serialize(_)) )",
            matches!(
                engine.observe_if_active(|| Bad),
                Err(ObservationError::Serialize(_))
            ),
            true,
        );
        certification.check_fact_expect("sink.is_empty()", sink.is_empty(), true);
        certification.assert_certify();
    }

    #[test]
    fn message_works_without_registration_and_policy_defaults_to_always() {
        let mut certification = sun_observations::certifications::certification(
            "message works without registration and policy defaults to always",
        );
        #[derive(Serialize)]
        struct Message {
            text: &'static str,
        }
        impl Observation for Message {
            const NAMESPACE: &'static str = "engine_tests::message";
            fn message(&self) -> Option<&str> {
                Some(self.text)
            }
        }
        let engine = ObservationEngine::isolated();
        let sink = capture(&engine, "engine_tests");
        engine.set_selection(Selection::new().activate_all());
        certification.check_fact_expect(
            "engine .observe_if_active(|| Message { text: \"hello\" }) .unwrap()",
            engine
                .observe_if_active(|| Message { text: "hello" })
                .unwrap(),
            true,
        );
        certification.check_fact_expect(
            "engine .observe_if_active(|| Message { text: \"again\" }) .unwrap()",
            engine
                .observe_if_active(|| Message { text: "again" })
                .unwrap(),
            true,
        );
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(2));
        certification.check_fact_expect(
            "records[0].message.as_deref()",
            &(records[0].message.as_deref()),
            &(Some("hello")),
        );
        certification.check_fact_expect(
            "records[0].data",
            &(records[0].data),
            &(serde_json::json!({"text": "hello"})),
        );
        certification.check_fact_expect(
            "records[1].message.as_deref()",
            &(records[1].message.as_deref()),
            &(Some("again")),
        );
        {
            let _scope = start_scope(
                ScopeConfig::new("override").override_policy::<Message>(crate::Policy::Never),
            );
            certification.check_fact_expect(
                "scope policy rejects before construction",
                engine
                    .observe_if_active::<Message, _>(|| {
                        panic!("scope policy must reject before build")
                    })
                    .unwrap(),
                false,
            );
        }
        certification.assert_certify();
    }

    #[test]
    fn sinkless_admission_still_serializes_and_calls_message() {
        let mut certification = sun_observations::certifications::certification(
            "sinkless admission still serializes and calls message",
        );
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Probed {
            calls: Arc<AtomicUsize>,
            fail: bool,
        }
        impl Serialize for Probed {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.calls.fetch_add(1, Ordering::Relaxed);
                if self.fail {
                    Err(serde::ser::Error::custom("failed"))
                } else {
                    serializer.serialize_unit()
                }
            }
        }
        impl Observation for Probed {
            const NAMESPACE: &'static str = "engine_tests::sinkless";
            fn message(&self) -> Option<&str> {
                self.calls.fetch_add(1, Ordering::Relaxed);
                Some("message")
            }
        }
        let engine = ObservationEngine::isolated();
        engine.set_selection(Selection::new().activate_all());
        let calls = Arc::new(AtomicUsize::new(0));
        certification.check_fact_expect(
            "admitted payload constructed",
            engine
                .observe_if_active(|| Probed {
                    calls: calls.clone(),
                    fail: false,
                })
                .unwrap(),
            true,
        );
        certification.check_fact_expect(
            "calls.load(Ordering::Relaxed)",
            &(calls.load(Ordering::Relaxed)),
            &(2),
        );
        certification.check_fact_expect(
            "sinkless admission still serializes and calls message",
            matches!(
                engine.observe_if_active(|| Probed {
                    calls: calls.clone(),
                    fail: true
                }),
                Err(ObservationError::Serialize(_))
            ),
            true,
        );
        certification.check_fact_expect(
            "calls.load(Ordering::Relaxed)",
            &(calls.load(Ordering::Relaxed)),
            &(3),
        );
        certification.assert_certify();
    }

    #[test]
    fn flush_barrier_visits_shared_sinks_once_even_without_routes() {
        let mut certification = sun_observations::certifications::certification(
            "flush barrier visits shared sinks once even without routes",
        );
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Flushed(AtomicUsize);
        impl Sink for Flushed {
            fn write(&self, _: &ObservationRecord) -> io::Result<()> {
                Ok(())
            }
            fn flush(&self) -> io::Result<()> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }
        let engine = ObservationEngine::isolated();
        let sink = Arc::new(Flushed(AtomicUsize::new(0)));
        engine.add_channel_sink("a", sink.clone());
        engine.add_channel_sink("b", sink.clone());
        engine.flush_channels().unwrap();
        certification.check_fact_expect(
            "sink.0.load(Ordering::Relaxed)",
            &(sink.0.load(Ordering::Relaxed)),
            &(1),
        );
        certification.assert_certify();
    }

    #[test]
    fn sink_failures_are_counted_and_other_channels_still_receive() {
        let mut certification = sun_observations::certifications::certification(
            "sink failures are counted and other channels still receive",
        );
        struct Broken;
        impl Sink for Broken {
            fn write(&self, _: &ObservationRecord) -> io::Result<()> {
                Err(io::Error::other("broken"))
            }
            fn flush(&self) -> io::Result<()> {
                Err(io::Error::other("broken"))
            }
        }
        let engine = ObservationEngine::isolated();
        let memory = Arc::new(MemorySink::new());
        engine.add_channel_sink("broken", Arc::new(Broken));
        engine.add_channel_sink("healthy", memory.clone());
        engine
            .register_namespace_channel("engine_tests", "broken")
            .unwrap();
        engine
            .register_namespace_channel("engine_tests", "healthy")
            .unwrap();
        engine.set_selection(Selection::new().activate_all());
        certification.check_fact_expect(
            "sink error surfaced",
            matches!(
                engine.observe_if_active(|| Event { epoch: 1 }),
                Err(ObservationError::Sink(_))
            ),
            true,
        );
        certification.check_fact_expect("memory.len()", &(memory.len()), &(1));
        certification.check_fact_expect(
            "engine.sink_error_count()",
            &(engine.sink_error_count()),
            &(1),
        );
        certification.check_fact_expect(
            "engine.flush_channels().is_err()",
            engine.flush_channels().is_err(),
            true,
        );
        certification.check_fact_expect(
            "engine.sink_error_count()",
            &(engine.sink_error_count()),
            &(2),
        );
        certification.assert_certify();
    }
}
