mod support;
use serde::Serialize;
use std::{
    cell::Cell,
    fs::File,
    hint::black_box,
    io::{BufRead, BufReader},
    sync::{Arc, Barrier},
    time::Instant,
};
use sun_observations::{
    global, observe, start_scope, ConsoleSink, Context, JsonlSink, Observation, ObservationRecord,
    ScopeConfig, Selection,
};

const N: u64 = 262_144;
thread_local! { static RENDERS: Cell<u64> = const { Cell::new(0) }; }
#[derive(Serialize)]
struct Metrics<const CUSTOM: bool> {
    epoch: u64,
    loss: f64,
    accuracy: f64,
    trades: u64,
    profit: f64,
}
impl<const CUSTOM: bool> Observation for Metrics<CUSTOM> {
    const NAMESPACE: &'static str = "human_bench::metrics";
    fn render_text(&self) -> Option<String> {
        if !CUSTOM {
            return None;
        }
        RENDERS.with(|count| count.set(count.get() + 1));
        Some(format!(
            "{:>5} | {:8.4} | {:8.4} | {:6} | {:7.2}%",
            self.epoch,
            self.loss,
            self.accuracy,
            self.trades,
            self.profit * 100.0
        ))
    }
}
fn emit<const CUSTOM: bool>(epoch: u64) {
    black_box(
        observe!(Metrics::<CUSTOM> {
            epoch: black_box(epoch),
            loss: 0.3812,
            accuracy: 0.6123,
            trades: 191,
            profit: -0.0211
        })
        .unwrap(),
    );
}
fn run<const CUSTOM: bool>(
    name: &str,
    threads: usize,
    consoles: usize,
    jsonl: bool,
    sample: usize,
) {
    global().clear_channel_sinks("human");
    let directory = support::tempdir().unwrap();
    let mut paths = Vec::new();
    for i in 0..consoles {
        let path = directory.path().join(format!("console-{i}.txt"));
        global().add_channel_sink(
            "human",
            Arc::new(ConsoleSink::new(File::create(&path).unwrap())),
        );
        paths.push(path);
    }
    let machine = directory.path().join("records.jsonl");
    if jsonl {
        global().add_channel_sink("human", Arc::new(JsonlSink::open_append(&machine).unwrap()));
    }
    let _scope = start_scope(ScopeConfig::new("benchmark"));
    emit::<CUSTOM>(u64::MAX);
    global().flush_channels().unwrap();
    let context = Context::capture();
    let barrier = Arc::new(Barrier::new(threads + 1));
    let (producer, total, renders) = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for producer in 0..threads {
            let context = context.clone();
            let barrier = barrier.clone();
            workers.push(scope.spawn(move || {
                context.run(|| {
                    RENDERS.with(|count| count.set(0));
                    barrier.wait();
                    for index in 0..N / threads as u64 {
                        emit::<CUSTOM>(producer as u64 * (N / threads as u64) + index);
                    }
                    RENDERS.with(Cell::get)
                })
            }));
        }
        let start = Instant::now();
        barrier.wait();
        let renders: u64 = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum();
        let producer = start.elapsed();
        global().flush_channels().unwrap();
        (producer, start.elapsed(), renders)
    });
    assert_eq!(renders, if CUSTOM { N } else { 0 });
    println!(
        "console,{name},{threads},{N},{sample},{:.3},{:.3},{renders}",
        producer.as_nanos() as f64 / N as f64,
        total.as_nanos() as f64 / N as f64
    );
    for path in paths {
        let count = BufReader::new(File::open(path).unwrap())
            .lines()
            .try_fold(0u64, |count, line| line.map(|_| count + 1))
            .unwrap();
        assert_eq!(count, N + 1);
    }
    if jsonl {
        let mut count = 0;
        for line in BufReader::new(File::open(machine).unwrap()).lines() {
            let record: ObservationRecord = serde_json::from_str(&line.unwrap()).unwrap();
            assert!(record.message.is_none());
            assert!(record.data["epoch"].is_u64());
            count += 1;
        }
        assert_eq!(count, N + 1);
    }
    global().clear_channel_sinks("human");
}
fn representation(sample: usize) {
    let start = Instant::now();
    for epoch in 0..N {
        let row = Metrics::<true> {
            epoch,
            loss: 0.3812,
            accuracy: 0.6123,
            trades: 191,
            profit: -0.0211,
        };
        black_box(row.render_text());
    }
    let ns = start.elapsed().as_nanos() as f64 / N as f64;
    println!("render,metrics_row,1,{N},{sample},{ns:.3},{ns:.3},{N}");
}
fn benchmark() {
    global().set_selection(Selection::new().activate_all());
    global()
        .register_namespace_channel("human_bench", "human")
        .unwrap();
    let repeats = std::env::var("SUN_OBS_BENCH_REPEATS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(5);
    println!("group,scenario,threads,calls,sample,producer_ns_per_call,e2e_ns_per_call,renders");
    for sample in 0..repeats {
        representation(sample);
        for threads in [1, 4, 16] {
            run::<false>("default", threads, 1, false, sample);
            run::<true>("custom", threads, 1, false, sample);
            run::<true>("jsonl_custom", threads, 1, true, sample);
            run::<true>("jsonl_two_human", threads, 2, true, sample);
        }
    }
}
fn main() {
    support::run(benchmark);
}
