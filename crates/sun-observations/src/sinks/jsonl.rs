use crate::{ObservationRecord, Sink};
use parking_lot::Mutex;
use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::Path,
};

/// One lock covers an entire serialized line. File opening is append-only, not truncating.
/// Flush explicitly before consuming buffered output; flush is not an fsync guarantee.
pub struct JsonlSink {
    writer: Mutex<Box<dyn Write + Send>>,
}
impl JsonlSink {
    pub fn new(writer: impl Write + Send + 'static) -> Self {
        Self {
            writer: Mutex::new(Box::new(writer)),
        }
    }
    pub fn append(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(
            OpenOptions::new().create(true).append(true).open(path)?,
        ))
    }
}
impl Sink for JsonlSink {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
        line.push(b'\n');
        self.writer.lock().write_all(&line)
    }
    fn flush(&self) -> io::Result<()> {
        self.writer.lock().flush()
    }
}

#[cfg(test)]
#[path = "jsonl_test.rs"]
mod tests;
