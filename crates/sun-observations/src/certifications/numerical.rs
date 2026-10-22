use crate::certifications::*;
use serde::{Deserialize, Serialize};

/// Rank and linear association, without difference or regression acceptance semantics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CorrelationMetrics {
    pub pearson: Metric,
    pub spearman: Metric,
}
pub fn correlation<I: Iterator<Item = (f64, f64)> + Clone>(
    pairs: I,
) -> Result<CorrelationMetrics, NumericError> {
    Ok(CorrelationMetrics {
        pearson: pearson(pairs.clone())?,
        spearman: spearman(pairs)?,
    })
}
/// Ordinary least-squares fit of right on left, with observed-minus-predicted residuals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinearFitMetrics {
    pub linear: LinearMetrics,
    pub residual: Option<DifferenceSummary>,
}
pub fn linear_fit<I: Iterator<Item = (f64, f64)> + Clone>(
    pairs: I,
) -> Result<LinearFitMetrics, NumericError> {
    fit_with_linear(linear_metrics(pairs.clone())?, pairs)
}
pub(crate) fn fit_with_linear(
    linear: LinearMetrics,
    pairs: impl Iterator<Item = (f64, f64)>,
) -> Result<LinearFitMetrics, NumericError> {
    let residual = match (linear.slope, linear.intercept) {
        (Metric::Defined(s), Metric::Defined(b)) => {
            Some(difference_summary(pairs.map(|(x, y)| (s * x + b, y)))?)
        }
        _ => None,
    };
    Ok(LinearFitMetrics { linear, residual })
}
impl Fact<LinearFitMetrics> {
    fn metric(self, select: impl FnOnce(&LinearMetrics) -> Metric) -> Fact<f64> {
        let observed = self.observed.and_then(|r| {
            select(&r.linear).value().ok_or_else(|| AnalysisError {
                message: "undefined linear-fit metric".into(),
            })
        });
        Fact {
            source: self.source,
            observed,
            details: self.details,
        }
    }
    pub fn slope(self) -> Fact<f64> {
        self.metric(|r| r.slope)
    }
    pub fn intercept(self) -> Fact<f64> {
        self.metric(|r| r.intercept)
    }
    pub fn r_squared(self) -> Fact<f64> {
        self.metric(|r| r.r_squared)
    }
}
/// Inclusive scenario bounds on one typed numerical measure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "MetricConstraintInput<M>",
    bound(deserialize = "M: Deserialize<'de>")
)]
pub struct MetricConstraint<M> {
    pub(crate) measure: M,
    pub(crate) min: Option<f64>,
    pub(crate) max: Option<f64>,
}
#[derive(Deserialize)]
struct MetricConstraintInput<M> {
    measure: M,
    min: Option<f64>,
    max: Option<f64>,
}
impl<M> TryFrom<MetricConstraintInput<M>> for MetricConstraint<M> {
    type Error = NumericError;
    fn try_from(input: MetricConstraintInput<M>) -> Result<Self, Self::Error> {
        Self::new(input.measure, input.min, input.max)
    }
}
impl<M> MetricConstraint<M> {
    pub fn new(measure: M, min: Option<f64>, max: Option<f64>) -> Result<Self, NumericError> {
        validate_bounds(min, max)?;
        Ok(Self { measure, min, max })
    }
    pub fn measure(&self) -> &M {
        &self.measure
    }
    pub fn min(&self) -> Option<f64> {
        self.min
    }
    pub fn max(&self) -> Option<f64> {
        self.max
    }
}
pub(crate) fn validate_bounds(min: Option<f64>, max: Option<f64>) -> Result<(), NumericError> {
    if min.is_none() && max.is_none() {
        return Err(NumericError::InvalidParameter {
            name: "empty expectation bounds".into(),
            value: 0.0.into(),
        });
    }
    for value in [min, max].into_iter().flatten() {
        if !value.is_finite() {
            return Err(NumericError::InvalidParameter {
                name: "expectation bound".into(),
                value: value.into(),
            });
        }
    }
    if let (Some(a), Some(b)) = (min, max) {
        if a > b {
            return Err(NumericError::InvalidParameter {
                name: "reversed expectation bounds".into(),
                value: a.into(),
            });
        }
    }
    Ok(())
}
pub(crate) fn assess<M: Clone>(
    constraints: &[MetricConstraint<M>],
    value: impl Fn(&M) -> Option<f64>,
) -> Result<Vec<MetricConstraint<M>>, AnalysisError> {
    let mut violations = Vec::new();
    for c in constraints {
        let v = value(&c.measure)
            .filter(|v| v.is_finite())
            .ok_or_else(|| AnalysisError {
                message: "undefined or nonfinite required metric".into(),
            })?;
        if c.min.is_some_and(|m| v < m) || c.max.is_some_and(|m| v > m) {
            violations.push(c.clone());
        }
    }
    Ok(violations)
}
/// Correlation-only scenario measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorrelationMeasure {
    Pearson,
    Spearman,
    AbsolutePearson,
    AbsoluteSpearman,
}
/// Nonempty scenario-owned correlation bounds, independent of production evidence.
///
/// ```compile_fail
/// use sun_observations::certifications::CorrelationExpectation;
/// let _ = CorrelationExpectation::default();
/// ```
#[derive(Clone, Debug)]
pub struct CorrelationExpectation {
    constraints: Vec<MetricConstraint<CorrelationMeasure>>,
}
impl CorrelationExpectation {
    pub fn pearson_at_least(min: f64) -> Result<Self, NumericError> {
        Self::with_bound(CorrelationMeasure::Pearson, Some(min), None)
    }
    pub fn spearman_at_least(min: f64) -> Result<Self, NumericError> {
        Self::with_bound(CorrelationMeasure::Spearman, Some(min), None)
    }
    pub fn spearman_at_most(max: f64) -> Result<Self, NumericError> {
        Self::with_bound(CorrelationMeasure::Spearman, None, Some(max))
    }
    pub fn with_bound(
        measure: CorrelationMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        Self {
            constraints: Vec::new(),
        }
        .bound(measure, min, max)
    }

    pub fn bound(
        mut self,
        measure: CorrelationMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        self.constraints
            .push(MetricConstraint::new(measure, min, max)?);
        Ok(self)
    }
    pub fn and_pearson_bounds(
        self,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        self.bound(CorrelationMeasure::Pearson, min, max)
    }
    pub fn and_spearman_bounds(
        self,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        self.bound(CorrelationMeasure::Spearman, min, max)
    }
}
impl Expectation<CorrelationMetrics> for CorrelationExpectation {
    type Violation = MetricConstraint<CorrelationMeasure>;
    fn assess(&self, r: &CorrelationMetrics) -> Result<Vec<Self::Violation>, AnalysisError> {
        assess(&self.constraints, |m| match m {
            CorrelationMeasure::Pearson => r.pearson.value(),
            CorrelationMeasure::Spearman => r.spearman.value(),
            CorrelationMeasure::AbsolutePearson => r.pearson.value().map(f64::abs),
            CorrelationMeasure::AbsoluteSpearman => r.spearman.value().map(f64::abs),
        })
    }
    fn describe(&self) -> String {
        bounded_debug(&self.constraints, 1024)
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
/// Regression-only scenario measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinearFitMeasure {
    Slope,
    Intercept,
    RSquared,
    ResidualMeanAbsolute,
    ResidualMaxAbsolute,
}
/// Scenario-owned linear transformation and residual bounds.
#[derive(Clone, Debug)]
pub struct LinearFitExpectation {
    constraints: Vec<MetricConstraint<LinearFitMeasure>>,
}
impl LinearFitExpectation {
    pub fn slope_between(min: f64, max: f64) -> Result<Self, NumericError> {
        Self::with_bound(LinearFitMeasure::Slope, Some(min), Some(max))
    }
    pub fn intercept_between(min: f64, max: f64) -> Result<Self, NumericError> {
        Self::with_bound(LinearFitMeasure::Intercept, Some(min), Some(max))
    }
    pub fn r_squared_at_least(min: f64) -> Result<Self, NumericError> {
        Self::with_bound(LinearFitMeasure::RSquared, Some(min), None)
    }
    pub fn residual_mean_absolute_at_most(max: f64) -> Result<Self, NumericError> {
        Self::with_bound(LinearFitMeasure::ResidualMeanAbsolute, None, Some(max))
    }
    pub fn residual_max_absolute_at_most(max: f64) -> Result<Self, NumericError> {
        Self::with_bound(LinearFitMeasure::ResidualMaxAbsolute, None, Some(max))
    }
    pub fn with_bound(
        measure: LinearFitMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        Self {
            constraints: Vec::new(),
        }
        .bound(measure, min, max)
    }
    pub fn bound(
        mut self,
        measure: LinearFitMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        self.constraints
            .push(MetricConstraint::new(measure, min, max)?);
        Ok(self)
    }
    pub fn and_slope_between(self, min: f64, max: f64) -> Result<Self, NumericError> {
        self.bound(LinearFitMeasure::Slope, Some(min), Some(max))
    }
    pub fn and_intercept_between(self, min: f64, max: f64) -> Result<Self, NumericError> {
        self.bound(LinearFitMeasure::Intercept, Some(min), Some(max))
    }
    pub fn and_r_squared_at_least(self, min: f64) -> Result<Self, NumericError> {
        self.bound(LinearFitMeasure::RSquared, Some(min), None)
    }
    pub fn and_residual_mean_absolute_at_most(self, max: f64) -> Result<Self, NumericError> {
        self.bound(LinearFitMeasure::ResidualMeanAbsolute, None, Some(max))
    }
    pub fn and_residual_max_absolute_at_most(self, max: f64) -> Result<Self, NumericError> {
        self.bound(LinearFitMeasure::ResidualMaxAbsolute, None, Some(max))
    }
}
impl Expectation<LinearFitMetrics> for LinearFitExpectation {
    type Violation = MetricConstraint<LinearFitMeasure>;
    fn assess(&self, r: &LinearFitMetrics) -> Result<Vec<Self::Violation>, AnalysisError> {
        assess(&self.constraints, |m| match m {
            LinearFitMeasure::Slope => r.linear.slope.value(),
            LinearFitMeasure::Intercept => r.linear.intercept.value(),
            LinearFitMeasure::RSquared => r.linear.r_squared.value(),
            LinearFitMeasure::ResidualMeanAbsolute => r.residual.as_ref().map(|v| v.absolute.mean),
            LinearFitMeasure::ResidualMaxAbsolute => r.residual.as_ref().map(|v| v.absolute.max),
        })
    }
    fn describe(&self) -> String {
        bounded_debug(&self.constraints, 1024)
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
/// Direct right-minus-left error measures, not fitted residuals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DifferenceMeasure {
    MeanAbsolute,
    P95Absolute,
    MaxAbsolute,
}
/// Scenario-owned pairwise difference bounds.
#[derive(Clone, Debug)]
pub struct DifferenceExpectation {
    constraints: Vec<MetricConstraint<DifferenceMeasure>>,
}
impl DifferenceExpectation {
    pub fn mean_absolute_at_most(max: f64) -> Result<Self, NumericError> {
        Self::with_bound(DifferenceMeasure::MeanAbsolute, None, Some(max))
    }
    pub fn p95_absolute_at_most(max: f64) -> Result<Self, NumericError> {
        Self::with_bound(DifferenceMeasure::P95Absolute, None, Some(max))
    }
    pub fn max_absolute_at_most(max: f64) -> Result<Self, NumericError> {
        Self::with_bound(DifferenceMeasure::MaxAbsolute, None, Some(max))
    }
    pub fn with_bound(
        measure: DifferenceMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        Self {
            constraints: Vec::new(),
        }
        .bound(measure, min, max)
    }
    pub fn bound(
        mut self,
        measure: DifferenceMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        self.constraints
            .push(MetricConstraint::new(measure, min, max)?);
        Ok(self)
    }
    pub fn and_mean_absolute_at_most(self, max: f64) -> Result<Self, NumericError> {
        self.bound(DifferenceMeasure::MeanAbsolute, None, Some(max))
    }
    pub fn and_p95_absolute_at_most(self, max: f64) -> Result<Self, NumericError> {
        self.bound(DifferenceMeasure::P95Absolute, None, Some(max))
    }
    pub fn and_max_absolute_at_most(self, max: f64) -> Result<Self, NumericError> {
        self.bound(DifferenceMeasure::MaxAbsolute, None, Some(max))
    }
}
impl Expectation<DifferenceSummary> for DifferenceExpectation {
    type Violation = MetricConstraint<DifferenceMeasure>;
    fn assess(&self, r: &DifferenceSummary) -> Result<Vec<Self::Violation>, AnalysisError> {
        assess(&self.constraints, |m| {
            Some(match m {
                DifferenceMeasure::MeanAbsolute => r.absolute.mean,
                DifferenceMeasure::P95Absolute => r.p95_absolute,
                DifferenceMeasure::MaxAbsolute => r.absolute.max,
            })
        })
    }
    fn describe(&self) -> String {
        bounded_debug(&self.constraints, 1024)
    }
    fn class(&self) -> ViolationClass {
        ViolationClass::Statistical
    }
}
