use super::*;
use std::{fmt, marker::PhantomData};

/// Execution ordinal within one planned certification, independent of input equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CaseId(pub(crate) usize);

/// Retained successful positive execution, including evidence from locally failing checks.
pub struct AggregateCase<I, X: CertificationExecution> {
    pub(crate) id: CaseId,
    pub(crate) input: I,
    pub(crate) execution: X,
}
impl<I, X: CertificationExecution> AggregateCase<I, X> {
    pub fn id(&self) -> CaseId {
        self.id
    }
    pub fn input(&self) -> &I {
        &self.input
    }
    pub fn result(&self) -> &X::Output {
        self.execution
            .outcome()
            .unwrap_or_else(|_| unreachable!("retained positive execution"))
    }
    pub fn execution(&self) -> &X {
        &self.execution
    }
}
impl<I, X: CertificationExecution> fmt::Debug for AggregateCase<I, X> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AggregateCase")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Complete retained positive evidence from the executions actually explored.
/// Invalid positive execution evidence blocks access rather than selecting a successful subset.
pub struct CaseAggregate<I, X: CertificationExecution> {
    pub(crate) retained: Vec<AggregateCase<I, X>>,
    error: Option<AnalysisError>,
}
impl<I, X: CertificationExecution> CaseAggregate<I, X> {
    pub(crate) fn new() -> Self {
        Self {
            retained: vec![],
            error: None,
        }
    }
    pub(crate) fn invalidate(&mut self, id: CaseId, diagnostic: impl fmt::Display) {
        if self.error.is_none() {
            self.error = Some(AnalysisError {
                message: format!("positive {id:?}: {diagnostic}"),
            });
        }
    }
    pub fn cases(&self) -> Result<&[AggregateCase<I, X>], AnalysisError> {
        match &self.error {
            Some(e) => Err(e.clone()),
            None => Ok(&self.retained),
        }
    }
}

/// Symbolic post-exploration root; like CaseContext, it does not borrow the runner.
/// Per-case and aggregate plans cannot be registered in the opposite phase.
/// ```compile_fail
/// use sun_observations::certifications::*;
/// let mut runner = certification("wrong phase").executor(|_: &usize| Ok::<_, ()>(()));
/// let cases = runner.case_aggregate();
/// runner.check(fact!(cases.cases().unwrap().len()).expect(1usize));
/// ```
/// ```compile_fail
/// use sun_observations::certifications::*;
/// let mut runner = certification("wrong phase").executor(|n: &usize| Ok::<_, ()>(*n));
/// let case = runner.case();
/// runner.check_aggregate(fact!(*case.result()).expect(1usize));
/// ```
/// ```compile_fail
/// use sun_observations::certifications::*;
/// let runner = certification("mixed roots").executor(|n: &usize| Ok::<_, ()>(*n));
/// let case = runner.case();
/// let cases = runner.case_aggregate();
/// let _ = fact!(*case.input() + cases.cases().unwrap().len());
/// ```
pub struct CaseAggregateContext<I, X> {
    pub(crate) marker: PhantomData<fn(I) -> X>,
}
impl<I, X: CertificationExecution> CaseAggregateContext<I, X> {
    pub fn fact<F, V>(&self, source: &'static str, accessor: F) -> AggregateFact<I, X, F, V>
    where
        F: Fn(&CaseAggregate<I, X>) -> V,
    {
        AggregateFact {
            source,
            accessor,
            marker: PhantomData,
        }
    }
}

/// A deferred expression over the complete retained case evidence.
pub struct AggregateFact<I, X, F, V> {
    source: &'static str,
    accessor: F,
    marker: PhantomData<fn(I, X) -> V>,
}
/// Aggregate fact and acceptance policy, evaluated through the ordinary Fact boundary.
pub struct AggregateCheckPlan<I, X, F, V, E, K> {
    fact: AggregateFact<I, X, F, V>,
    expected: E,
    description: Option<String>,
    marker: PhantomData<K>,
}
/// Resolves aggregate expected evidence through the ordinary Fact acceptance boundary.
#[doc(hidden)]
pub trait AggregateExpectation<I, X: CertificationExecution, V, K> {
    type Expected: Expectation<V>;
    fn source(&self) -> Option<&str> {
        None
    }
    fn evaluate(
        &self,
        fact: Fact<V>,
        cases: &CaseAggregate<I, X>,
    ) -> CheckResult<V, <Self::Expected as Expectation<V>>::Violation>;
}
impl<I, X: CertificationExecution, V, E: Expectation<V>>
    AggregateExpectation<I, X, V, StaticExpectation> for E
{
    type Expected = E;
    fn evaluate(&self, fact: Fact<V>, _: &CaseAggregate<I, X>) -> CheckResult<V, E::Violation> {
        fact.expect(super::execution::BorrowExpectation(self))
    }
}
impl<I, X: CertificationExecution, V, F, E> AggregateExpectation<I, X, V, DeferredExpectation>
    for AggregateFact<I, X, F, E>
where
    F: Fn(&CaseAggregate<I, X>) -> E,
    E: Expectation<V>,
{
    type Expected = E;
    fn source(&self) -> Option<&str> {
        Some(self.source)
    }
    fn evaluate(&self, fact: Fact<V>, cases: &CaseAggregate<I, X>) -> CheckResult<V, E::Violation> {
        fact.expect((self.accessor)(cases))
    }
}
impl<I, X: CertificationExecution, F, V> AggregateFact<I, X, F, V>
where
    F: Fn(&CaseAggregate<I, X>) -> V,
{
    pub fn expect<E, K>(self, expected: E) -> AggregateCheckPlan<I, X, F, V, E, K>
    where
        E: AggregateExpectation<I, X, V, K>,
    {
        AggregateCheckPlan {
            fact: self,
            expected,
            description: None,
            marker: PhantomData,
        }
    }
}
impl<I, X, F, V, E, K> AggregateCheckPlan<I, X, F, V, E, K> {
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
/// Erased aggregate plan evaluated at the ordinary certification boundary.
#[doc(hidden)]
pub trait AggregateCheck<I, X: CertificationExecution> {
    fn evaluate(&self, cases: &CaseAggregate<I, X>, limit: usize) -> Option<CheckFailure>;
    fn description(&self) -> &str;
    fn expression(&self) -> String;
}
impl<I, X: CertificationExecution, F, V: fmt::Debug, E, K> AggregateCheck<I, X>
    for AggregateCheckPlan<I, X, F, V, E, K>
where
    F: Fn(&CaseAggregate<I, X>) -> V,
    E: AggregateExpectation<I, X, V, K>,
{
    fn evaluate(&self, cases: &CaseAggregate<I, X>, limit: usize) -> Option<CheckFailure> {
        let mut observed = fact(self.fact.source, (self.fact.accessor)(cases));
        if let Some(source) = self.expected.source() {
            observed = observed.details(format!("expected source: {source}"));
        }
        super::execution::finish_deferred_check(
            self.description(),
            self.expected.evaluate(observed, cases),
            limit,
        )
    }
    fn description(&self) -> &str {
        self.description.as_deref().unwrap_or(self.fact.source)
    }
    fn expression(&self) -> String {
        super::execution::check_expression(
            self.description(),
            self.fact.source,
            self.expected.source(),
        )
    }
}

#[cfg(test)]
#[path = "aggregate_certification.rs"]
mod certification;
