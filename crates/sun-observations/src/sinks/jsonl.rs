use crate::{ObservationRecord, Sink};
use crossbeam_channel::{unbounded, Sender};
use parking_lot::Mutex;
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, OnceLock,
    },
    thread::JoinHandle,
    time::Duration,
};
#[path = "jsonl_writer.rs"]
mod writer;

/// File limits are optional. MiB means 1024 * 1024 bytes. Invalid settings fail construction.
#[derive(Clone, Debug)]
pub struct JsonlConfig {
    pub(crate) interval: Duration,
    pub(crate) max_bytes: Option<u64>,
    pub(crate) trim: f64,
}
impl Default for JsonlConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(1000),
            max_bytes: None,
            trim: 0.10,
        }
    }
}
impl JsonlConfig {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn flush_interval_ms(mut self, ms: u64) -> io::Result<Self> {
        self.interval = Duration::from_millis(ms);
        self.validate()?;
        Ok(self)
    }
    pub fn max_file_size_mb(self, mib: u64) -> io::Result<Self> {
        let bytes = mib
            .checked_mul(1024 * 1024)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "MiB limit overflow"))?;
        self.max_file_size_bytes(bytes)
    }
    pub fn max_file_size_bytes(mut self, bytes: u64) -> io::Result<Self> {
        self.max_bytes = Some(bytes);
        self.validate()?;
        Ok(self)
    }
    pub fn trim_fraction(mut self, fraction: f64) -> io::Result<Self> {
        self.trim = fraction;
        self.validate()?;
        Ok(self)
    }
    fn validate(&self) -> io::Result<()> {
        if self.interval.is_zero()
            || std::time::Instant::now()
                .checked_add(self.interval)
                .is_none()
            || self.max_bytes == Some(0)
            || !self.trim.is_finite()
            || self.trim <= 0.0
            || self.trim >= 1.0
        {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "flush_interval_ms and file size must be positive; trim_fraction must be finite and between 0 and 1"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonlStats {
    pub records_written: u64,
    pub batches: u64,
    pub flushes: u64,
    pub compactions: u64,
}
#[derive(Clone)]
struct Failure {
    kind: io::ErrorKind,
    message: String,
}
impl Failure {
    fn error(&self) -> io::Error {
        io::Error::new(self.kind, self.message.clone())
    }
}
#[derive(Default)]
struct Status {
    failed: AtomicBool,
    failure: Mutex<Option<Failure>>,
    records: AtomicU64,
    batches: AtomicU64,
    flushes: AtomicU64,
    compactions: AtomicU64,
}
impl Status {
    fn fail(&self, error: io::Error) {
        let mut failure = self.failure.lock();
        if failure.is_none() {
            *failure = Some(Failure {
                kind: error.kind(),
                message: error.to_string(),
            });
        }
        self.failed.store(true, Ordering::Release);
    }
    fn error(&self) -> io::Error {
        self.failure
            .lock()
            .as_ref()
            .map(Failure::error)
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "JSONL writer stopped"))
    }
}
enum Destination {
    File(PathBuf),
    Writer(Box<dyn Write + Send>),
}
enum Message {
    Record(Arc<ObservationRecord>),
    Flush(Sender<io::Result<()>>),
}
struct Worker {
    sender: Option<Sender<Message>>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    fn send(&self, message: Message, status: &Status) -> io::Result<()> {
        if status.failed.load(Ordering::Acquire) {
            return Err(status.error());
        }
        self.sender
            .as_ref()
            .expect("live worker")
            .send(message)
            .map_err(|_| status.error())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        drop(self.sender.take());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Async, lazy, unbounded JSONL destination. All file I/O and JSONL encoding run on its worker.
/// Call flush() / engine.flush_channels() to observe asynchronous failures and await persistence.
pub struct JsonlSink {
    destination: Mutex<Option<Destination>>,
    config: JsonlConfig,
    worker: OnceLock<io::Result<Worker>>,
    status: Arc<Status>,
}
impl JsonlSink {
    pub fn open_append(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::with_config(path, JsonlConfig::default())
    }
    /// Validates configuration only. File open/permission errors surface asynchronously via flush.
    pub fn with_config(path: impl AsRef<Path>, config: JsonlConfig) -> io::Result<Self> {
        Self::create(Destination::File(path.as_ref().to_owned()), config)
    }
    /// Custom writers use the same asynchronous path; size retention only applies to files.
    pub fn from_writer(writer: impl Write + Send + 'static) -> Self {
        Self::writer_with_config(writer, JsonlConfig::default()).expect("valid default config")
    }
    pub fn writer_with_config(
        writer: impl Write + Send + 'static,
        config: JsonlConfig,
    ) -> io::Result<Self> {
        if config.max_bytes.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "retention needs a file destination",
            ));
        }
        Self::create(Destination::Writer(Box::new(writer)), config)
    }
    fn create(destination: Destination, config: JsonlConfig) -> io::Result<Self> {
        Ok(Self {
            destination: Mutex::new(Some(destination)),
            config,
            worker: OnceLock::new(),
            status: Arc::new(Status::default()),
        })
    }
    pub fn is_started(&self) -> bool {
        self.worker.get().is_some()
    }
    /// Sticky first background error; inspecting it does not clear it.
    pub fn last_error(&self) -> Option<io::Error> {
        self.status
            .failed
            .load(Ordering::Acquire)
            .then(|| self.status.error())
    }
    pub fn stats(&self) -> JsonlStats {
        JsonlStats {
            records_written: self.status.records.load(Ordering::Relaxed),
            batches: self.status.batches.load(Ordering::Relaxed),
            flushes: self.status.flushes.load(Ordering::Relaxed),
            compactions: self.status.compactions.load(Ordering::Relaxed),
        }
    }
    fn start(&self) -> io::Result<&Worker> {
        let result = self.worker.get_or_init(|| {
            let destination = self
                .destination
                .lock()
                .take()
                .expect("single initialization");
            let config = self.config.clone();
            let status = self.status.clone();
            let (sender, receiver) = unbounded();
            match std::thread::Builder::new()
                .name("sun-observations-jsonl".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        writer::run(destination, config, &receiver, &status)
                    }));
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => status.fail(error),
                        Err(_) => status.fail(io::Error::other("JSONL writer panicked")),
                    }
                }) {
                Ok(join) => Ok(Worker {
                    sender: Some(sender),
                    join: Some(join),
                }),
                Err(error) => {
                    self.status.fail(error);
                    Err(self.status.error())
                }
            }
        });
        result.as_ref().map_err(|_| self.status.error())
    }
}
impl Sink for JsonlSink {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.write_shared(Arc::new(record.clone()))
    }
    fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()> {
        self.start()?.send(Message::Record(record), &self.status)
    }
    fn flush(&self) -> io::Result<()> {
        let Some(worker) = self.worker.get() else {
            return Ok(());
        };
        let worker = worker.as_ref().map_err(|_| self.status.error())?;
        let (sender, receiver) = unbounded();
        worker.send(Message::Flush(sender), &self.status)?;
        receiver.recv().map_err(|_| {
            while !self.status.failed.load(Ordering::Acquire)
                && !worker.join.as_ref().expect("live worker").is_finished()
            {
                std::thread::yield_now();
            }
            self.status.error()
        })?
    }
}
impl Drop for JsonlSink {
    fn drop(&mut self) {
        drop(self.worker.take());
        if let Some(error) = self.last_error() {
            eprintln!("SunObservations JSONL shutdown: {error}");
        }
    }
}

#[cfg(test)]
#[path = "jsonl_certification.rs"]
mod certification;
