//! Reproducible macro workloads; no adaptive iterations. Run via cargo bench --bench runtime.
mod support;
use serde::Serialize;
use std::{
    fs::File,
    hint::black_box,
    io::{BufRead, BufReader},
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};
use sun_observations::{
    global, observe, start_scope, Context, JsonlConfig, JsonlSink, MemorySink, Observation,
    ObservationRecord, Policy, ScopeConfig, Selection,
};

const N: u64 = 1_000_000;
#[derive(Serialize)]
struct Event {
    index: u64,
    value: f64,
}
impl Observation for Event {
    const NAMESPACE: &'static str = "bench::event";
    const TAGS: &'static [&'static str] = &["detail"];
    fn render_text(&self) -> Option<String> {
        Some(format!(
            "{:>5} | {:8.4} | {:8.4} | {:6} | {:7.2}%",
            self.index,
            self.value,
            self.value * 0.9,
            self.index % 1000,
            self.value * 100.0
        ))
    }
}

fn emit(index: u64) {
    black_box(
        observe!(Event {
            index: black_box(index),
            value: 0.5
        })
        .unwrap(),
    );
}
fn report(
    group: &str,
    scenario: &str,
    selectors: usize,
    threads: usize,
    calls: u64,
    sample: usize,
    timing: (Duration, Duration),
) {
    let (producer, total) = timing;
    println!(
        "{group},{scenario},{selectors},{threads},{calls},{sample},{:.3},{:.3}",
        producer.as_nanos() as f64 / calls as f64,
        total.as_nanos() as f64 / calls as f64
    );
}
fn selectors(count: usize) -> Selection {
    let mut selection = Selection::new();
    for i in 0..count {
        selection = selection
            .activate_namespace(format!("unrelated::n{i}"))
            .unwrap()
            .activate_tags([format!("tag_{i}")])
            .unwrap()
            .exclude_namespace(format!("excluded::n{i}"))
            .unwrap()
            .exclude_tags([format!("exclude_{i}")])
            .unwrap();
    }
    selection
}
fn configure(count: usize, active: bool, excluded: bool) {
    let mut selection = selectors(count);
    if active {
        selection = selection.activate_namespace("bench").unwrap();
    }
    if excluded {
        selection = selection.exclude_tags(["detail"]).unwrap();
    }
    global().set_selection(selection);
}
fn reset() {
    for channel in ["a", "b", "c"] {
        global().clear_channel_sinks(channel);
    }
}
fn warm(policy: Policy) {
    let iterations = if matches!(policy, Policy::EveryN(_)) {
        100
    } else {
        1
    };
    for _ in 0..iterations {
        emit(u64::MAX);
    }
    global().flush_channels().unwrap();
}
fn macro_run(
    group: &str,
    name: &str,
    count: usize,
    policy: Policy,
    active: bool,
    excluded: bool,
    sample: usize,
) {
    reset();
    configure(count, active, excluded);
    let _scope = start_scope(ScopeConfig::new("benchmark").override_policy::<Event>(policy));
    warm(policy);
    let start = Instant::now();
    for index in 0..N {
        emit(index);
    }
    let producer = start.elapsed();
    global().flush_channels().unwrap();
    report(
        group,
        name,
        count,
        1,
        N,
        sample,
        (producer, start.elapsed()),
    );
}
fn cold(count: usize, sample: usize) {
    reset();
    configure(count, false, false);
    let mut total = Duration::ZERO;
    let calls = 1000;
    for _ in 0..calls {
        let _scope = start_scope(ScopeConfig::new("cold"));
        let start = Instant::now();
        emit(0);
        total += start.elapsed();
    }
    report(
        "selectors_cold",
        "first_resolve",
        count,
        1,
        calls,
        sample,
        (total, total),
    );
}

#[derive(Clone, Copy)]
enum Mode {
    NoSink,
    Memory,
    Jsonl,
    Both,
    Fanout,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::NoSink => "no_sink",
            Self::Memory => "memory",
            Self::Jsonl => "jsonl",
            Self::Both => "memory_jsonl",
            Self::Fanout => "fanout_2channels_4sinks",
        }
    }
}
struct Capture {
    _directory: tempfile::TempDir,
    memories: Vec<Arc<MemorySink>>,
    file: Option<std::path::PathBuf>,
}
fn capture(mode: Mode) -> Capture {
    reset();
    let directory = support::tempdir().unwrap();
    let mut memories = Vec::new();
    let mut file = None;
    if matches!(mode, Mode::Memory | Mode::Both | Mode::Fanout) {
        let copies = if matches!(mode, Mode::Fanout) { 4 } else { 1 };
        for i in 0..copies {
            let memory = Arc::new(MemorySink::with_config(
                sun_observations::MemorySinkConfig::new(N as usize + 1, 0.1).unwrap(),
            ));
            global().add_channel_sink(if i < 2 { "a" } else { "b" }, memory.clone());
            memories.push(memory);
        }
    }
    if matches!(mode, Mode::Jsonl | Mode::Both) {
        let path = directory.path().join("records.jsonl");
        global().add_channel_sink("a", Arc::new(JsonlSink::open_append(&path).unwrap()));
        file = Some(path);
    }
    Capture {
        _directory: directory,
        memories,
        file,
    }
}
fn verify_file(path: &std::path::Path, expected: Option<u64>, last: u64) -> (u64, u64) {
    let mut count = 0;
    let mut first = None;
    let mut newest = 0;
    let mut previous = None;
    for line in BufReader::new(File::open(path).unwrap()).split(b'\n') {
        let line = line.unwrap();
        assert!(!line.is_empty());
        let record: ObservationRecord = serde_json::from_slice(&line).unwrap();
        let index = record.data["index"].as_u64().unwrap();
        first.get_or_insert(index);
        newest = index;
        if expected.is_none() {
            if let Some(previous) = previous {
                assert_eq!(index, previous + 1);
            }
            previous = Some(index);
        }
        count += 1;
    }
    if let Some(expected) = expected {
        assert_eq!(count, expected);
    } else {
        assert!(first.unwrap() > 0);
    }
    assert_eq!(newest, last);
    let mut file = File::open(path).unwrap();
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::End(-1)).unwrap();
    let mut end = [0];
    file.read_exact(&mut end).unwrap();
    assert_eq!(end, [b'\n']);
    (count, first.unwrap())
}
fn sink_run(mode: Mode, threads: usize, policy: Policy, group: &str, sample: usize) {
    let capture = capture(mode);
    configure(0, true, false);
    let _scope = start_scope(ScopeConfig::new("benchmark").override_policy::<Event>(policy));
    warm(policy);
    let context = Context::capture();
    let barrier = Arc::new(Barrier::new(threads + 1));
    let (producer, total) = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for thread in 0..threads {
            let context = context.clone();
            let barrier = barrier.clone();
            workers.push(scope.spawn(move || {
                context.run(|| {
                    barrier.wait();
                    for i in 0..N / threads as u64 {
                        emit(thread as u64 * (N / threads as u64) + i);
                    }
                })
            }));
        }
        let start = Instant::now();
        barrier.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        let producer = start.elapsed();
        global().flush_channels().unwrap();
        (producer, start.elapsed())
    });
    let name = if policy == Policy::Always {
        mode.name().to_owned()
    } else {
        format!("{}_every100", mode.name())
    };
    report(group, &name, 0, threads, N, sample, (producer, total));
    let expected = if policy == Policy::Always {
        N + 1
    } else {
        N / 100 + 1
    };
    for memory in &capture.memories {
        assert_eq!(memory.len() as u64, expected);
    }
    if let Some(path) = &capture.file {
        let last = if threads == 1 {
            N - 1
        } else {
            let record: ObservationRecord = serde_json::from_str(
                &BufReader::new(File::open(path).unwrap())
                    .lines()
                    .last()
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            record.data["index"].as_u64().unwrap()
        };
        verify_file(path, Some(expected), last);
    }
    drop(_scope);
    reset();
}
fn short_threads(threads: usize, active: bool, sample: usize) {
    reset();
    configure(0, active, false);
    let _scope = start_scope(ScopeConfig::new("short-lived"));
    warm(Policy::Always);
    let context = Context::capture();
    let start = Instant::now();
    for _ in 0..threads {
        let context = context.clone();
        std::thread::spawn(move || {
            context.run(|| {
                for i in 0..N / threads as u64 {
                    emit(i);
                }
            })
        })
        .join()
        .unwrap();
    }
    let elapsed = start.elapsed();
    report(
        "short_threads",
        if active { "always" } else { "rejected" },
        0,
        threads,
        N,
        sample,
        (elapsed, elapsed),
    );
}
fn compaction(limit: bool, sample: usize) {
    reset();
    configure(0, true, false);
    let directory = support::tempdir().unwrap();
    let path = directory.path().join("retention.jsonl");
    let config = if limit {
        JsonlConfig::new()
            .max_file_size_bytes(1024 * 1024)
            .unwrap()
            .trim_fraction(0.25)
            .unwrap()
    } else {
        JsonlConfig::new()
    };
    let sink = Arc::new(JsonlSink::with_config(&path, config).unwrap());
    global().add_channel_sink("c", sink.clone());
    let _scope = start_scope(ScopeConfig::new("compaction"));
    let mut batch_times = [Duration::ZERO; 2];
    let mut batch_calls = [0_u64; 2];
    let start = Instant::now();
    for begin in (0..N).step_by(256) {
        let previous = sink.stats().compactions;
        let batch_start = Instant::now();
        for index in begin..(begin + 256).min(N) {
            emit(index);
        }
        global().flush_channels().unwrap();
        let kind = usize::from(sink.stats().compactions > previous);
        batch_times[kind] += batch_start.elapsed();
        batch_calls[kind] += (N - begin).min(256);
    }
    let elapsed = start.elapsed();
    report(
        "writer_batches",
        if limit {
            "compacting_amortized"
        } else {
            "normal"
        },
        0,
        1,
        N,
        sample,
        (elapsed, elapsed),
    );
    if limit {
        for kind in 0..2 {
            if batch_calls[kind] != 0 {
                report(
                    "writer_batch_latency",
                    if kind == 0 {
                        "non_compacting"
                    } else {
                        "compaction_triggering"
                    },
                    0,
                    1,
                    batch_calls[kind],
                    sample,
                    (batch_times[kind], batch_times[kind]),
                );
            }
        }
    }
    let (count, first) = verify_file(&path, (!limit).then_some(N), N - 1);
    let size = std::fs::metadata(&path).unwrap().len();
    let stats = sink.stats();
    if limit {
        assert!((768 * 1024 - 1024..=1024 * 1024).contains(&size));
        assert!(stats.compactions > 2);
    }
    eprintln!("retention limit={limit} sample={sample} lines={count} first={first} bytes={size} batches={} compactions={}", stats.batches, stats.compactions);
    drop(_scope);
    reset();
}
fn main() {
    support::run(benchmark);
}

fn benchmark() {
    let repeats = std::env::var("SUN_OBS_BENCH_REPEATS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(3);
    let group = std::env::var("SUN_OBS_BENCH_GROUP").unwrap_or_default();
    let enabled = |name: &str| group.is_empty() || group == name;
    global().register_namespace_channel("bench", "a").unwrap();
    global()
        .register_namespace_channel("bench::event", "b")
        .unwrap();
    global().register_namespace_channel("bench", "c").unwrap();
    println!("group,scenario,selectors_per_category,threads,calls,sample,producer_ns_per_call,e2e_ns_per_call");
    for sample in 0..repeats {
        if enabled("hot") {
            for (name, policy, active, excluded) in [
                ("inactive", Policy::Always, false, false),
                ("excluded", Policy::Always, true, true),
                ("never", Policy::Never, true, false),
                ("always", Policy::Always, true, false),
                ("every100", Policy::every_n(100).unwrap(), true, false),
            ] {
                macro_run("hot", name, 0, policy, active, excluded, sample);
            }
        }
        if enabled("selectors") {
            for count in [0, 10, 100, 1000, 10000] {
                cold(count, sample);
                macro_run(
                    "selectors_warm",
                    "inactive",
                    count,
                    Policy::Always,
                    false,
                    false,
                    sample,
                );
                macro_run(
                    "selectors_warm",
                    "active",
                    count,
                    Policy::Always,
                    true,
                    false,
                    sample,
                );
            }
        }
        if enabled("sinks") {
            for mode in [
                Mode::NoSink,
                Mode::Memory,
                Mode::Jsonl,
                Mode::Both,
                Mode::Fanout,
            ] {
                sink_run(mode, 1, Policy::Always, "sinks", sample);
            }
        }
        if enabled("threads") {
            for threads in [1, 2, 4, 8, 16] {
                for (mode, policy) in [
                    (Mode::NoSink, Policy::Always),
                    (Mode::NoSink, Policy::every_n(100).unwrap()),
                    (Mode::Memory, Policy::Always),
                    (Mode::Jsonl, Policy::Always),
                ] {
                    sink_run(mode, threads, policy, "threads", sample);
                }
            }
        }
        if enabled("short_threads") {
            for active in [false, true] {
                for threads in [1, 1000] {
                    short_threads(threads, active, sample);
                }
            }
        }
        if enabled("compaction") {
            compaction(false, sample);
            compaction(true, sample);
        }
    }
    reset();
}
