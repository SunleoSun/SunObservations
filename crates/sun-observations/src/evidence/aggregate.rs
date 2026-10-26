use super::{
    CapturedRun, DecodeError, SpanError, SpanMeasurement, SpanMeasurements, SpanSources,
    TypedEntry, TypedView,
};
use crate::certifications::{
    collection, AggregateCase, AnalysisError, CaseAggregate, CaseId, CertificationExecution,
    PreparedSource, SourceProvenance,
};
use crate::Observation;
use serde::de::DeserializeOwned;
use std::{fmt, sync::Arc};

/// A blocked aggregate execution or canonical evidence failure attributed to its execution.
#[derive(Debug)]
pub enum AggregateError<E> {
    Execution(AnalysisError),
    Evidence { case: CaseId, error: E },
}
impl<E: fmt::Display> fmt::Display for AggregateError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Execution(e) => write!(f, "{}", e.message),
            Self::Evidence { case, error } => write!(f, "{case:?}: {error}"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for AggregateError<E> {}

/// One decoded evidence item associated with its retained positive execution.
/// The canonical per-execution view is shared, never re-decoded or re-paired by projections.
pub struct AggregateEntry<'a, I, X: CertificationExecution, E> {
    case: &'a AggregateCase<I, X>,
    evidence: Arc<E>,
    index: usize,
}
impl<I, X: CertificationExecution, E> AggregateEntry<'_, I, X, E> {
    pub fn case(&self) -> &AggregateCase<I, X> {
        self.case
    }
}
impl<I, X: CertificationExecution, E> fmt::Debug for AggregateEntry<'_, I, X, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AggregateEntry")
            .field("case", &self.case.id())
            .field("index", &self.index)
            .finish()
    }
}
impl<I, X: CertificationExecution, T> AggregateEntry<'_, I, X, TypedView<T>> {
    pub fn value(&self) -> &T {
        &self.evidence.values()[self.index]
    }
    pub fn observation(&self) -> TypedEntry<'_, T> {
        self.evidence.entry(self.index)
    }
}
impl<I, X: CertificationExecution, T> AggregateEntry<'_, I, X, SpanMeasurements<T>> {
    pub fn measurement(&self) -> &SpanMeasurement<T> {
        &self.evidence.values()[self.index]
    }
    pub fn sources(&self) -> &SpanSources {
        &self.evidence.sources()[self.index]
    }
}

/// Flattened canonical evidence in execution order, preserving case and source provenance.
pub struct AggregateEvidence<'a, I, X: CertificationExecution, E> {
    rows: Vec<AggregateEntry<'a, I, X, E>>,
}
impl<I, X: CertificationExecution, E: SourceProvenance> AggregateEvidence<'_, I, X, E> {
    pub fn values(&self) -> &[AggregateEntry<'_, I, X, E>] {
        &self.rows
    }
    pub fn source(
        &self,
        name: impl Into<Arc<str>>,
    ) -> PreparedSource<'_, AggregateEntry<'_, I, X, E>> {
        collection(name, &self.rows).with_provenance(self)
    }
}
impl<I, X: CertificationExecution, E: SourceProvenance> SourceProvenance
    for AggregateEvidence<'_, I, X, E>
{
    fn describe(&self, index: usize, out: &mut dyn fmt::Write) -> fmt::Result {
        let row = &self.rows[index];
        write!(out, " case={:?}", row.case.id())?;
        row.evidence.describe(row.index, out)
    }
}
/// Typed Observation rows associated with their originating captured executions.
pub type AggregateObservations<'a, I, O, E, T> =
    AggregateEvidence<'a, I, CapturedRun<Result<O, E>>, TypedView<T>>;
/// Canonically paired span rows associated with their originating captured executions.
pub type AggregateSpans<'a, I, O, E, T> =
    AggregateEvidence<'a, I, CapturedRun<Result<O, E>>, SpanMeasurements<T>>;

impl<I, O, E: fmt::Debug> CaseAggregate<I, CapturedRun<Result<O, E>>> {
    pub fn observations<T: Observation + DeserializeOwned>(
        &self,
    ) -> Result<AggregateObservations<'_, I, O, E, T>, AggregateError<DecodeError>> {
        let mut rows = vec![];
        for case in self.cases().map_err(AggregateError::Execution)? {
            let evidence = Arc::new(case.execution().to_typed::<T>().map_err(|error| {
                AggregateError::Evidence {
                    case: case.id(),
                    error,
                }
            })?);
            rows.extend((0..evidence.values().len()).map(|index| AggregateEntry {
                case,
                evidence: evidence.clone(),
                index,
            }));
        }
        Ok(AggregateEvidence { rows })
    }
    pub fn spans<T: Observation + DeserializeOwned>(
        &self,
    ) -> Result<AggregateSpans<'_, I, O, E, T>, AggregateError<SpanError>> {
        let mut rows = vec![];
        for case in self.cases().map_err(AggregateError::Execution)? {
            let evidence = Arc::new(case.execution().to_spans::<T>().map_err(|error| {
                AggregateError::Evidence {
                    case: case.id(),
                    error,
                }
            })?);
            rows.extend((0..evidence.values().len()).map(|index| AggregateEntry {
                case,
                evidence: evidence.clone(),
                index,
            }));
        }
        Ok(AggregateEvidence { rows })
    }
}
