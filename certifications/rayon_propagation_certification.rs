use sun_observations::certifications::*;

#[derive(serde::Serialize, serde::Deserialize, sun_observations::Observation)]
#[observation(namespace = "rayon_capture::event", tags("evidence"))]
struct Captured {
    run: u64,
    row: u64,
}
#[test]
fn scoped_parallel_captures_follow_rayon_context_without_cross_talk() {
    use rayon::prelude::*;
    let workers: Vec<_> = (0..2)
        .map(|id| {
            std::thread::spawn(move || {
                let run = sun_observations::evidence::capture_observations(
                    sun_observations::evidence::CaptureConfig::new().activate_tag("evidence"),
                    || {
                        let pool =
                            sun_observations::context::rayon::ContextThreadPool::new(2).unwrap();
                        pool.install(|| {
                            (0..512u64).into_par_iter().for_each(|row| {
                                sun_observations::observe!(Captured { run: id, row }).unwrap();
                            })
                        });
                    },
                );
                let events = run.typed::<Captured>().unwrap();
                run.certify("Rayon capture isolation")
                    .check("count", events.source("events").count().expect(512usize))
                    .check(
                        "own execution",
                        events
                            .source("events")
                            .matching_count(|v| v.run != id)
                            .expect(0usize),
                    )
                    .check(
                        "identity",
                        events.source("events").unique(|v| v.row).expect_valid(),
                    )
                    .assert_certify();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}

use sun_observations_certifications::certify as run_scenario;

#[test]
fn public_nested_parallel_work_persists_pool_construction_scope() {
    let records = run_scenario(env!("CARGO_BIN_EXE_scenario"), "rayon");
    let all = source_slice("rayon observations", &records);
    let mut proof = certify("nested parallel work preserves pool construction scope");
    proof.check("record count", all.count().expect(2 * (1024 + 311)));
    for (parent, expected_scope) in [(0, "rayon-a"), (1, "rayon-b")] {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["parent"] == parent)
            .collect();
        let own = source_slice(format!("parent {parent}"), &own);
        proof.section(format!("parent {parent}"), |c| {
            c.check(
                "scope",
                own.matching_count(|r| r["scope"] != expected_scope)
                    .expect(0),
            )
            .check(
                "limited budget",
                own.matching_count(|r| r["namespace"] == "cert::limited")
                    .expect(311),
            );
        });
    }
    proof.assert_certify();
}
