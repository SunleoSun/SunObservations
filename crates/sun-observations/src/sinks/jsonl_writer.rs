use super::{Destination, JsonlConfig, Message, Status};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use std::{
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::Ordering,
    time::Instant,
};

struct FileWriter {
    path: PathBuf,
    output: Option<BufWriter<File>>,
    size: u64,
    last_line: u64,
}
enum Output {
    File(FileWriter),
    Writer(Box<dyn Write + Send>),
}
impl Output {
    fn open(destination: Destination) -> io::Result<Self> {
        Ok(match destination {
            Destination::Writer(writer) => Self::Writer(writer),
            Destination::File(path) => {
                let file = OpenOptions::new().create(true).append(true).open(&path)?;
                let size = file.metadata()?.len();
                Self::File(FileWriter {
                    path,
                    output: Some(BufWriter::with_capacity(64 * 1024, file)),
                    size,
                    last_line: 0,
                })
            }
        })
    }
    fn flush(&mut self, status: &Status) -> io::Result<()> {
        match self {
            Self::File(file) => file.output.as_mut().expect("open file").flush()?,
            Self::Writer(writer) => writer.flush()?,
        }
        status.flushes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn batch(
        &mut self,
        bytes: &mut Vec<u8>,
        records: &mut u64,
        last_line: u64,
        config: &JsonlConfig,
        status: &Status,
    ) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        match self {
            Self::Writer(writer) => writer.write_all(bytes)?,
            Self::File(file) => {
                file.output.as_mut().expect("open file").write_all(bytes)?;
                file.size += bytes.len() as u64;
                file.last_line = last_line;
                if let Some(max) = config.max_bytes {
                    if file.size > max {
                        file.compact(max, config.trim, status)?;
                    }
                }
            }
        }
        status.records.fetch_add(*records, Ordering::Relaxed);
        status.batches.fetch_add(1, Ordering::Relaxed);
        bytes.clear();
        *records = 0;
        Ok(())
    }
}
impl FileWriter {
    fn compact(&mut self, max: u64, trim: f64, status: &Status) -> io::Result<()> {
        let target = (max as f64 * (1.0 - trim)) as u64;
        let latest_start = self.size.saturating_sub(self.last_line);
        let approximate = self.size.saturating_sub(target).min(latest_start);
        if approximate == 0 {
            return Ok(());
        }
        self.output.as_mut().expect("open file").flush()?;
        let mut reader = BufReader::new(File::open(&self.path)?);
        reader.seek(SeekFrom::Start(approximate - 1))?;
        let mut discard = Vec::new();
        reader.read_until(b'\n', &mut discard)?;
        let cut = (approximate - 1 + discard.len() as u64).min(latest_start);
        reader.seek(SeekFrom::Start(cut))?;
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary
            .as_file()
            .set_permissions(reader.get_ref().metadata()?.permissions())?;
        io::copy(&mut reader, temporary.as_file_mut())?;
        temporary.flush()?;
        drop(reader);
        drop(self.output.take());
        let replacement = temporary.persist(&self.path).map_err(|error| error.error)?;
        drop(replacement);
        self.output = Some(BufWriter::with_capacity(
            64 * 1024,
            OpenOptions::new().append(true).open(&self.path)?,
        ));
        self.size -= cut;
        status.compactions.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

pub(super) fn run(
    destination: Destination,
    config: JsonlConfig,
    receiver: &Receiver<Message>,
    status: &Status,
) -> io::Result<()> {
    let mut output = Output::open(destination)?;
    let mut deadline = Instant::now() + config.interval;
    let mut bytes = Vec::with_capacity(64 * 1024);
    let mut messages = Vec::new();
    let mut records = 0;
    let mut last_line = 0;
    loop {
        let first = match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => {
                output.flush(status)?;
                deadline = Instant::now() + config.interval;
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                output.flush(status)?;
                return Ok(());
            }
        };
        // Snapshot queued messages after the first blocking receive. Drain that entire prefix
        // before writing; new arrivals cannot starve persistence or the periodic deadline.
        let pending = receiver.len();
        messages.push(first);
        messages.extend((0..pending).filter_map(|_| receiver.try_recv().ok()));
        for message in messages.drain(..) {
            match message {
                Message::Record(record) => {
                    let before = bytes.len();
                    serde_json::to_writer(&mut bytes, &*record).map_err(io::Error::other)?;
                    bytes.push(b'\n');
                    last_line = (bytes.len() - before) as u64;
                    records += 1;
                }
                Message::Flush(reply) => {
                    output.batch(&mut bytes, &mut records, last_line, &config, status)?;
                    output.flush(status)?;
                    deadline = Instant::now() + config.interval;
                    let _ = reply.send(Ok(()));
                }
            }
        }
        output.batch(&mut bytes, &mut records, last_line, &config, status)?;
        if Instant::now() >= deadline {
            output.flush(status)?;
            deadline = Instant::now() + config.interval;
        }
    }
}
