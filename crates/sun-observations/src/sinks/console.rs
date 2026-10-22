use crate::{ObservationDelivery, ObservationRecord, Phase, Sink};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    thread::JoinHandle,
};

enum Destination {
    Stdout,
    Stderr,
    Writer(Box<dyn Write + Send>),
}
impl Destination {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Stdout => io::stdout().lock().write_all(bytes),
            Self::Stderr => io::stderr().lock().write_all(bytes),
            Self::Writer(writer) => writer.write_all(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout => io::stdout().lock().flush(),
            Self::Stderr => io::stderr().lock().flush(),
            Self::Writer(writer) => writer.flush(),
        }
    }
}

#[derive(Default)]
struct Status {
    failed: AtomicBool,
    failure: Mutex<Option<(io::ErrorKind, String)>>,
}
impl Status {
    fn fail(&self, error: io::Error) {
        let mut failure = self.failure.lock();
        if failure.is_none() {
            *failure = Some((error.kind(), error.to_string()));
        }
        self.failed.store(true, Ordering::Release);
    }
    fn error(&self) -> io::Error {
        self.failure.lock().as_ref().map_or_else(
            || io::Error::new(io::ErrorKind::BrokenPipe, "console worker stopped"),
            |(kind, message)| io::Error::new(*kind, message.clone()),
        )
    }
}

enum Message {
    Record(ObservationDelivery),
    Flush(Sender<()>),
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

/// Asynchronous human output, with one lazy FIFO worker and an unbounded producer queue.
/// Flush is a barrier; last-owner Drop drains locally. Background errors are sticky, not retried.
/// Custom text is literal, with a final newline added only when absent.
pub struct ConsoleSink {
    destination: Mutex<Option<Destination>>,
    worker: OnceLock<Result<Worker, ()>>,
    status: Arc<Status>,
}
impl ConsoleSink {
    pub fn stdout() -> Self {
        Self::create(Destination::Stdout)
    }
    pub fn stderr() -> Self {
        Self::create(Destination::Stderr)
    }
    /// Custom output uses exactly the production worker and barrier path.
    pub fn new(writer: impl Write + Send + 'static) -> Self {
        Self::create(Destination::Writer(Box::new(writer)))
    }
    fn create(destination: Destination) -> Self {
        Self {
            destination: Mutex::new(Some(destination)),
            worker: OnceLock::new(),
            status: Arc::new(Status::default()),
        }
    }
    pub fn is_started(&self) -> bool {
        self.worker.get().is_some()
    }
    pub fn last_error(&self) -> Option<io::Error> {
        self.status
            .failed
            .load(Ordering::Acquire)
            .then(|| self.status.error())
    }
    fn start(&self) -> io::Result<&Worker> {
        self.worker
            .get_or_init(|| {
                let destination = self
                    .destination
                    .lock()
                    .take()
                    .expect("single initialization");
                let status = self.status.clone();
                let (sender, receiver) = unbounded();
                match std::thread::Builder::new()
                    .name("sun-observations-console".into())
                    .spawn(move || {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            run(destination, &receiver)
                        }));
                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => status.fail(error),
                            Err(_) => status.fail(io::Error::other("console writer panicked")),
                        }
                    }) {
                    Ok(join) => Ok(Worker {
                        sender: Some(sender),
                        join: Some(join),
                    }),
                    Err(error) => {
                        self.status.fail(error);
                        Err(())
                    }
                }
            })
            .as_ref()
            .map_err(|_| self.status.error())
    }
}
impl Sink for ConsoleSink {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.write_shared(Arc::new(record.clone()))
    }
    fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()> {
        self.write_delivery(&ObservationDelivery::new(record, None))
    }
    fn wants_render_text(&self) -> bool {
        true
    }
    fn write_delivery(&self, delivery: &ObservationDelivery) -> io::Result<()> {
        self.start()?
            .send(Message::Record(delivery.clone()), &self.status)
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
        })
    }
}

fn format_delivery(bytes: &mut Vec<u8>, delivery: &ObservationDelivery) -> io::Result<()> {
    if let Some(text) = delivery.render_text() {
        bytes.extend_from_slice(text.as_bytes());
        if !text.ends_with('\n') {
            bytes.push(b'\n');
        }
        return Ok(());
    }
    let record = delivery.record();
    write!(bytes, "{} {}", record.ts, record.level)?;
    if let Some(scope) = &record.scope {
        write!(bytes, " [{scope}]")?;
    }
    write!(bytes, " {}", record.namespace)?;
    if let Some(phase) = record.phase {
        write!(
            bytes,
            " phase={}",
            match phase {
                Phase::Start => "start",
                Phase::End => "end",
            }
        )?;
    }
    if let Some(span) = &record.span {
        write!(bytes, " span={span}")?;
    }
    if let Some(message) = &record.message {
        bytes.extend_from_slice(b" message=");
        serde_json::to_writer(&mut *bytes, message).map_err(io::Error::other)?;
    }
    bytes.extend_from_slice(b" data=");
    serde_json::to_writer(&mut *bytes, &record.data).map_err(io::Error::other)?;
    bytes.push(b'\n');
    Ok(())
}

const MAX_BATCH_MESSAGES: usize = 1024;

fn run(mut destination: Destination, receiver: &Receiver<Message>) -> io::Result<()> {
    let mut bytes = Vec::with_capacity(8192);
    let mut messages = Vec::new();
    while let Ok(first) = receiver.recv() {
        let pending = receiver.len().min(MAX_BATCH_MESSAGES - 1);
        messages.push(first);
        messages.extend((0..pending).filter_map(|_| receiver.try_recv().ok()));
        for message in messages.drain(..) {
            match message {
                Message::Record(delivery) => format_delivery(&mut bytes, &delivery)?,
                Message::Flush(reply) => {
                    destination.write_all(&bytes)?;
                    bytes.clear();
                    destination.flush()?;
                    let _ = reply.send(());
                }
            }
        }
        // One locked stdout/stderr write per chunk keeps each multiline record indivisible.
        destination.write_all(&bytes)?;
        bytes.clear();
    }
    destination.flush()
}

#[cfg(test)]
#[path = "console_certification.rs"]
mod certification;
