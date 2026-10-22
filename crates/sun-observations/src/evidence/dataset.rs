use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{error::Error, fmt, sync::Arc};
use sun_observations::certifications::*;
use sun_observations::{MemorySink, Observation, ObservationLevel, ObservationRecord, Phase};

/// Rejected external record and its input position.
#[derive(Debug)]
pub struct IngestionError {
    pub index: usize,
    pub error: sun_observations::record::RecordValidationError,
}
impl fmt::Display for IngestionError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "record {}: {}", self.index, self.error)
    }
}
impl Error for IngestionError {}

/// Fixed membership and order over shared immutable canonical records.
#[derive(Clone, Debug)]
pub struct ObservationDataset {
    records: Arc<[Arc<ObservationRecord>]>,
}
impl ObservationDataset {
    pub(crate) fn from_shared(records: Vec<Arc<ObservationRecord>>) -> Self {
        Self {
            records: records.into(),
        }
    }
    pub fn try_from_records(records: Vec<Arc<ObservationRecord>>) -> Result<Self, IngestionError> {
        for (index, record) in records.iter().enumerate() {
            record
                .validate()
                .map_err(|error| IngestionError { index, error })?;
        }
        Ok(Self::from_shared(records))
    }
    pub fn try_from_memory(memory: &MemorySink) -> Result<Self, IngestionError> {
        Self::try_from_records(memory.shared_records())
    }
    pub fn records(&self) -> &[Arc<ObservationRecord>] {
        &self.records
    }
    pub fn query<'a>(&'a self, query: ObservationsQuery<'a>) -> RecordView<'a> {
        RecordView {
            dataset: self,
            query,
        }
    }
    pub fn to_typed<T: Observation + DeserializeOwned>(&self) -> Result<TypedView<T>, DecodeError> {
        self.query(ObservationsQuery::default()).to_typed()
    }
}

/// Observation selectors combine with AND; namespace trees respect segment boundaries.
/// Builders replace earlier intent for the same selector, including incompatible point/span intent.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObservationsQuery<'a> {
    namespace: Option<&'a str>,
    namespace_subtree: Option<&'a str>,
    /// Some(None) selects records outside a scope; None leaves scope unrestricted.
    scope: Option<Option<&'a str>>,
    level: Option<ObservationLevel>,
    /// Some(None) selects point records.
    phase: Option<Option<Phase>>,
    span: Option<Option<&'a str>>,
    proc: Option<&'a str>,
}
impl<'a> ObservationsQuery<'a> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn namespace(mut self, namespace: &'a str) -> Self {
        self.namespace = Some(namespace);
        self.namespace_subtree = None;
        self
    }
    pub fn namespace_subtree(mut self, namespace: &'a str) -> Self {
        self.namespace_subtree = Some(namespace);
        self.namespace = None;
        self
    }
    pub fn scope(mut self, scope: &'a str) -> Self {
        self.scope = Some(Some(scope));
        self
    }
    pub fn without_scope(mut self) -> Self {
        self.scope = Some(None);
        self
    }
    pub fn level(mut self, level: ObservationLevel) -> Self {
        self.level = Some(level);
        self
    }
    pub fn points(mut self) -> Self {
        self.phase = Some(None);
        self.span = Some(None);
        self
    }
    pub fn span_starts(mut self) -> Self {
        self.phase = Some(Some(Phase::Start));
        if self.span == Some(None) {
            self.span = None;
        }
        self
    }
    pub fn span_ends(mut self) -> Self {
        self.phase = Some(Some(Phase::End));
        if self.span == Some(None) {
            self.span = None;
        }
        self
    }
    pub fn span(mut self, span: &'a str) -> Self {
        self.span = Some(Some(span));
        if self.phase == Some(None) {
            self.phase = None;
        }
        self
    }
    pub fn without_span(self) -> Self {
        self.points()
    }
    pub fn process(mut self, process: &'a str) -> Self {
        self.proc = Some(process);
        self
    }
    fn matches(&self, r: &ObservationRecord) -> bool {
        self.namespace.is_none_or(|s| s == r.namespace)
            && self
                .namespace_subtree
                .is_none_or(|s| sun_observations::namespace::matches_subtree(s, &r.namespace))
            && self.scope.is_none_or(|s| s == r.scope.as_deref())
            && self.level.is_none_or(|s| s == r.level)
            && self.phase.is_none_or(|s| s == r.phase)
            && self.span.is_none_or(|s| s == r.span.as_deref())
            && self.proc.is_none_or(|s| s == r.proc)
    }
}

/// A lazy envelope selection, preserving original dataset indices and shared identity.
pub struct RecordView<'a> {
    dataset: &'a ObservationDataset,
    query: ObservationsQuery<'a>,
}
impl RecordView<'_> {
    pub fn iter(&self) -> impl Iterator<Item = (usize, &Arc<ObservationRecord>)> + Clone {
        self.dataset
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| self.query.matches(r))
    }
    /// Decodes points and span starts of T's namespace; lifecycle ends have no typed payload.
    pub fn to_typed<T: Observation + DeserializeOwned>(&self) -> Result<TypedView<T>, DecodeError> {
        let mut values = Vec::new();
        let mut sources = Vec::new();
        for (index, record) in self
            .iter()
            .filter(|(_, r)| r.namespace == T::NAMESPACE && r.phase != Some(Phase::End))
        {
            let value = T::deserialize(&record.data).map_err(|e| DecodeError {
                source: Box::new(SourceContext::new(index, record)),
                target_type: std::any::type_name::<T>().into(),
                message: e.to_string(),
            })?;
            values.push(value);
            sources.push((index, Arc::clone(record)));
        }
        Ok(TypedView { values, sources })
    }
}

/// Source coordinates for locating a failed decode in a snapshot or persisted trace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceContext {
    pub dataset_index: usize,
    pub namespace: String,
    pub proc: String,
    pub scope: Option<String>,
    pub span: Option<String>,
    pub phase: Option<Phase>,
    pub ts: String,
    pub proc_ms: u64,
}
impl SourceContext {
    fn new(index: usize, r: &ObservationRecord) -> Self {
        Self {
            dataset_index: index,
            namespace: r.namespace.clone(),
            proc: r.proc.clone(),
            scope: r.scope.clone(),
            span: r.span.clone(),
            phase: r.phase,
            ts: r.ts.clone(),
            proc_ms: r.proc_ms,
        }
    }
}
/// A hard payload/schema failure, distinct from statistical suspicion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "decode_failure")]
pub struct DecodeError {
    pub source: Box<SourceContext>,
    pub target_type: String,
    pub message: String,
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "decode {} at record {}: {}",
            self.source.namespace, self.source.dataset_index, self.message
        )
    }
}
impl Error for DecodeError {}

/// A decoded value and its immutable canonical evidence; all references borrow the prepared view.
#[derive(Debug)]
pub struct TypedEntry<'a, T> {
    pub value: &'a T,
    pub dataset_index: usize,
    pub source: &'a Arc<ObservationRecord>,
}

/// Reusable decoded values and their original shared records; operations never deserialize again.
#[derive(Debug)]
pub struct TypedView<T> {
    values: Vec<T>,
    sources: Vec<(usize, Arc<ObservationRecord>)>,
}
impl<T> TypedView<T> {
    /// Borrow decoded values with their original dataset coordinates, without allocating.
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = TypedEntry<'_, T>> + DoubleEndedIterator + Clone {
        (0..self.values.len()).map(|index| self.entry(index))
    }
    pub(crate) fn entry(&self, index: usize) -> TypedEntry<'_, T> {
        let (dataset_index, source) = &self.sources[index];
        TypedEntry {
            value: &self.values[index],
            dataset_index: *dataset_index,
            source,
        }
    }
    pub fn source(&self, name: impl Into<Arc<str>>) -> PreparedSource<'_, T> {
        collection(name, &self.values).with_provenance(self)
    }

    pub fn values(&self) -> &[T] {
        &self.values
    }
    pub fn sources(&self) -> &[(usize, Arc<ObservationRecord>)] {
        &self.sources
    }
}

impl<T> SourceProvenance for TypedView<T> {
    fn describe(&self, index: usize, out: &mut dyn fmt::Write) -> fmt::Result {
        let (dataset_index, r) = &self.sources[index];
        write!(
            out,
            " dataset_index={dataset_index} proc={} scope={:?} namespace={} span={:?} ts={}",
            r.proc, r.scope, r.namespace, r.span, r.ts
        )
    }
}
#[cfg(test)]
#[path = "dataset_certification.rs"]
mod tests;
