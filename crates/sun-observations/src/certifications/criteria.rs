use crate::certifications::*;
use std::fmt;

/// Acceptance classification; analysis/precondition errors are not failed expectations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckClass {
    Pass,
    HardViolation,
    StatisticalViolation,
    EvaluationError,
}
/// Only failed-acceptance classes; evaluation failures are owned by Fact.
///
/// ```compile_fail
/// use sun_observations::certifications::*;
/// let _ = criterion("invalid", CheckClass::Pass, |_: &usize| false);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViolationClass {
    Hard,
    Statistical,
}
impl From<ViolationClass> for CheckClass {
    fn from(value: ViolationClass) -> Self {
        match value {
            ViolationClass::Hard => Self::HardViolation,
            ViolationClass::Statistical => Self::StatisticalViolation,
        }
    }
}
/// A preparation/analysis error blocking this proof, without blocking independent proofs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AnalysisError {
    pub message: String,
}
/// Full typed observed evidence, kept separate from acceptance.
#[derive(Debug)]
pub struct Fact<T> {
    pub source: String,
    pub observed: Result<T, AnalysisError>,
    pub details: String,
}
pub fn fact<T>(source: impl Into<String>, observed: T) -> Fact<T> {
    Fact {
        source: source.into(),
        observed: Ok(observed),
        details: String::new(),
    }
}
pub fn try_fact<T, E: fmt::Display>(source: impl Into<String>, observed: Result<T, E>) -> Fact<T> {
    Fact {
        source: source.into(),
        observed: observed.map_err(|e| AnalysisError {
            message: crate::certifications::certification::bounded_display(&e, 2048),
        }),
        details: String::new(),
    }
}
impl<T> Fact<T> {
    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = details.into();
        self
    }
    pub fn expect_valid(self) -> CheckResult<T, <AllValid as Expectation<T>>::Violation>
    where
        AllValid: Expectation<T>,
    {
        self.expect(AllValid)
    }

    pub fn expect<E: Expectation<T>>(self, expected: E) -> CheckResult<T, E::Violation> {
        let mut violations = Vec::new();
        let observed = self.observed;
        let mut analysis_error = None;
        let mut class = CheckClass::Pass;
        if let Ok(value) = &observed {
            match expected.assess(value) {
                Ok(v) => {
                    violations = v;
                    if !violations.is_empty() {
                        class = expected.class().into();
                    }
                }
                Err(e) => {
                    analysis_error = Some(e);
                    class = CheckClass::EvaluationError;
                }
            }
        } else {
            class = CheckClass::EvaluationError;
        }
        CheckResult {
            source: self.source,
            expected: expected.describe(),
            observed,
            violations,
            class,
            analysis_error,
            details: self.details,
        }
    }
}
/// A reusable scenario-owned criterion. All acceptance paths use this boundary.
pub trait Expectation<T> {
    type Violation: fmt::Debug;
    fn assess(&self, observed: &T) -> Result<Vec<Self::Violation>, AnalysisError>;
    fn describe(&self) -> String;
    fn class(&self) -> ViolationClass {
        ViolationClass::Hard
    }
}
/// Immutable acceptance result; aggregation consumes it without copying observed evidence.
///
/// ```compile_fail
/// use sun_observations::certifications::*;
/// let mut result = fact("value", 1).expect(2);
/// result.class = CheckClass::Pass;
/// ```
#[derive(Debug)]
pub struct CheckResult<T, V = Violation> {
    pub(crate) source: String,
    pub(crate) expected: String,
    pub(crate) observed: Result<T, AnalysisError>,
    pub(crate) violations: Vec<V>,
    pub(crate) class: CheckClass,
    pub(crate) analysis_error: Option<AnalysisError>,
    pub(crate) details: String,
}
impl<T, V> CheckResult<T, V> {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn expected(&self) -> &str {
        &self.expected
    }
    pub fn observed(&self) -> &Result<T, AnalysisError> {
        &self.observed
    }
    pub fn violations(&self) -> &[V] {
        &self.violations
    }
    pub fn class(&self) -> CheckClass {
        self.class
    }
    pub fn analysis_error(&self) -> Option<&AnalysisError> {
        self.analysis_error.as_ref()
    }
    pub fn details(&self) -> &str {
        &self.details
    }
    pub fn is_pass(&self) -> bool {
        self.class == CheckClass::Pass
    }
}
/// Equality requires only PartialEq, including structured or collection values.
#[derive(Clone, Debug)]
pub(crate) struct Equal<T>(pub T);
#[derive(Clone, Debug)]
pub struct NotEqual<T>(pub T);
impl<T: PartialEq + fmt::Debug> Expectation<T> for Equal<T> {
    type Violation = Violation;
    fn assess(&self, v: &T) -> Result<Vec<Violation>, AnalysisError> {
        Ok(if v == &self.0 {
            vec![]
        } else {
            vec![Violation {
                expected: self.describe(),
            }]
        })
    }
    fn describe(&self) -> String {
        format!("equal {}", bounded_debug(&self.0, 512))
    }
}
impl<T: PartialEq + fmt::Debug> Expectation<T> for NotEqual<T> {
    type Violation = Violation;
    fn assess(&self, v: &T) -> Result<Vec<Violation>, AnalysisError> {
        Ok(if v != &self.0 {
            vec![]
        } else {
            vec![Violation {
                expected: self.describe(),
            }]
        })
    }
    fn describe(&self) -> String {
        format!("not equal {}", bounded_debug(&self.0, 512))
    }
}
/// Ordinary value criteria. Bounds are inclusive; incomparable values fail closed.
#[derive(Clone, Debug)]
pub struct Criterion<T>(CriterionKind<T>);
#[derive(Clone, Debug)]
enum CriterionKind<T> {
    Greater(T),
    AtLeast(T),
    Less(T),
    AtMost(T),
    Range(InclusiveRange<T>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub expected: String,
}
/// Validated inclusive interval over comparable ordered values.
#[derive(Clone, Debug)]
struct InclusiveRange<T> {
    min: T,
    max: T,
}
impl<T: PartialOrd> InclusiveRange<T> {
    pub fn new(min: T, max: T) -> Result<Self, AnalysisError> {
        if !matches!(
            min.partial_cmp(&max),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ) {
            return Err(AnalysisError {
                message: "invalid or incomparable range bounds".into(),
            });
        }
        Ok(Self { min, max })
    }
}
impl<T: PartialOrd> Criterion<T> {
    fn bound(value: T, make: impl FnOnce(T) -> CriterionKind<T>) -> Result<Self, AnalysisError> {
        if value.partial_cmp(&value).is_none() {
            return Err(AnalysisError {
                message: "incomparable bound".into(),
            });
        }
        Ok(Self(make(value)))
    }
    pub fn greater_than(value: T) -> Result<Self, AnalysisError> {
        Self::bound(value, CriterionKind::Greater)
    }
    pub fn at_least(value: T) -> Result<Self, AnalysisError> {
        Self::bound(value, CriterionKind::AtLeast)
    }
    pub fn less_than(value: T) -> Result<Self, AnalysisError> {
        Self::bound(value, CriterionKind::Less)
    }
    pub fn at_most(value: T) -> Result<Self, AnalysisError> {
        Self::bound(value, CriterionKind::AtMost)
    }
    pub fn between(min: T, max: T) -> Result<Self, AnalysisError> {
        Ok(Self(CriterionKind::Range(InclusiveRange::new(min, max)?)))
    }
}
impl<T: PartialOrd + fmt::Debug> Expectation<T> for Criterion<T> {
    type Violation = Violation;
    fn assess(&self, v: &T) -> Result<Vec<Violation>, AnalysisError> {
        let ordering = |left: &T, right: &T| {
            left.partial_cmp(right).ok_or_else(|| AnalysisError {
                message: "incomparable criterion value".into(),
            })
        };
        let valid = match &self.0 {
            CriterionKind::Greater(x) => ordering(v, x)?.is_gt(),
            CriterionKind::AtLeast(x) => !ordering(v, x)?.is_lt(),
            CriterionKind::Less(x) => ordering(v, x)?.is_lt(),
            CriterionKind::AtMost(x) => !ordering(v, x)?.is_gt(),
            CriterionKind::Range(InclusiveRange { min, max }) => {
                !ordering(v, min)?.is_lt() && !ordering(v, max)?.is_gt()
            }
        };
        Ok(if valid {
            vec![]
        } else {
            vec![Violation {
                expected: self.describe(),
            }]
        })
    }
    fn describe(&self) -> String {
        bounded_debug(self, 512)
    }
}
/// Arbitrary domain criterion retaining caller-selected hard/statistical meaning.
pub struct Predicate<F> {
    expected: String,
    class: ViolationClass,
    predicate: F,
}
pub fn criterion<F>(
    expected: impl Into<String>,
    class: ViolationClass,
    predicate: F,
) -> Predicate<F> {
    Predicate {
        expected: expected.into(),
        class,
        predicate,
    }
}
impl<T, F: Fn(&T) -> bool> Expectation<T> for Predicate<F> {
    type Violation = Violation;
    fn assess(&self, v: &T) -> Result<Vec<Violation>, AnalysisError> {
        Ok(if (self.predicate)(v) {
            vec![]
        } else {
            vec![Violation {
                expected: self.expected.clone(),
            }]
        })
    }
    fn describe(&self) -> String {
        self.expected.clone()
    }
    fn class(&self) -> ViolationClass {
        self.class
    }
}
/// Structural report acceptance; no duplicate acceptance logic lives in aggregators.
#[derive(Clone, Copy, Debug)]
pub struct AllValid;
impl<K: fmt::Debug> Expectation<IntegrityReport<K>> for AllValid {
    type Violation = Violation;
    fn assess(&self, v: &IntegrityReport<K>) -> Result<Vec<Violation>, AnalysisError> {
        Ok(if v.is_valid() {
            vec![]
        } else {
            vec![Violation {
                expected: format!(
                    "no integrity issues; observed {} issues among {} records",
                    v.issues.len(),
                    v.record_count
                ),
            }]
        })
    }
    fn describe(&self) -> String {
        "no integrity violations".into()
    }
}
impl<K> Expectation<&Comparison<K>> for AllValid {
    type Violation = Violation;
    fn assess(&self, v: &&Comparison<K>) -> Result<Vec<Violation>, AnalysisError> {
        Ok(if v.is_complete() {
            vec![]
        } else {
            vec![Violation {
                expected: "exact unique key correspondence".into(),
            }]
        })
    }
    fn describe(&self) -> String {
        "exact unique key correspondence".into()
    }
}
impl<K> Expectation<PairCheck<K>> for AllValid {
    type Violation = Violation;
    fn assess(&self, v: &PairCheck<K>) -> Result<Vec<Violation>, AnalysisError> {
        Ok(if v.failures == 0 {
            vec![]
        } else {
            vec![Violation {
                expected: format!("no mismatches; observed {} of {}", v.failures, v.matched),
            }]
        })
    }
    fn describe(&self) -> String {
        "all compared pairs satisfy predicate".into()
    }
}
impl Expectation<usize> for CountRule {
    type Violation = Violation;
    fn assess(&self, v: &usize) -> Result<Vec<Violation>, AnalysisError> {
        AllValid.assess(&check_count(*v, *self))
    }
    fn describe(&self) -> String {
        format!("count {self:?}")
    }
}
impl Expectation<f64> for NumericRule {
    type Violation = Violation;
    fn assess(&self, v: &f64) -> Result<Vec<Violation>, AnalysisError> {
        let r = NumericSeries::new(std::iter::once(*v))
            .integrity(*self)
            .map_err(|e| AnalysisError {
                message: e.to_string(),
            })?;
        AllValid.assess(&r)
    }
    fn describe(&self) -> String {
        format!("{self:?}")
    }
}
impl Expectation<RelationMetrics> for RelationExpectation {
    type Violation = RelationConstraint;
    fn assess(&self, v: &RelationMetrics) -> Result<Vec<RelationConstraint>, AnalysisError> {
        self.violations(v)
    }
    fn describe(&self) -> String {
        bounded_debug(&self.constraints, 1024)
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
impl Expectation<LagReport> for LagExpectation {
    type Violation = LagViolation;
    fn assess(&self, v: &LagReport) -> Result<Vec<LagViolation>, AnalysisError> {
        Ok(self.violations(v))
    }
    fn describe(&self) -> String {
        format!("best lag = {}, no interior gaps", self.best_lag)
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
fn anomaly_violations(observed: usize, maximum: usize) -> Result<Vec<Violation>, AnalysisError> {
    let mut violations = Criterion::at_most(maximum)?.assess(&observed)?;
    for violation in &mut violations {
        violation.expected = format!("anomaly count <= {maximum}; observed {observed}");
    }
    Ok(violations)
}
/// Scenario-owned anomaly count limit (not an instrumentation threshold).
#[derive(Clone, Copy, Debug)]
pub struct AtMostAnomalies(pub usize);
impl Expectation<OutlierReport> for AtMostAnomalies {
    type Violation = Violation;
    fn assess(&self, v: &OutlierReport) -> Result<Vec<Violation>, AnalysisError> {
        anomaly_violations(v.suspicious.len(), self.0)
    }
    fn describe(&self) -> String {
        format!("outliers <= {}", self.0)
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
impl Expectation<JumpReport> for AtMostAnomalies {
    type Violation = Violation;
    fn assess(&self, v: &JumpReport) -> Result<Vec<Violation>, AnalysisError> {
        anomaly_violations(v.suspicious.len(), self.0)
    }
    fn describe(&self) -> String {
        format!("jumps <= {}", self.0)
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
/// Per-group acceptance prevents global statistics from hiding broken subgroups.
#[derive(Clone, Copy, Debug)]
pub struct GroupExpectation {
    minimum_count: usize,
    mean_min: f64,
    mean_max: f64,
}
impl GroupExpectation {
    pub fn new(minimum_count: usize, mean_min: f64, mean_max: f64) -> Result<Self, NumericError> {
        validate_bounds(Some(mean_min), Some(mean_max))?;
        Ok(Self {
            minimum_count,
            mean_min,
            mean_max,
        })
    }
}
impl<K: fmt::Debug> Expectation<GroupedSummaryReport<K>> for GroupExpectation {
    type Violation = Violation;
    fn assess(&self, v: &GroupedSummaryReport<K>) -> Result<Vec<Violation>, AnalysisError> {
        let mut failures = Vec::new();
        if v.groups.is_empty() {
            failures.push(Violation {
                expected: "at least one group".into(),
            });
        }
        for g in &v.groups {
            let s = g.summary.as_ref().map_err(|e| AnalysisError {
                message: format!("group {}: {e}", bounded_debug(&g.key, 128)),
            })?;
            if g.member_count < self.minimum_count
                || !(self.mean_min..=self.mean_max).contains(&s.mean)
            {
                failures.push(Violation {
                    expected: format!(
                        "group {} count={} mean={}",
                        bounded_debug(&g.key, 128),
                        g.member_count,
                        s.mean
                    ),
                });
            }
        }
        Ok(failures)
    }
    fn describe(&self) -> String {
        format!(
            "per-group count >= {}, mean in [{},{}]",
            self.minimum_count, self.mean_min, self.mean_max
        )
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
