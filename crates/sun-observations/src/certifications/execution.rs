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
struct BorrowExpectation<'a, E>(&'a E);
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
        let mut c = certification("case");
        c.limits(DiagnosticLimits {
            failures: 1,
            characters_per_failure: limit,
        });
        c.check(self.description(), result);
        c.report().failures.into_iter().next()
    }
    fn description(&self) -> &str {
        self.description.as_deref().unwrap_or(self.fact.source)
    }
    fn expression(&self) -> String {
        match self.expected.source() {
            Some(source) => format!(
                "{}: {} expects {}",
                self.description(),
                self.fact.source,
                source
            ),
            None => format!("{}: {}", self.description(), self.fact.source),
        }
    }
}
/// Registration supports heterogeneous planned checks and already-evaluated eager checks.
#[doc(hidden)]
pub trait RegisterCheck<'a, I, X: CertificationExecution, F> {
    fn register(self, certification: &mut PlannedCertification<'a, I, X, F>);
}
impl<'a, I, X: CertificationExecution, F, A, V, E, K> RegisterCheck<'a, I, X, F>
    for CheckPlan<I, X, A, V, E, K>
where
    Self: CaseCheck<I, X> + 'a,
{
    fn register(self, c: &mut PlannedCertification<'a, I, X, F>) {
        c.checks.push(Box::new(self))
    }
}
impl<'a, I, X: CertificationExecution, F, T: fmt::Debug, V: fmt::Debug> RegisterCheck<'a, I, X, F>
    for CheckResult<T, V>
{
    fn register(self, c: &mut PlannedCertification<'a, I, X, F>) {
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

/// Real input execution with optional deterministic Domain exploration.
pub struct PlannedCertification<'a, I, X: CertificationExecution, F> {
    eager: Certification,
    executor: F,
    domain: Option<Box<dyn PlannedDomain<I> + 'a>>,
    domain_registered: bool,
    inputs: Vec<I>,
    checks: Vec<Box<dyn CaseCheck<I, X> + 'a>>,
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
            inputs: vec![],
            checks: vec![],
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
impl<'a, I, X: CertificationExecution, F: FnMut(&I) -> X> PlannedCertification<'a, I, X, F> {
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
    pub fn check<P: RegisterCheck<'a, I, X, F>>(&mut self, plan: P) -> &mut Self {
        plan.register(self);
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
            evaluation_error: None,
            stopped: false,
            coverage: CoverageReport::default(),
            checks: self
                .checks
                .iter()
                .map(|p| bounded_display(&p.expression(), 512))
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
        let mut coverage = CoverageTracker::default();
        if let Some(domain) = &compiled {
            coverage.catalog(domain.coverage_catalog());
        }
        let mut planner = compiled
            .as_deref()
            .map(|domain| CaseCoveragePlanner::new(domain, local_seed(self.seed, &self.identity)));
        let available = planner
            .as_ref()
            .map(|p| p.available)
            .unwrap_or([true, false, false]);
        let mut failures = FailureCoveragePlanner::new(self.failure_budget, available);
        for input in std::mem::take(&mut self.inputs) {
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
            if let Some((kind, check, diagnostic, class)) = execute_case(
                &mut self.executor,
                &input,
                &coordinate,
                &self.checks,
                self.diagnostic_bytes,
            ) {
                failures.retain(
                    &coordinate,
                    kind,
                    check,
                    diagnostic,
                    class,
                    self.diagnostic_bytes,
                );
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
                        if let Some((kind, check, diagnostic, class)) = execute_case(
                            &mut self.executor,
                            &input,
                            &coordinate,
                            &self.checks,
                            self.diagnostic_bytes,
                        ) {
                            failures.retain(
                                &coordinate,
                                kind,
                                check,
                                diagnostic,
                                class,
                                self.diagnostic_bytes,
                            );
                        }
                    }
                }
            }
        }
        report.stopped = failures.full();
        report.failures = failures.failures;
        report.coverage = coverage.report();
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
fn execute_case<I, X: CertificationExecution, F: FnMut(&I) -> X>(
    executor: &mut F,
    input: &I,
    coordinate: &CaseDomainCoordinate,
    checks: &[Box<dyn CaseCheck<I, X> + '_>],
    limit: usize,
) -> Option<(FailureKind, String, String, CheckClass)> {
    let execution = match case_unwind(|| executor(input)) {
        Ok(e) => e,
        Err(message) => {
            return Some((
                FailureKind::Panic,
                "executor".into(),
                format!("panic: {message}"),
                CheckClass::HardViolation,
            ))
        }
    };
    let evaluated = case_unwind(|| {
        if let Err(error) = execution.prerequisite() {
            return Some((
                FailureKind::Prerequisite,
                "execution completeness".into(),
                bounded_display(&error.message, limit),
                CheckClass::EvaluationError,
            ));
        }
        match (coordinate.violated.is_empty(), execution.outcome()) {
            (false, Err(_)) => None,
            (false, Ok(_)) => Some((
                FailureKind::AcceptedNegative,
                "rejection".into(),
                "invalid input accepted".into(),
                CheckClass::HardViolation,
            )),
            (true, Err(e)) => Some((
                FailureKind::RejectedPositive,
                "execution".into(),
                format!("valid input rejected: {}", bounded_debug(e, limit)),
                CheckClass::HardViolation,
            )),
            (true, Ok(_)) => {
                let case = TestCase {
                    input,
                    execution: &execution,
                };
                let mut first = None;
                let mut details = LimitedWriter::new(limit);
                for (index, check) in checks.iter().enumerate() {
                    let failure = match case_unwind(|| check.evaluate(&case, limit)) {
                        Ok(failure) => failure,
                        Err(message) => Some(CheckFailure {
                            section: String::new(),
                            name: check.description().into(),
                            diagnostic: format!("check evaluation panic: {message}"),
                            class: CheckClass::EvaluationError,
                        }),
                    };
                    if let Some(failure) = failure {
                        if first.is_none() {
                            first = Some((
                                FailureKind::Check(index),
                                failure.name.clone(),
                                failure.class,
                            ));
                        }
                        let _ = writeln!(details, "{}: {}", failure.name, failure.diagnostic);
                    }
                }
                first.map(|(kind, name, class)| (kind, name, details.finish(), class))
            }
        }
    });
    evaluated.unwrap_or_else(|message| {
        Some((
            FailureKind::Prerequisite,
            "execution evidence".into(),
            format!("evidence evaluation panic: {message}"),
            CheckClass::EvaluationError,
        ))
    })
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
        check: String,
        diagnostic: String,
        class: CheckClass,
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
            self.failures.push(CaseFailure {
                category: c.category(),
                coordinate: writer.finish(),
                check: bounded_display(&check, 256),
                diagnostic: bounded_display(&diagnostic, limit),
                class,
            });
        }
    }
}
/// One retained semantic representative, never an unbounded Input dump.
#[derive(Clone, Debug)]
pub struct CaseFailure {
    pub category: CaseCategory,
    pub coordinate: String,
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
    enums: BTreeSet<CaseDomainCoordinateSector>,
    pairs: BTreeSet<(CaseDomainCoordinateSector, CaseDomainCoordinateSector)>,
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
                    self.anchors.insert(anchor);
                }
                if self.catalog.enum_values.contains(&e.sector) {
                    self.enums.insert(e.sector.clone());
                    enums.push(e.sector.clone());
                }
            }
        }
        for (i, a) in enums.iter().enumerate() {
            for b in &enums[i + 1..] {
                let pair = (a.clone(), b.clone());
                if self.catalog.pairwise_combinations.contains(&pair) {
                    self.pairs.insert(pair);
                }
            }
        }
    }
    fn report(self) -> CoverageReport {
        CoverageReport {
            positive_anchors: (self.anchors.len(), self.catalog.positive_anchors.len()),
            enum_values: (self.enums.len(), self.catalog.enum_values.len()),
            pairwise_combinations: (self.pairs.len(), self.catalog.pairwise_combinations.len()),
            negative_sectors: (self.negative.len(), self.catalog.negative_sectors.len()),
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
    pub evaluation_error: Option<AnalysisError>,
    pub stopped: bool,
    pub coverage: CoverageReport,
    pub checks: Vec<String>,
    report_mode: bool,
    eager: CertificationReport,
}
impl PlannedCertificationReport {
    pub fn is_pass(&self) -> bool {
        self.evaluation_error.is_none()
            && self.failures.is_empty()
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
{}  check: {}
  {}",
                i + 1,
                fail.category,
                fail.coordinate,
                fail.check,
                fail.diagnostic
            )?;
        }
        if self.eager.failed() > 0 {
            write!(f, "{}", self.eager)?;
        }
        if self.report_mode && !self.checks.is_empty() {
            writeln!(f, "checks:")?;
            for check in &self.checks {
                writeln!(f, "  {check}")?;
            }
        }
        Ok(())
    }
}
