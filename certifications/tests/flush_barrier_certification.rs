use serde::Serialize;
use std::{
    fs,
    io::{self, Write},
    sync::{mpsc, Arc, Barrier, Mutex},
    time::Duration,
};
use sun_observations::*;
use sun_observations_certifications::read;
#[derive(Serialize, Observation)]
#[observation(namespace = "barrier::event")]
struct Event {
    set: &'static str,
    sequence: usize,
}
struct FileProbe {
    file: fs::File,
    count: usize,
    flushes: Arc<Mutex<Vec<usize>>>,
    first: Option<mpsc::Sender<usize>>,
    release: mpsc::Receiver<()>,
}
impl Write for FileProbe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.file.write_all(bytes)?;
        self.count += bytes.iter().filter(|&&b| b == b'\n').count();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.flushes.lock().unwrap().push(self.count);
        if let Some(first) = self.first.take() {
            first.send(self.count).unwrap();
            self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        Ok(())
    }
}
#[test]
fn real_file_engine_barrier_cannot_overtake_prior_bounded_chunks() {
    let mut certification = sun_observations::certifications::certification(
        "real file engine barrier cannot overtake prior bounded chunks",
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    let (first, observed) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let flushes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(
        JsonlSink::writer_with_config(
            FileProbe {
                file: fs::File::create(&path).unwrap(),
                count: 0,
                flushes: flushes.clone(),
                first: Some(first),
                release: resume,
            },
            JsonlConfig::new().flush_interval_ms(60000).unwrap(),
        )
        .unwrap(),
    );
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    engine
        .register_namespace_channel("barrier", "primary")
        .unwrap();
    engine.add_channel_sink("primary", sink.clone());
    engine.add_channel_sink("shared", sink.clone());
    for sequence in 0..50000 {
        engine
            .observe_if_active(|| Event {
                set: "before",
                sequence,
            })
            .unwrap();
    }
    let start = Barrier::new(3);
    std::thread::scope(|threads| {
        let engine = &engine;
        let start = &start;
        let barrier = threads.spawn(move || {
            start.wait();
            engine.flush_channels().unwrap();
        });
        let racing = threads.spawn(move || {
            start.wait();
            for sequence in 0..1000 {
                engine
                    .observe_if_active(|| Event {
                        set: "racing",
                        sequence,
                    })
                    .unwrap();
            }
        });
        start.wait();
        let count = observed.recv_timeout(Duration::from_secs(10)).unwrap();
        certification.check_fact_expect("count >= 50000", count >= 50000, true);
        let at_flush = read(&path);
        certification.check_fact_expect(
            "at_flush .iter() .filter(|r| r[\"data\"][\"set\"] == \"before\") .count()",
            &(at_flush
                .iter()
                .filter(|r| r["data"]["set"] == "before")
                .count()),
            &(50000),
        );
        certification.check_fact_expect(
            "at_flush.iter().all(|r| r[\"data\"][\"set\"] != \"after\")",
            at_flush.iter().all(|r| r["data"]["set"] != "after"),
            true,
        );
        release.send(()).unwrap();
        barrier.join().unwrap();
        racing.join().unwrap();
    });
    certification.check_fact_expect(
        "flushes.lock().unwrap().len()",
        &(flushes.lock().unwrap().len()),
        &(1),
    );
    for sequence in 0..1000 {
        engine
            .observe_if_active(|| Event {
                set: "after",
                sequence,
            })
            .unwrap();
    }
    engine.flush_channels().unwrap();
    certification.check_fact_expect(
        "flushes.lock().unwrap().len()",
        &(flushes.lock().unwrap().len()),
        &(2),
    );
    let records = read(&path);
    certification.check_fact_expect("records.len()", &(records.len()), &(52000));
    for (set, n) in [("before", 50000), ("racing", 1000), ("after", 1000)] {
        let indices: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["set"] == set)
            .map(|r| r["data"]["sequence"].as_u64().unwrap())
            .collect();
        certification.check_fact_expect("indices", &(indices), &((0..n).collect::<Vec<_>>()));
    }
    certification.assert_certify();
}
