use super::{
    CaptureStatus, CapturedRun, DecodeError, ObservationDataset, RecordView, SourceContext,
};
use crate::certifications::{collection, PreparedSource, SourceProvenance};
use crate::{Observation, ObservationRecord, Phase};
use serde::de::DeserializeOwned;
use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

/// Stable lifecycle identity shared by one span start/end pair.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpanIdentity {
    process: String,
    scope: Option<String>,
    namespace: String,
    span: String,
}
impl SpanIdentity {
    pub fn process(&self) -> &str {
        &self.process
    }
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn span(&self) -> &str {
        &self.span
    }
}

/// A decoded span start with canonical lifecycle identity and checked elapsed nanoseconds.
#[derive(Debug)]
pub struct SpanMeasurement<T> {
    identity: SpanIdentity,
    value: T,
    duration_ns: u64,
}
impl<T> SpanMeasurement<T> {
    pub fn identity(&self) -> &SpanIdentity {
        &self.identity
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn duration_ns(&self) -> u64 {
        self.duration_ns
    }
    pub fn duration(&self) -> Duration {
        Duration::from_nanos(self.duration_ns)
    }
}

/// Both original lifecycle records and their coordinates in the input snapshot.
#[derive(Debug)]
pub struct SpanSources {
    pub start_index: usize,
    pub end_index: usize,
    pub start: Arc<ObservationRecord>,
    pub end: Arc<ObservationRecord>,
}

/// Owned derived evidence, ordered by original start index; preparation never pairs or decodes again.
#[derive(Debug)]
pub struct SpanMeasurements<T> {
    values: Vec<SpanMeasurement<T>>,
    sources: Vec<SpanSources>,
}
impl<T> SpanMeasurements<T> {
    pub fn values(&self) -> &[SpanMeasurement<T>] {
        &self.values
    }
    pub fn sources(&self) -> &[SpanSources] {
        &self.sources
    }
    pub fn source(&self, name: impl Into<Arc<str>>) -> PreparedSource<'_, SpanMeasurement<T>> {
        collection(name, &self.values).with_provenance(self)
    }
}
impl<T> SourceProvenance for SpanMeasurements<T> {
    fn describe(&self, index: usize, out: &mut dyn fmt::Write) -> fmt::Result {
        let s = &self.sources[index];
        write!(out, " start_index={} end_index={} proc={} scope={:?} namespace={} span={:?} start_proc_ns={} end_proc_ns={}",
            s.start_index, s.end_index, s.start.proc, s.start.scope, s.start.namespace, s.start.span, s.start.proc_ns, s.end.proc_ns)
    }
}

/// Invalid or incomplete lifecycle evidence is never omitted from a profiling sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanLifecycleError {
    MissingStart,
    MissingEnd,
    DuplicateStart,
    DuplicateEnd,
    ReversedTime,
    LevelMismatch,
}
/// Pairing, capture, or payload failure with original evidence coordinates.
#[derive(Debug)]
pub enum SpanError {
    IncompleteCapture(CaptureStatus),
    Lifecycle {
        kind: SpanLifecycleError,
        source: Box<SourceContext>,
        related: Option<Box<SourceContext>>,
    },
    Decode(DecodeError),
}
impl fmt::Display for SpanError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompleteCapture(status) => write!(out, "incomplete capture: {status:?}"),
            Self::Decode(error) => error.fmt(out),
            Self::Lifecycle {
                kind,
                source,
                related,
            } => write!(out, "span {kind:?}: {source:?}; related={related:?}"),
        }
    }
}
impl std::error::Error for SpanError {}

type RecordCoordinate<'a> = (usize, &'a Arc<ObservationRecord>);
/// Endpoints accumulate independently of delivery order.
#[derive(Default)]
struct SpanPair<'a> {
    start: Option<RecordCoordinate<'a>>,
    end: Option<RecordCoordinate<'a>>,
}
fn lifecycle_error(
    kind: SpanLifecycleError,
    source: RecordCoordinate<'_>,
    related: Option<RecordCoordinate<'_>>,
) -> SpanError {
    SpanError::Lifecycle {
        kind,
        source: Box::new(SourceContext::new(source.0, source.1)),
        related: related.map(|(index, record)| Box::new(SourceContext::new(index, record))),
    }
}
impl ObservationDataset {
    pub fn to_spans<T: Observation + DeserializeOwned>(
        &self,
    ) -> Result<SpanMeasurements<T>, SpanError> {
        self.query(super::ObservationsQuery::new()).to_spans()
    }
}
impl RecordView<'_> {
    /// Requires exactly one start/end per selected identity, including fan-out duplicates.
    /// Phase filters that remove one side fail closed. Points are not measurements.
    pub fn to_spans<T: Observation + DeserializeOwned>(
        &self,
    ) -> Result<SpanMeasurements<T>, SpanError> {
        let mut identities = HashMap::new();
        let mut pairs: Vec<SpanPair<'_>> = Vec::new();
        for (index, record) in self
            .iter_namespace(Some(T::NAMESPACE))
            .filter(|(_, record)| record.namespace == T::NAMESPACE && record.phase.is_some())
        {
            let key = (&record.proc, &record.scope, &record.namespace, &record.span);
            let pair_index = *identities.entry(key).or_insert_with(|| {
                pairs.push(SpanPair::default());
                pairs.len() - 1
            });
            let pair = &mut pairs[pair_index];
            let (slot, duplicate) = match record.phase {
                Some(Phase::Start) => (&mut pair.start, SpanLifecycleError::DuplicateStart),
                Some(Phase::End) => (&mut pair.end, SpanLifecycleError::DuplicateEnd),
                None => unreachable!(),
            };
            if let Some(previous) = *slot {
                return Err(lifecycle_error(duplicate, (index, record), Some(previous)));
            }
            *slot = Some((index, record));
        }
        let mut completed = Vec::with_capacity(pairs.len());
        for pair in pairs {
            let (start, end) = match (pair.start, pair.end) {
                (Some(start), Some(end)) => (start, end),
                (Some(start), None) => {
                    return Err(lifecycle_error(SpanLifecycleError::MissingEnd, start, None))
                }
                (None, Some(end)) => {
                    return Err(lifecycle_error(SpanLifecycleError::MissingStart, end, None))
                }
                (None, None) => unreachable!(),
            };
            if start.1.level != end.1.level {
                return Err(lifecycle_error(
                    SpanLifecycleError::LevelMismatch,
                    end,
                    Some(start),
                ));
            }
            let ns = end.1.proc_ns.checked_sub(start.1.proc_ns).ok_or_else(|| {
                lifecycle_error(SpanLifecycleError::ReversedTime, end, Some(start))
            })?;
            let value = T::deserialize(&start.1.data).map_err(|error| {
                SpanError::Decode(DecodeError {
                    source: Box::new(SourceContext::new(start.0, start.1)),
                    target_type: std::any::type_name::<T>().into(),
                    message: error.to_string(),
                })
            })?;
            completed.push((
                SpanMeasurement {
                    identity: SpanIdentity {
                        process: start.1.proc.clone(),
                        scope: start.1.scope.clone(),
                        namespace: start.1.namespace.clone(),
                        span: start
                            .1
                            .span
                            .clone()
                            .expect("validated span start has lifecycle identity"),
                    },
                    value,
                    duration_ns: ns,
                },
                SpanSources {
                    start_index: start.0,
                    end_index: end.0,
                    start: Arc::clone(start.1),
                    end: Arc::clone(end.1),
                },
            ));
        }
        completed.sort_unstable_by_key(|(_, sources)| sources.start_index);
        let (values, sources) = completed.into_iter().unzip();
        Ok(SpanMeasurements { values, sources })
    }
}
impl<R> CapturedRun<R> {
    /// Capture health is checked before lifecycle interpretation, even when retained pairs look complete.
    pub fn to_spans<T: Observation + DeserializeOwned>(
        &self,
    ) -> Result<SpanMeasurements<T>, SpanError> {
        if !self.status().is_complete() {
            return Err(SpanError::IncompleteCapture(self.status()));
        }
        self.dataset().to_spans()
    }
}

impl<I, O, E: fmt::Debug> crate::certifications::TestCase<'_, I, CapturedRun<Result<O, E>>> {
    pub fn spans<T: Observation + DeserializeOwned>(
        &self,
    ) -> Result<SpanMeasurements<T>, SpanError> {
        self.execution.to_spans()
    }
}

#[cfg(test)]
#[path = "spans_certification.rs"]
mod certification;
