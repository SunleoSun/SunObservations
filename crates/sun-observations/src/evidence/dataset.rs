use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{error::Error, fmt, sync::Arc};
use sun_observations::certifications::*;
use sun_observations::{MemorySink, Observation, ObservationLevel, ObservationRecord, Phase};

/// Fixed membership and order over shared immutable canonical records.
#[derive(Clone, Debug)]
pub struct ObservationDataset {
    records: Arc<[Arc<ObservationRecord>]>,
}
impl ObservationDataset {
    pub fn from_shared(records: Vec<Arc<ObservationRecord>>) -> Self {
        Self {
            records: records.into(),
        }
    }
    pub fn from_memory(memory: &MemorySink) -> Self {
        Self::from_shared(memory.shared_records())
    }
    pub fn records(&self) -> &[Arc<ObservationRecord>] {
        &self.records
    }
    pub fn query<'a>(&'a self, query: EnvelopeQuery<'a>) -> RecordView<'a> {
        RecordView {
            dataset: self,
            query,
        }
    }
    pub fn typed<T: Observation + DeserializeOwned>(&self) -> Result<TypedView<T>, DecodeError> {
        self.query(EnvelopeQuery::default()).typed()
    }
}

/// Optional envelope selectors combine with AND; namespace trees respect segment boundaries.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvelopeQuery<'a> {
    pub namespace: Option<&'a str>,
    pub namespace_subtree: Option<&'a str>,
    /// Some(None) selects records outside a scope; None leaves scope unrestricted.
    pub scope: Option<Option<&'a str>>,
    pub level: Option<ObservationLevel>,
    /// Some(None) selects point records.
    pub phase: Option<Option<Phase>>,
    pub span: Option<Option<&'a str>>,
    pub proc: Option<&'a str>,
}
impl EnvelopeQuery<'_> {
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
    query: EnvelopeQuery<'a>,
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
    pub fn typed<T: Observation + DeserializeOwned>(&self) -> Result<TypedView<T>, DecodeError> {
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
        source_slice(name, &self.values).with_provenance(self)
    }
    pub fn group_by<K: Ord>(&self, key: impl Fn(&T) -> K) -> GroupedView<'_, T, K> {
        self.source("observations").group_by(key)
    }
    pub fn lag_relation_by<R: Observation>(
        &self,
        right: &TypedView<R>,
        left: impl Fn(&T) -> (i64, f64),
        rp: impl Fn(&R) -> (i64, f64),
        config: LagConfig,
    ) -> Result<LagReport, LagError>
    where
        T: Observation,
    {
        let mut report = lag_relation_by(self.values(), right.values(), left, rp, config)?;
        report.left_source = Some(T::NAMESPACE.into());
        report.right_source = Some(R::NAMESPACE.into());
        Ok(report)
    }
    pub fn values(&self) -> &[T] {
        &self.values
    }
    pub fn sources(&self) -> &[(usize, Arc<ObservationRecord>)] {
        &self.sources
    }
    pub fn numeric<'a, F: Fn(&T) -> f64 + Clone + 'a>(
        &'a self,
        project: F,
    ) -> NumericSeries<impl Iterator<Item = f64> + Clone + 'a> {
        NumericSeries::new(self.values.iter().map(project))
    }
}
impl<T: Observation> TypedView<T> {
    pub fn join_by<'a, R: Observation, K: Ord + Clone>(
        &'a self,
        right: &'a TypedView<R>,
        left_key: impl Fn(&T) -> K,
        right_key: impl Fn(&R) -> K,
    ) -> Join<'a, T, R, K> {
        let mut joined = join_by(&self.values, &right.values, left_key, right_key);
        joined.set_source_names(T::NAMESPACE, R::NAMESPACE);
        joined
    }
    pub fn join_u64_by<'a, R: Observation>(
        &'a self,
        right: &'a TypedView<R>,
        left_key: impl Fn(&T) -> u64,
        right_key: impl Fn(&R) -> u64,
    ) -> Join<'a, T, R, u64> {
        let mut joined = join_u64_by(&self.values, &right.values, left_key, right_key);
        joined.set_source_names(T::NAMESPACE, R::NAMESPACE);
        joined
    }
    pub fn join_sorted_by<'a, R: Observation, K: Ord + Clone>(
        &'a self,
        right: &'a TypedView<R>,
        left_key: impl Fn(&T) -> K,
        right_key: impl Fn(&R) -> K,
    ) -> Result<Join<'a, T, R, K>, JoinError> {
        let mut joined = join_sorted_by(&self.values, &right.values, left_key, right_key)?;
        joined.set_source_names(T::NAMESPACE, R::NAMESPACE);
        Ok(joined)
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
