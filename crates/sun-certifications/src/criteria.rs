use crate::*;
use std::fmt;

/// Acceptance classification; analysis/precondition errors are not failed expectations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckClass {
    Pass,
    HardViolation,
    StatisticalViolation,
    EvaluationError,
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
            message: crate::certification::bounded_display(&e, 2048),
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
                        class = expected.class();
                        if !matches!(
                            class,
                            CheckClass::HardViolation | CheckClass::StatisticalViolation
                        ) {
                            class = CheckClass::EvaluationError;
                            analysis_error = Some(AnalysisError {
                                message: "violated expectation must be hard or statistical".into(),
                            });
                        }
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
    fn class(&self) -> CheckClass {
        CheckClass::HardViolation
    }
}
/// Directly inspectable result; aggregation consumes it without copying its report.
#[derive(Debug)]
pub struct CheckResult<T, V = Violation> {
    pub source: String,
    pub expected: String,
    pub observed: Result<T, AnalysisError>,
    pub violations: Vec<V>,
    pub class: CheckClass,
    pub analysis_error: Option<AnalysisError>,
    pub details: String,
}
impl<T, V> CheckResult<T, V> {
    pub fn is_pass(&self) -> bool {
        self.class == CheckClass::Pass
    }
}
/// Equality requires only PartialEq, including structured or collection values.
#[derive(Clone, Debug)]
pub struct Equal<T>(pub T);
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
pub enum Criterion<T> {
    Equal(T),
    NotEqual(T),
    Greater(T),
    AtLeast(T),
    Less(T),
    AtMost(T),
    Range { min: T, max: T },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub expected: String,
}
impl<T: PartialOrd + fmt::Debug> Expectation<T> for Criterion<T> {
    type Violation = Violation;
    fn assess(&self, v: &T) -> Result<Vec<Violation>, AnalysisError> {
        let ordering = |left: &T, right: &T| {
            left.partial_cmp(right).ok_or_else(|| AnalysisError {
                message: "incomparable criterion value".into(),
            })
        };
        let valid = match self {
            Self::Equal(x) => v == x,
            Self::NotEqual(x) => v != x,
            Self::Greater(x) => ordering(v, x)?.is_gt(),
            Self::AtLeast(x) => !ordering(v, x)?.is_lt(),
            Self::Less(x) => ordering(v, x)?.is_lt(),
            Self::AtMost(x) => !ordering(v, x)?.is_gt(),
            Self::Range { min, max } => {
                if ordering(min, max)?.is_gt() {
                    return Err(AnalysisError {
                        message: "invalid criterion range".into(),
                    });
                }
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
    class: CheckClass,
    predicate: F,
}
pub fn criterion<F>(expected: impl Into<String>, class: CheckClass, predicate: F) -> Predicate<F> {
    Predicate {
        expected: expected.into(),
        class,
        predicate,
    }
}
impl<T, F: Fn(&T) -> bool> Expectation<T> for Predicate<F> {
    type Violation = Violation;
    fn assess(&self, v: &T) -> Result<Vec<Violation>, AnalysisError> {
        if !matches!(
            self.class,
            CheckClass::HardViolation | CheckClass::StatisticalViolation
        ) {
            return Err(AnalysisError {
                message: "criterion must be hard or statistical".into(),
            });
        }
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
    fn class(&self) -> CheckClass {
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
        "all aligned pairs satisfy predicate".into()
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
            .check(*self)
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
    fn class(&self) -> CheckClass {
        CheckClass::StatisticalViolation
    }
}
impl<K> Expectation<RelationReport<K>> for RelationExpectation {
    type Violation = RelationConstraint;
    fn assess(&self, v: &RelationReport<K>) -> Result<Vec<RelationConstraint>, AnalysisError> {
        if !v.comparison.is_complete() {
            return Err(AnalysisError {
                message: "incomplete relation comparison".into(),
            });
        }
        self.assess(&v.observed)
    }
    fn describe(&self) -> String {
        bounded_debug(&self.constraints, 1024)
    }
    fn class(&self) -> CheckClass {
        CheckClass::StatisticalViolation
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
    fn class(&self) -> CheckClass {
        CheckClass::StatisticalViolation
    }
}
fn anomaly_violations(observed: usize, maximum: usize) -> Result<Vec<Violation>, AnalysisError> {
    let mut violations = Criterion::AtMost(maximum).assess(&observed)?;
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
    fn class(&self) -> CheckClass {
        CheckClass::StatisticalViolation
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
    fn class(&self) -> CheckClass {
        CheckClass::StatisticalViolation
    }
}
/// Per-group acceptance prevents global statistics from hiding broken subgroups.
#[derive(Clone, Copy, Debug)]
pub struct GroupExpectation {
    pub minimum_count: usize,
    pub mean_min: f64,
    pub mean_max: f64,
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
        if !self.mean_min.is_finite() || !self.mean_max.is_finite() || self.mean_min > self.mean_max
        {
            return Err(AnalysisError {
                message: "invalid group mean bounds".into(),
            });
        }
        Ok(failures)
    }
    fn describe(&self) -> String {
        format!(
            "per-group count >= {}, mean in [{},{}]",
            self.minimum_count, self.mean_min, self.mean_max
        )
    }
    fn class(&self) -> CheckClass {
        CheckClass::StatisticalViolation
    }
}
