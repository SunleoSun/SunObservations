use super::*;
use super::{
    certification::{bounded_display, LimitedWriter},
    domain::compilation::{coordinate_key, local_seed},
};
use std::{
    collections::BTreeSet,
    fmt::{self, Write},
    marker::PhantomData,
    panic::{catch_unwind, AssertUnwindSafe},
};

/// Rejection-capable execution evidence. Adapters own completeness prerequisites.
pub trait CertificationExecution {
    type Output;
    type Error: fmt::Debug;
    fn outcome(&self) -> Result<&Self::Output, &Self::Error>;
    fn prerequisite(&self) -> Result<(), AnalysisError> {
        Ok(())
    }
}
impl<O, E: fmt::Debug> CertificationExecution for Result<O, E> {
    type Output = O;
    type Error = E;
    fn outcome(&self) -> Result<&O, &E> {
        self.as_ref()
    }
}
/// Borrowed input and successful execution evidence for a deferred check.
/// Observation access is provided only by the evidence adapter for captured executions.
/// ```compile_fail
/// use sun_observations::certifications::*;
/// use sun_observations::Observation;
/// #[derive(serde::Serialize,serde::Deserialize,Observation)]
/// #[observation(namespace="plain::event")]
/// struct Event { count: usize }
/// let mut c = certification("plain").executor(|_: &usize| Ok::<_, ()>(1usize));
/// let case = c.case();
/// c.check(fact!(case.observations::<Event>().unwrap().values().len()).expect(1));
/// ```
pub struct TestCase<'a, I, X: CertificationExecution> {
    pub(crate) input: &'a I,
    pub(crate) execution: &'a X,
}
impl<I, X: CertificationExecution> TestCase<'_, I, X> {
    pub fn input(&self) -> &I {
        self.input
    }
    pub fn result(&self) -> &X::Output {
        self.execution
            .outcome()
            .unwrap_or_else(|_| unreachable!("checks only run for successful positive cases"))
    }
    pub fn execution(&self) -> &X {
        self.execution
    }
}
/// A typed accessor evaluated anew for each successful positive execution.
pub struct PlannedFact<I, X, F, V> {
    source: &'static str,
    accessor: F,
    marker: PhantomData<fn(I, X) -> V>,
}

/// Type context for the symbolic case root; it never borrows the runner.
pub struct CaseContext<I, X> {
    marker: PhantomData<fn(I) -> X>,
}
impl<I, X: CertificationExecution> CaseContext<I, X> {
    pub fn fact<F, V>(&self, source: &'static str, accessor: F) -> PlannedFact<I, X, F, V>
    where
        F: Fn(&TestCase<'_, I, X>) -> V,
    {
        PlannedFact {
            source,
            accessor,
            marker: PhantomData,
        }
    }
}
/// A typed deferred Fact → Expectation plan; erasure occurs only at registration.
pub struct CheckPlan<I, X, F, V, E, K> {
    fact: PlannedFact<I, X, F, V>,
    expected: E,
    description: Option<String>,
    marker: PhantomData<K>,
}
#[doc(hidden)]
pub struct StaticExpectation;
#[doc(hidden)]
pub struct DeferredExpectation;
pub(super) struct BorrowExpectation<'a, E>(pub(super) &'a E);
impl<T, E: Expectation<T>> Expectation<T> for BorrowExpectation<'_, E> {
    type Violation = E::Violation;
    fn assess(&self, v: &T) -> Result<Vec<Self::Violation>, AnalysisError> {
        self.0.assess(v)
    }
    fn describe(&self) -> String {
        self.0.describe()
    }
    fn class(&self) -> ViolationClass {
        self.0.class()
    }
}
/// Resolves expected evidence without introducing another acceptance engine.
#[doc(hidden)]
pub trait PlanExpectation<I, X: CertificationExecution, V, K> {
    type Expected: Expectation<V>;
    fn source(&self) -> Option<&str> {
        None
    }
    fn evaluate(
        &self,
        value: Fact<V>,
        case: &TestCase<'_, I, X>,
    ) -> CheckResult<V, <Self::Expected as Expectation<V>>::Violation>;
}
impl<I, X: CertificationExecution, V, E: Expectation<V>> PlanExpectation<I, X, V, StaticExpectation>
    for E
{
    type Expected = E;
    fn evaluate(&self, value: Fact<V>, _: &TestCase<'_, I, X>) -> CheckResult<V, E::Violation> {
        value.expect(BorrowExpectation(self))
    }
}
impl<I, X: CertificationExecution, V, F, E> PlanExpectation<I, X, V, DeferredExpectation>
    for PlannedFact<I, X, F, E>
where
    F: Fn(&TestCase<'_, I, X>) -> E,
    E: Expectation<V>,
{
    type Expected = E;
    fn source(&self) -> Option<&str> {
        Some(self.source)
    }
    fn evaluate(&self, value: Fact<V>, case: &TestCase<'_, I, X>) -> CheckResult<V, E::Violation> {
        value.expect((self.accessor)(case))
    }
}
impl<I, X: CertificationExecution, F, V> PlannedFact<I, X, F, V>
where
    F: Fn(&TestCase<'_, I, X>) -> V,
{
    pub fn expect<E, K>(self, expected: E) -> CheckPlan<I, X, F, V, E, K>
    where
        E: PlanExpectation<I, X, V, K>,
    {
        CheckPlan {
            fact: self,
            expected,
            description: None,
            marker: PhantomData,
        }
    }
}
impl<I, X, F, V, E, K> CheckPlan<I, X, F, V, E, K> {
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
trait CaseCheck<I, X: CertificationExecution> {
    fn evaluate(&self, case: &TestCase<'_, I, X>, limit: usize) -> Option<CheckFailure>;
    fn description(&self) -> &str;
    fn expression(&self) -> String;
}
impl<I, X: CertificationExecution, F, V: fmt::Debug, E, K> CaseCheck<I, X>
    for CheckPlan<I, X, F, V, E, K>
where
    F: Fn(&TestCase<'_, I, X>) -> V,
    E: PlanExpectation<I, X, V, K>,
{
    fn evaluate(&self, case: &TestCase<'_, I, X>, limit: usize) -> Option<CheckFailure> {
        let observed = (self.fact.accessor)(case);
        let mut observed = fact(self.fact.source, observed);
        if let Some(source) = self.expected.source() {
            observed = observed.details(format!("expected source: {source}"));
        }
        let result = self.expected.evaluate(observed, case);
        finish_deferred_check(self.description(), result, limit)
    }
    fn description(&self) -> &str {
        self.description.as_deref().unwrap_or(self.fact.source)
    }
    fn expression(&self) -> String {
        check_expression(self.description(), self.fact.source, self.expected.source())
    }
}
pub(super) fn finish_deferred_check<T: fmt::Debug, V: fmt::Debug>(
    name: &str,
    result: CheckResult<T, V>,
    limit: usize,
) -> Option<CheckFailure> {
    let mut c = certification("deferred check");
    c.limits(DiagnosticLimits {
        failures: 1,
        characters_per_failure: limit,
    });
    c.check(name, result);
    c.report().failures.into_iter().next()
}
pub(super) fn check_expression(description: &str, source: &str, expected: Option<&str>) -> String {
    match expected {
        Some(expected) => format!("{description}: {source} expects {expected}"),
        None => format!("{description}: {source}"),
    }
}

/// Registration supports heterogeneous planned checks and already-evaluated eager checks.
#[doc(hidden)]
pub trait RegisterCheck<'a, I, X: CertificationExecution, F, S = fn(&I, u64) -> X> {
    fn register(self, certification: &mut PlannedCertification<'a, I, X, F, S>);
}
impl<'a, I, X: CertificationExecution, F, S, A, V, E, K> RegisterCheck<'a, I, X, F, S>
    for CheckPlan<I, X, A, V, E, K>
where
    Self: CaseCheck<I, X> + 'a,
{
    fn register(self, c: &mut PlannedCertification<'a, I, X, F, S>) {
        c.checks.push(RegisteredCheck {
            section: c.eager.section_path().into(),
            plan: Box::new(self),
        })
    }
}
impl<'a, I, X: CertificationExecution, F, S, T: fmt::Debug, V: fmt::Debug>
    RegisterCheck<'a, I, X, F, S> for CheckResult<T, V>
{
    fn register(self, c: &mut PlannedCertification<'a, I, X, F, S>) {
        let name = self.source().to_string();
        c.eager.check(name, self);
    }
}

trait PlannedDomain<I> {
    fn target_counts(&self) -> [usize; 3];
    fn classify(&self, input: &I) -> Result<CaseDomainCoordinate, AnalysisError>;
    fn next(
        &self,
        category: usize,
        cursor: &mut usize,
        seed: u64,
        occupied: &BTreeSet<Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>>,
    ) -> Result<Option<(I, CaseDomainCoordinate)>, AnalysisError>;
    fn coverage_catalog(&self) -> &domain::compilation::DomainCoverageCatalog;
}

impl<I: DomainInput> PlannedDomain<I> for CompiledDomain<I> {
    fn target_counts(&self) -> [usize; 3] {
        CompiledDomain::target_counts(self)
    }

    fn classify(&self, input: &I) -> Result<CaseDomainCoordinate, AnalysisError> {
        CompiledDomain::classify(self, input)
    }

    fn next(
        &self,
        category: usize,
        cursor: &mut usize,
        seed: u64,
        occupied: &BTreeSet<Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>>,
    ) -> Result<Option<(I, CaseDomainCoordinate)>, AnalysisError> {
        CompiledDomain::next(self, category, cursor, seed, occupied)
    }

    fn coverage_catalog(&self) -> &domain::compilation::DomainCoverageCatalog {
        CompiledDomain::coverage_catalog(self)
    }
}

/// Executor arity is kept distinct while sharing all outcome and evidence handling.
enum Executor<F, S> {
    Ordinary(F),
    Seeded(S),
}

#[cfg(test)]
#[path = "execution_certification.rs"]
mod certification;

/// A deferred check bound to its registration-time semantic section.
struct RegisteredCheck<'a, I, X: CertificationExecution> {
    section: String,
    plan: Box<dyn CaseCheck<I, X> + 'a>,
}

/// A post-exploration plan bound to its registration-time semantic section.
struct RegisteredAggregateCheck<'a, I, X: CertificationExecution> {
    section: String,
    plan: Box<dyn AggregateCheck<I, X> + 'a>,
}

/// The deferred execution phase of a registered check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckPhase {
    Case,
    Aggregate,
}

/// A registered expression and its semantic section, independent of case execution.
#[derive(Clone, Debug)]
pub struct PlannedCheckReport {
    pub phase: CheckPhase,
    pub section: String,
    pub expression: String,
}

/// Real input execution with optional deterministic Domain exploration.
pub struct PlannedCertification<'a, I, X: CertificationExecution, F, S = fn(&I, u64) -> X> {
    eager: Certification,
    executor: Executor<F, S>,
    domain: Option<Box<dyn PlannedDomain<I> + 'a>>,
    domain_registered: bool,
    pairwise_coverage_required: bool,
    inputs: Vec<I>,
    checks: Vec<RegisteredCheck<'a, I, X>>,
    aggregate_checks: Vec<RegisteredAggregateCheck<'a, I, X>>,
    budget: usize,
    failure_budget: usize,
    seed: u64,
    identity: u64,
    report_mode: bool,
    config_error: Option<AnalysisError>,
    diagnostic_bytes: usize,
}
impl Certification {
    pub fn executor<'a, I, X: CertificationExecution, F: FnMut(&I) -> X>(
        self,
        executor: F,
    ) -> PlannedCertification<'a, I, X, F> {
        self.with_executor(Executor::Ordinary(executor))
    }

    /// Supplies a deterministic case-local seed, independent of execution order.
    /// Generated seeds use canonical Domain coordinates; real seeds use data-source ordinals.
    pub fn executor_with_seed<'a, I, X: CertificationExecution, F: FnMut(&I, u64) -> X>(
        self,
        executor: F,
    ) -> PlannedCertification<'a, I, X, fn(&I) -> X, F> {
        self.with_executor(Executor::Seeded(executor))
    }

    fn with_executor<'a, I, X: CertificationExecution, F, S>(
        self,
        executor: Executor<F, S>,
    ) -> PlannedCertification<'a, I, X, F, S> {
        let mut config_error = None;
        fn setting(name: &str, default: u64, error: &mut Option<AnalysisError>) -> u64 {
            match std::env::var(name) {
                Ok(v) => match v.parse() {
                    Ok(n) => n,
                    Err(_) => {
                        *error = Some(domain_error(format!("invalid {name}")));
                        default
                    }
                },
                Err(std::env::VarError::NotPresent) => default,
                Err(_) => {
                    *error = Some(domain_error(format!("invalid {name}")));
                    default
                }
            }
        }
        let budget = setting("SUN_CERT_CASES_BUDGET", 64, &mut config_error);
        let seed = setting("SUN_CERT_SEED", 184729361, &mut config_error);
        let identity = self.identity;
        PlannedCertification {
            eager: self,
            executor,
            domain: None,
            domain_registered: false,
            pairwise_coverage_required: false,
            inputs: vec![],
            checks: vec![],
            aggregate_checks: vec![],
            budget: usize::try_from(budget).unwrap_or(usize::MAX),
            failure_budget: 5,
            seed,
            identity,
            report_mode: std::env::var("SUN_CERT_REPORT").is_ok_and(|v| v == "1"),
            config_error,
            diagnostic_bytes: 2048,
        }
    }
}
impl<'a, I, X: CertificationExecution, F: FnMut(&I) -> X, S: FnMut(&I, u64) -> X>
    PlannedCertification<'a, I, X, F, S>
{
    pub fn data_source(&mut self, inputs: impl IntoIterator<Item = I>) -> &mut Self {
        self.inputs.extend(inputs);
        self
    }
    pub fn domain(&mut self, domain: Domain<I>) -> &mut Self
    where
        I: DomainInput + 'a,
    {
        if self.domain_registered {
            self.config_error = Some(domain_error("Domain already registered"));
            return self;
        }
        self.domain_registered = true;
        match domain.compile() {
            Ok(domain) => self.domain = Some(Box::new(domain)),
            Err(error) if self.config_error.is_none() => self.config_error = Some(error),
            Err(_) => {}
        }
        self
    }
    /// Requires every feasible enum value and pairwise combination within the normal budgets.
    /// A missing Domain or a catalog without pairwise combinations cannot satisfy this requirement.
    pub fn require_pairwise_coverage(&mut self) -> &mut Self {
        self.pairwise_coverage_required = true;
        self
    }
    pub fn cases_budget(&mut self, budget: usize) -> &mut Self {
        self.budget = budget;
        self
    }
    pub fn failure_budget(&mut self, budget: usize) -> &mut Self {
        self.failure_budget = budget;
        self
    }
    pub fn seed(&mut self, seed: u64) -> &mut Self {
        self.seed = seed;
        self
    }
    pub fn report_mode(&mut self, enabled: bool) -> &mut Self {
        self.report_mode = enabled;
        self
    }
    pub fn diagnostic_bytes(&mut self, bytes: usize) -> &mut Self {
        self.diagnostic_bytes = bytes;
        self
    }
    pub fn case(&self) -> CaseContext<I, X> {
        CaseContext {
            marker: PhantomData,
        }
    }
    pub fn case_aggregate(&self) -> CaseAggregateContext<I, X> {
        CaseAggregateContext {
            marker: PhantomData,
        }
    }
    pub fn check_aggregate<P: AggregateCheck<I, X> + 'a>(&mut self, plan: P) -> &mut Self {
        self.aggregate_checks.push(RegisteredAggregateCheck {
            section: self.eager.section_path().into(),
            plan: Box::new(plan),
        });
        self
    }
    pub fn check<P: RegisterCheck<'a, I, X, F, S>>(&mut self, plan: P) -> &mut Self {
        plan.register(self);
        self
    }
    /// Registers checks under a nested semantic path; execution remains deferred.
    pub fn section(
        &mut self,
        name: impl Into<String>,
        checks: impl FnOnce(&mut Self),
    ) -> &mut Self {
        let child = self.eager.section_child(name.into());
        let parent = std::mem::replace(&mut self.eager, child);
        let start = self.checks.len();
        let aggregate_start = self.aggregate_checks.len();
        let result = catch_unwind(AssertUnwindSafe(|| checks(self)));
        let child = std::mem::replace(&mut self.eager, parent);
        match result {
            Ok(()) => self.eager.merge_section(child),
            Err(panic) => {
                self.checks.truncate(start);
                self.aggregate_checks.truncate(aggregate_start);
                std::panic::resume_unwind(panic);
            }
        }
        self
    }

    pub fn report(mut self) -> PlannedCertificationReport {
        let mut report = PlannedCertificationReport {
            name: self.eager.report().name,
            seed: self.seed,
            budget: self.budget,
            executed: [0; 2],
            categories: [0; 3],
            failure_budget: self.failure_budget,
            failures: vec![],
            aggregate_failures: vec![],
            evaluation_error: None,
            stopped: false,
            coverage: CoverageReport::default(),
            checks: self
                .checks
                .iter()
                .map(|p| PlannedCheckReport {
                    phase: CheckPhase::Case,
                    section: p.section.clone(),
                    expression: bounded_display(&p.plan.expression(), 512),
                })
                .chain(self.aggregate_checks.iter().map(|p| PlannedCheckReport {
                    phase: CheckPhase::Aggregate,
                    section: p.section.clone(),
                    expression: bounded_display(&p.plan.expression(), 512),
                }))
                .collect(),
            report_mode: self.report_mode,
            eager: self.eager.report(),
        };
        if let Some(error) = self.config_error.take() {
            report.evaluation_error = Some(error);
            return report;
        }
        if self.failure_budget == 0 {
            report.evaluation_error = Some(domain_error("failure budget must be positive"));
            return report;
        }
        let no_generated_capacity = self.domain_registered && self.budget <= self.inputs.len();
        let compiled = self.domain.take();
        if self.inputs.is_empty() && compiled.is_none() {
            report.evaluation_error = Some(domain_error("no input cases or Domain"));
            return report;
        }
        let mut aggregate = CaseAggregate::new();
        let retain = !self.aggregate_checks.is_empty();
        let mut coverage = CoverageTracker::default();
        if let Some(domain) = &compiled {
            coverage.catalog(domain.coverage_catalog());
        }
        let certification_seed = local_seed(self.seed, &self.identity);
        let mut planner = compiled
            .as_deref()
            .map(|domain| CaseCoveragePlanner::new(domain, certification_seed));
        let available = planner
            .as_ref()
            .map(|p| p.available)
            .unwrap_or([true, false, false]);
        let mut failures = FailureCoveragePlanner::new(self.failure_budget, available);
        for (ordinal, input) in std::mem::take(&mut self.inputs).into_iter().enumerate() {
            if failures.full() {
                break;
            }
            let coordinate = match &mut planner {
                Some(p) => match p.real_input(&input) {
                    Ok(c) => c,
                    Err(error) => {
                        report.evaluation_error = Some(error);
                        break;
                    }
                },
                None => CaseDomainCoordinate {
                    entries: vec![],
                    violated: vec![],
                },
            };
            coverage.record(&coordinate);
            report.executed[0] += 1;
            report.categories[coordinate.category().index()] += 1;
            if let Some((kind, checks)) = execute_case(
                &mut self.executor,
                input,
                || local_seed(certification_seed, &("data_source", ordinal)),
                &coordinate,
                &self.checks,
                self.diagnostic_bytes,
                retain.then_some((CaseId(report.cases_executed() - 1), &mut aggregate)),
            ) {
                failures.retain(&coordinate, kind, checks, self.diagnostic_bytes);
            }
        }
        if no_generated_capacity && report.evaluation_error.is_none() {
            report.evaluation_error = Some(domain_error("No budget for domain generations"));
        }
        if let Some(planner) = &mut planner {
            let capacity = self.budget.saturating_sub(report.executed[0]);
            while report.executed[1] < capacity
                && !failures.full()
                && report.evaluation_error.is_none()
            {
                match planner.next(&mut failures) {
                    Err(error) => {
                        report.evaluation_error = Some(error);
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some((input, coordinate))) => {
                        coverage.record(&coordinate);
                        report.executed[1] += 1;
                        report.categories[coordinate.category().index()] += 1;
                        if let Some((kind, checks)) = execute_case(
                            &mut self.executor,
                            input,
                            || local_seed(certification_seed, &coordinate_key(&coordinate)),
                            &coordinate,
                            &self.checks,
                            self.diagnostic_bytes,
                            retain.then_some((CaseId(report.cases_executed() - 1), &mut aggregate)),
                        ) {
                            failures.retain(&coordinate, kind, checks, self.diagnostic_bytes);
                        }
                    }
                }
            }
        }
        for check in &self.aggregate_checks {
            let failure =
                match case_unwind(|| check.plan.evaluate(&aggregate, self.diagnostic_bytes)) {
                    Ok(failure) => failure,
                    Err(message) => Some(CheckFailure {
                        section: String::new(),
                        name: bounded_display(&check.plan.description(), 256),
                        diagnostic: bounded_display(
                            &format!("aggregate evaluation panic: {message}"),
                            self.diagnostic_bytes,
                        ),
                        class: CheckClass::EvaluationError,
                    }),
                };
            if let Some(mut failure) = failure {
                failure.section = check.section.clone();
                report.aggregate_failures.push(failure);
            }
        }
        report.stopped = failures.full();
        report.failures = failures.failures;
        report.coverage = coverage.report();
        if self.pairwise_coverage_required && report.evaluation_error.is_none() {
            let (covered, total) = report.coverage.pairwise_combinations;
            let (values, total_values) = report.coverage.enum_values;
            if total == 0 {
                report.evaluation_error = Some(domain_error(
                    "required pairwise coverage needs a Domain with feasible pairwise combinations (total > 0)",
                ));
            } else if covered != total || values != total_values {
                report.evaluation_error = Some(domain_error(format!(
                    "required pairwise coverage not reached: {covered} / {total}; enum values: {values} / {total_values}",
                )));
            }
        }
        report
    }
    #[track_caller]
    pub fn assert_certify(self) {
        self.report().assert_certify();
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FailureKind {
    RejectedPositive,
    AcceptedNegative,
    Panic,
    Prerequisite,
    Check(usize),
}
fn execute_case<I, X: CertificationExecution, F: FnMut(&I) -> X, S: FnMut(&I, u64) -> X>(
    executor: &mut Executor<F, S>,
    input: I,
    seed: impl FnOnce() -> u64,
    coordinate: &CaseDomainCoordinate,
    checks: &[RegisteredCheck<'_, I, X>],
    limit: usize,
    mut aggregate: Option<(CaseId, &mut CaseAggregate<I, X>)>,
) -> Option<(FailureKind, Vec<CheckFailure>)> {
    let positive = coordinate.violated.is_empty();
    let single = |kind, name: &str, diagnostic, class| {
        Some((
            kind,
            vec![CheckFailure {
                section: String::new(),
                name: bounded_display(&name, 256),
                diagnostic: bounded_display(&diagnostic, limit),
                class,
            }],
        ))
    };
    let execution = match case_unwind(|| match executor {
        Executor::Ordinary(executor) => executor(&input),
        Executor::Seeded(executor) => executor(&input, seed()),
    }) {
        Ok(e) => e,
        Err(message) => {
            if positive {
                if let Some((id, a)) = &mut aggregate {
                    a.invalidate(*id, &message);
                }
            }
            return single(
                FailureKind::Panic,
                "executor",
                format!("panic: {message}"),
                CheckClass::HardViolation,
            );
        }
    };
    let mut eligible = false;
    let evaluated = case_unwind(|| {
        if let Err(error) = execution.prerequisite() {
            return single(
                FailureKind::Prerequisite,
                "execution completeness",
                bounded_display(&error.message, limit),
                CheckClass::EvaluationError,
            );
        }
        match (positive, execution.outcome()) {
            (false, Err(_)) => None,
            (false, Ok(_)) => single(
                FailureKind::AcceptedNegative,
                "rejection",
                "invalid input accepted".into(),
                CheckClass::HardViolation,
            ),
            (true, Err(e)) => single(
                FailureKind::RejectedPositive,
                "execution",
                format!("valid input rejected: {}", bounded_debug(e, limit)),
                CheckClass::HardViolation,
            ),
            (true, Ok(_)) => {
                eligible = true;
                let case = TestCase {
                    input: &input,
                    execution: &execution,
                };
                let mut first = None;
                let mut failures = Vec::new();
                let mut remaining = limit;
                for (index, check) in checks.iter().enumerate() {
                    let failure = match case_unwind(|| check.plan.evaluate(&case, remaining)) {
                        Ok(failure) => failure,
                        Err(message) => Some(CheckFailure {
                            section: String::new(),
                            name: bounded_display(&check.plan.description(), 256),
                            diagnostic: bounded_display(
                                &format!("check evaluation panic: {message}"),
                                remaining,
                            ),
                            class: CheckClass::EvaluationError,
                        }),
                    };
                    if let Some(mut failure) = failure {
                        failure.section = check.section.clone();
                        remaining = remaining.saturating_sub(failure.diagnostic.len());
                        first.get_or_insert(FailureKind::Check(index));
                        failures.push(failure);
                    }
                }
                first.map(|kind| (kind, failures))
            }
        }
    });
    let failure = evaluated.unwrap_or_else(|message| {
        eligible = false;
        single(
            FailureKind::Prerequisite,
            "execution evidence",
            format!("evidence evaluation panic: {message}"),
            CheckClass::EvaluationError,
        )
    });
    if positive {
        if let Some((id, a)) = aggregate {
            if eligible {
                a.retained.push(AggregateCase {
                    id,
                    input,
                    execution,
                });
            } else {
                a.invalidate(
                    id,
                    failure
                        .as_ref()
                        .map(|(_, checks)| {
                            checks
                                .iter()
                                .map(|c| c.diagnostic.as_str())
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                        .unwrap_or_else(|| "invalid execution evidence".into()),
                );
            }
        }
    }
    failure
}
thread_local! {static CASE_PANIC_DEPTH:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
fn case_unwind<T>(operation: impl FnOnce() -> T) -> Result<T, String> {
    static HOOK: std::sync::Once = std::sync::Once::new();
    // Suppression is thread-local; unrelated panics retain the previously installed hook.
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !CASE_PANIC_DEPTH.with(|d| d.get() > 0) {
                previous(info)
            }
        }));
    });
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            CASE_PANIC_DEPTH.with(|d| d.set(d.get() - 1));
        }
    }
    CASE_PANIC_DEPTH.with(|d| d.set(d.get() + 1));
    let _guard = Guard;
    catch_unwind(AssertUnwindSafe(operation)).map_err(|p| {
        if let Some(s) = p.downcast_ref::<String>() {
            bounded_display(s, 1024)
        } else if let Some(s) = p.downcast_ref::<&str>() {
            bounded_display(s, 1024)
        } else {
            "non-string panic".into()
        }
    })
}
/// Per-certification input coverage, shared by real-input classification and generated exploration.
struct CaseCoveragePlanner<'a, I> {
    domain: &'a dyn PlannedDomain<I>,
    cursors: [usize; 3],
    occupied: BTreeSet<Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>>,
    scheduler: WeightedScheduler,
    available: [bool; 3],
    seed: u64,
}
impl<'a, I> CaseCoveragePlanner<'a, I> {
    fn new(domain: &'a dyn PlannedDomain<I>, seed: u64) -> Self {
        let counts = domain.target_counts();
        Self {
            domain,
            cursors: [0; 3],
            occupied: BTreeSet::new(),
            scheduler: WeightedScheduler::default(),
            available: std::array::from_fn(|i| counts[i] > 0),
            seed,
        }
    }
    fn real_input(&mut self, input: &I) -> Result<CaseDomainCoordinate, AnalysisError> {
        let coordinate = self.domain.classify(input)?;
        self.occupied.insert(coordinate_key(&coordinate));
        Ok(coordinate)
    }
    fn next(
        &mut self,
        failures: &mut FailureCoveragePlanner,
    ) -> Result<Option<(I, CaseDomainCoordinate)>, AnalysisError> {
        while !failures.full() {
            let allowed = std::array::from_fn(|i| {
                self.available[i] && !failures.saturated(i, &self.available)
            });
            let Some(category) = self
                .scheduler
                .next(allowed)
                .or_else(|| self.scheduler.next(self.available))
            else {
                return Ok(None);
            };
            match self.domain.next(
                category,
                &mut self.cursors[category],
                self.seed,
                &self.occupied,
            )? {
                Some((input, coordinate)) => {
                    self.occupied.insert(coordinate_key(&coordinate));
                    return Ok(Some((input, coordinate)));
                }
                None => {
                    self.available[category] = false;
                    failures.available = self.available;
                }
            }
        }
        Ok(None)
    }
}
const CATEGORY_WEIGHTS: [u128; 3] = [100, 60, 36];
#[derive(Default)]
pub(crate) struct WeightedScheduler {
    served: [usize; 3],
}
impl WeightedScheduler {
    pub(crate) fn next(&mut self, available: [bool; 3]) -> Option<usize> {
        let weights = CATEGORY_WEIGHTS;
        let mut best = None;
        for i in 0..3 {
            if available[i]
                && best.is_none_or(|b| {
                    self.served[i] as u128 * weights[b] < self.served[b] as u128 * weights[i]
                })
            {
                best = Some(i)
            }
        }
        if let Some(i) = best {
            self.served[i] += 1;
        }
        best
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FailureKey {
    category: CaseCategory,
    sectors: Vec<CaseDomainCoordinateSector>,
    kind: FailureKind,
}
/// Distinct semantic failures drive temporary category saturation and the hard execution stop.
struct FailureCoveragePlanner {
    budget: usize,
    available: [bool; 3],
    counts: [usize; 3],
    keys: BTreeSet<FailureKey>,
    failures: Vec<CaseFailure>,
}
impl FailureCoveragePlanner {
    fn new(budget: usize, available: [bool; 3]) -> Self {
        Self {
            budget,
            available,
            counts: [0; 3],
            keys: BTreeSet::new(),
            failures: vec![],
        }
    }
    fn full(&self) -> bool {
        self.failures.len() >= self.budget
    }
    fn saturated(&self, i: usize, available: &[bool; 3]) -> bool {
        let weights = CATEGORY_WEIGHTS;
        let total: u128 = (0..3).filter(|i| available[*i]).map(|i| weights[i]).sum();
        total > 0 && self.counts[i] as u128 >= (self.budget as u128 * weights[i]).div_ceil(total)
    }
    fn retain(
        &mut self,
        c: &CaseDomainCoordinate,
        kind: FailureKind,
        checks: Vec<CheckFailure>,
        limit: usize,
    ) {
        let index = c.category().index();
        if self.full() || self.saturated(index, &self.available) {
            return;
        }
        let key = FailureKey {
            category: c.category(),
            sectors: if c.violated.len() > 1 {
                c.violated
                    .iter()
                    .map(|id| CaseDomainCoordinateSector {
                        constraint: *id,
                        region: DomainRegion::Relation,
                    })
                    .collect()
            } else {
                c.entries.iter().map(|e| e.sector.clone()).collect()
            },
            kind,
        };
        if self.keys.insert(key) {
            self.counts[index] += 1;
            let mut writer = LimitedWriter::new(limit);
            for e in &c.entries {
                let _ = writeln!(
                    writer,
                    "  {} = {} [{:?} / {:?}]",
                    e.name, e.value, e.sector.region, e.slot
                );
            }
            let first = &checks[0];
            let check = first.name.clone();
            let class = first.class;
            let mut details = LimitedWriter::new(limit);
            for failure in &checks {
                let _ = writeln!(
                    details,
                    "{} {}: {}",
                    failure.section, failure.name, failure.diagnostic
                );
            }
            self.failures.push(CaseFailure {
                category: c.category(),
                coordinate: writer.finish(),
                check: bounded_display(&check, 256),
                diagnostic: details.finish(),
                class,
                checks,
            });
        }
    }
}
/// One retained semantic representative, never an unbounded Input dump.
#[derive(Clone, Debug)]
pub struct CaseFailure {
    pub category: CaseCategory,
    pub coordinate: String,
    /// Independent check failures retain their own section paths, even when summary text is bounded.
    pub checks: Vec<CheckFailure>,
    pub check: String,
    pub diagnostic: String,
    pub class: CheckClass,
}
/// Compact coverage counters; totals describe semantic opportunities, not executed cases.
#[derive(Clone, Debug, Default)]
pub struct CoverageReport {
    pub positive_anchors: (usize, usize),
    pub enum_values: (usize, usize),
    pub pairwise_combinations: (usize, usize),
    pub negative_sectors: (usize, usize),
}
/// Occupied positions projected onto the compiled Domain's feasible coverage catalog.
#[derive(Default)]
struct CoverageTracker {
    anchors: BTreeSet<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>,
    enums: BTreeSet<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>,
    pairs: BTreeSet<(
        (CaseDomainCoordinateSector, CaseDomainCoordinateSlot),
        (CaseDomainCoordinateSector, CaseDomainCoordinateSlot),
    )>,
    negative: BTreeSet<CaseDomainCoordinateSector>,
    catalog: domain::compilation::DomainCoverageCatalog,
}
impl CoverageTracker {
    fn catalog(&mut self, catalog: &domain::compilation::DomainCoverageCatalog) {
        self.catalog = catalog.clone();
    }
    fn record(&mut self, c: &CaseDomainCoordinate) {
        let mut enums = vec![];
        for e in &c.entries {
            if c.violated.contains(&e.sector.constraint) {
                if self.catalog.negative_sectors.contains(&e.sector) {
                    self.negative.insert(e.sector.clone());
                }
            } else {
                let anchor = (e.sector.clone(), e.slot.clone());
                if self.catalog.positive_anchors.contains(&anchor) {
                    self.anchors.insert(anchor.clone());
                }
                if self.catalog.enum_values.contains(&anchor) {
                    self.enums.insert(anchor.clone());
                    enums.push((e.path.clone(), anchor));
                }
            }
        }
        for (i, (path_a, a)) in enums.iter().enumerate() {
            let count_a = enums.iter().filter(|(p, _)| p == path_a).count();
            for (path_b, b) in &enums[i + 1..] {
                if path_a == path_b || a.0.constraint == b.0.constraint {
                    continue;
                }
                let count_b = enums.iter().filter(|(p, _)| p == path_b).count();
                if count_a > 1 && count_b > 1 {
                    continue;
                }
                let pair = if a <= b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                self.pairs.insert(pair);
            }
        }
    }
    fn report(self) -> CoverageReport {
        CoverageReport {
            positive_anchors: (
                self.anchors.len(),
                self.catalog.positive_anchors.len().max(self.anchors.len()),
            ),
            enum_values: (
                self.enums.len(),
                self.catalog.enum_values.len().max(self.enums.len()),
            ),
            pairwise_combinations: (
                self.pairs.len(),
                self.catalog.pairwise_total.max(self.pairs.len()),
            ),
            negative_sectors: (
                self.negative.len(),
                self.catalog.negative_sectors.len().max(self.negative.len()),
            ),
        }
    }
}
/// Immutable multi-case proof. Display and assertion remain silent on ordinary success.
#[derive(Clone, Debug)]
pub struct PlannedCertificationReport {
    pub name: String,
    pub seed: u64,
    pub budget: usize,
    pub executed: [usize; 2],
    pub categories: [usize; 3],
    pub failure_budget: usize,
    pub failures: Vec<CaseFailure>,
    pub aggregate_failures: Vec<CheckFailure>,
    pub evaluation_error: Option<AnalysisError>,
    pub stopped: bool,
    pub coverage: CoverageReport,
    pub checks: Vec<PlannedCheckReport>,
    report_mode: bool,
    eager: CertificationReport,
}
impl PlannedCertificationReport {
    pub fn is_pass(&self) -> bool {
        self.evaluation_error.is_none()
            && self.failures.is_empty()
            && self.aggregate_failures.is_empty()
            && self.eager.failed() == 0
            && self.executed.iter().sum::<usize>() > 0
    }
    pub fn cases_executed(&self) -> usize {
        self.executed.iter().sum()
    }
    #[track_caller]
    pub fn assert_certify(&self) {
        if !self.is_pass() {
            panic!("{self}")
        }
        if self.report_mode {
            eprint!("{self}")
        }
    }
}
impl fmt::Display for PlannedCertificationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_pass() && !self.report_mode {
            return Ok(());
        }
        writeln!(
            f,
            "CERTIFICATION {}: {}",
            if self.is_pass() { "REPORT" } else { "FAILED" },
            self.name
        )?;
        writeln!(
            f,
            "status: {}; seed: {}",
            if self.is_pass() { "passed" } else { "failed" },
            self.seed
        )?;
        writeln!(
            f,
            "cases executed: {} / budget {}
  data_source: {}
  generated: {}",
            self.cases_executed(),
            self.budget,
            self.executed[0],
            self.executed[1]
        )?;
        if self.stopped {
            writeln!(
                f,
                "stopped: failure budget reached ({}/{})",
                self.failures.len(),
                self.failure_budget
            )?;
        }
        if let Some(error) = &self.evaluation_error {
            writeln!(
                f,
                "EvaluationError: {}",
                bounded_display(&error.message, 2048)
            )?;
        }
        let c = &self.coverage;
        writeln!(
            f,
            "coverage{}:
  positive anchors: {}/{}
  enum values: {}/{}
  pairwise combinations: {}/{}
  negative sectors: {}/{}",
            if self.stopped { " before stop" } else { "" },
            c.positive_anchors.0,
            c.positive_anchors.1,
            c.enum_values.0,
            c.enum_values.1,
            c.pairwise_combinations.0,
            c.pairwise_combinations.1,
            c.negative_sectors.0,
            c.negative_sectors.1
        )?;
        writeln!(f, "failures: {}", self.failures.len())?;
        for (i, fail) in self.failures.iter().enumerate() {
            writeln!(
                f,
                "{}. {:?}
{}  check: {}",
                i + 1,
                fail.category,
                fail.coordinate,
                fail.check,
            )?;
            for check in &fail.checks {
                writeln!(
                    f,
                    "  {} [{:?}] {}\n    {}",
                    check.section, check.class, check.name, check.diagnostic
                )?;
            }
        }
        for check in &self.aggregate_failures {
            writeln!(
                f,
                "aggregate {} [{:?}] {}\n    {}",
                check.section, check.class, check.name, check.diagnostic
            )?;
        }
        if self.eager.failed() > 0 {
            write!(f, "{}", self.eager)?;
        }
        if self.report_mode && !self.checks.is_empty() {
            writeln!(f, "checks:")?;
            for check in &self.checks {
                writeln!(
                    f,
                    "  {:?} {} {}",
                    check.phase, check.section, check.expression
                )?;
            }
        }
        Ok(())
    }
}
