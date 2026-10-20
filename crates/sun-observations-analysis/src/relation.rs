use crate::{
    checked, finite, quantile_sorted, Alignment, Join, NumericError, NumericSeries, Summary,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BinaryHeap, error::Error, fmt};

/// A mathematically undefined measurement is explicit rather than NaN or JSON null.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UndefinedReason {
    InsufficientPairs,
    ConstantLeft,
    ConstantRight,
}
/// Defined metrics always contain finite values.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Metric {
    Defined(f64),
    Undefined(UndefinedReason),
}
impl Metric {
    pub fn value(self) -> Option<f64> {
        match self {
            Self::Defined(v) => Some(v),
            Self::Undefined(_) => None,
        }
    }
}
/// Streaming centered bivariate moments and an ordinary least-squares fit of right on left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinearMetrics {
    pub count: usize,
    pub pearson: Metric,
    pub covariance: Metric,
    pub sample_covariance: Metric,
    pub slope: Metric,
    pub intercept: Metric,
    pub r_squared: Metric,
}
/// O(n), constant scratch storage; finite intermediate overflow is reported explicitly.
pub fn linear_metrics(
    pairs: impl IntoIterator<Item = (f64, f64)>,
) -> Result<LinearMetrics, NumericError> {
    let (mut count, mut mx, mut my, mut xx, mut yy, mut xy) = (0usize, 0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut origin_x, mut origin_y) = (0.0, 0.0);
    let (mut varies_x, mut varies_y) = (false, false);
    for (index, (x, y)) in pairs.into_iter().enumerate() {
        finite(x, index)?;
        finite(y, index)?;
        if count == 0 {
            origin_x = x;
            origin_y = y;
        }
        varies_x |= x != origin_x;
        varies_y |= y != origin_y;
        let x = checked(x - origin_x, "left shifted value")?;
        let y = checked(y - origin_y, "right shifted value")?;
        count += 1;
        let dx = checked(x - mx, "left centered delta")?;
        let dy = checked(y - my, "right centered delta")?;
        mx = checked(mx + dx / count as f64, "left mean")?;
        my = checked(my + dy / count as f64, "right mean")?;
        xx = checked(xx + dx * (x - mx), "left centered moment")?;
        yy = checked(yy + dy * (y - my), "right centered moment")?;
        xy = checked(xy + dx * (y - my), "cross moment")?;
    }
    if xx <= 0.0 && varies_x {
        return Err(NumericError::ArithmeticUnderflow {
            operation: "left variance".into(),
        });
    }
    if yy <= 0.0 && varies_y {
        return Err(NumericError::ArithmeticUnderflow {
            operation: "right variance".into(),
        });
    }
    let undefined = if count < 2 {
        Some(UndefinedReason::InsufficientPairs)
    } else if xx <= 0.0 {
        Some(UndefinedReason::ConstantLeft)
    } else if yy <= 0.0 {
        Some(UndefinedReason::ConstantRight)
    } else {
        None
    };
    let pearson = if let Some(reason) = undefined {
        Metric::Undefined(reason)
    } else {
        Metric::Defined(checked((xy / xx.sqrt()) / yy.sqrt(), "Pearson")?.clamp(-1.0, 1.0))
    };
    let slope = if count < 2 {
        Metric::Undefined(UndefinedReason::InsufficientPairs)
    } else if xx <= 0.0 {
        Metric::Undefined(UndefinedReason::ConstantLeft)
    } else {
        Metric::Defined(checked(xy / xx, "slope")?)
    };
    let intercept = match slope {
        Metric::Defined(s) => Metric::Defined(checked(
            checked(origin_y - s * origin_x, "intercept origin")? + (my - s * mx),
            "intercept",
        )?),
        other => other,
    };
    let covariance = if count == 0 {
        Metric::Undefined(UndefinedReason::InsufficientPairs)
    } else {
        Metric::Defined(xy / count as f64)
    };
    let sample_covariance = if count < 2 {
        Metric::Undefined(UndefinedReason::InsufficientPairs)
    } else {
        Metric::Defined(xy / (count - 1) as f64)
    };
    let r_squared = match pearson {
        Metric::Defined(r) => Metric::Defined(r * r),
        other => other,
    };
    Ok(LinearMetrics {
        count,
        pearson,
        covariance,
        sample_covariance,
        slope,
        intercept,
        r_squared,
    })
}
pub fn pearson(pairs: impl IntoIterator<Item = (f64, f64)>) -> Result<Metric, NumericError> {
    Ok(linear_metrics(pairs)?.pearson)
}

/// Average ranks for ties, including equal signed zeros; deterministic O(n log n).
pub fn spearman(pairs: impl IntoIterator<Item = (f64, f64)>) -> Result<Metric, NumericError> {
    let mut x = Vec::new();
    let mut y = Vec::new();
    for (index, (a, b)) in pairs.into_iter().enumerate() {
        x.push(finite(a, index)?);
        y.push(finite(b, index)?);
    }
    let rx = ranks(&x);
    let ry = ranks(&y);
    pearson(rx.into_iter().zip(ry))
}
fn ranks(values: &[f64]) -> Vec<f64> {
    let mut indices: Vec<_> = (0..values.len()).collect();
    indices.sort_unstable_by(|&a, &b| values[a].partial_cmp(&values[b]).unwrap().then(a.cmp(&b)));
    let mut ranks = vec![0.0; values.len()];
    let mut i = 0;
    while i < indices.len() {
        let mut end = i + 1;
        while end < indices.len() && values[indices[i]] == values[indices[end]] {
            end += 1;
        }
        let rank = (i as f64 + (end - 1) as f64) / 2.0 + 1.0;
        for &index in &indices[i..end] {
            ranks[index] = rank;
        }
        i = end;
    }
    ranks
}

/// Right-minus-left errors, including type-7 median and 95th percentile absolute error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DifferenceSummary {
    pub signed: Summary,
    pub absolute: Summary,
    pub median_signed: f64,
    pub median_absolute: f64,
    pub p95_absolute: f64,
}
pub fn difference_summary(
    pairs: impl IntoIterator<Item = (f64, f64)>,
) -> Result<DifferenceSummary, NumericError> {
    let mut signed = Vec::new();
    for (index, (x, y)) in pairs.into_iter().enumerate() {
        signed.push(checked(
            finite(y, index)? - finite(x, index)?,
            "difference",
        )?);
    }
    let signed_summary = NumericSeries::new(signed.iter().copied()).summary()?;
    let mut absolute: Vec<_> = signed.iter().map(|v| v.abs()).collect();
    let absolute_summary = NumericSeries::new(absolute.iter().copied()).summary()?;
    signed.sort_unstable_by(f64::total_cmp);
    absolute.sort_unstable_by(f64::total_cmp);
    Ok(DifferenceSummary {
        signed: signed_summary,
        absolute: absolute_summary,
        median_signed: quantile_sorted(&signed, 0.5),
        median_absolute: quantile_sorted(&absolute, 0.5),
        p95_absolute: quantile_sorted(&absolute, 0.95),
    })
}
/// Source-keyed disagreement, in decreasing absolute-error order with key-order tie breaks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairEvidence<K> {
    pub key: K,
    pub left: f64,
    pub right: f64,
    pub signed_difference: f64,
    pub absolute_difference: f64,
}
/// Heap ordering keeps the worst retained disagreement at the root, including deterministic ties.
struct RankedPair<K> {
    order: usize,
    evidence: PairEvidence<K>,
}
impl<K> PartialEq for RankedPair<K> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<K> Eq for RankedPair<K> {}
impl<K> PartialOrd for RankedPair<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<K> Ord for RankedPair<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .evidence
            .absolute_difference
            .total_cmp(&self.evidence.absolute_difference)
            .then(self.order.cmp(&other.order))
    }
}
/// Difference measurements plus a caller-bounded list of largest disagreements.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "difference")]
pub struct DifferenceReport<K> {
    pub summary: DifferenceSummary,
    pub largest: Vec<PairEvidence<K>>,
}
/// Relation measurements contain no domain acceptance decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelationMetrics {
    pub linear: LinearMetrics,
    pub spearman: Metric,
    pub difference: DifferenceSummary,
    /// Absent when the linear fit is undefined; errors are observed minus predicted right values.
    pub residual: Option<DifferenceSummary>,
}
pub fn numeric_relation<I: Iterator<Item = (f64, f64)> + Clone>(
    pairs: I,
) -> Result<RelationMetrics, NumericError> {
    let linear = linear_metrics(pairs.clone())?;
    let spearman = spearman(pairs.clone())?;
    let difference = difference_summary(pairs.clone())?;
    let residual = match (linear.slope, linear.intercept) {
        (Metric::Defined(s), Metric::Defined(b)) => {
            Some(difference_summary(pairs.map(|(x, y)| (s * x + b, y)))?)
        }
        _ => None,
    };
    Ok(RelationMetrics {
        linear,
        spearman,
        difference,
        residual,
    })
}
/// Complete alignment, numerical measurements, and source-keyed disagreements.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "relation")]
pub struct RelationReport<K> {
    pub alignment: Alignment<K>,
    pub observed: RelationMetrics,
    pub largest_disagreements: Vec<PairEvidence<K>>,
}
/// Hard alignment/numeric failure, distinct from a failed statistical expectation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RelationError<K> {
    Alignment(Box<Alignment<K>>),
    Numeric {
        alignment: Box<Alignment<K>>,
        error: NumericError,
    },
}
impl<K> fmt::Display for RelationError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Alignment(a) => write!(f, "incomplete relation alignment: left={}, right={}, matched={}, missing_left={}, missing_right={}, duplicate_left={}, duplicate_right={}", a.left_count,a.right_count,a.matched.len(),a.missing_left.len(),a.missing_right.len(),a.duplicate_left.len(),a.duplicate_right.len()),
            Self::Numeric { alignment,error } => write!(f, "relation over {} pairs: {error}",alignment.matched.len()),
        }
    }
}
impl<K: fmt::Debug> Error for RelationError<K> {}
impl<L, R, K: Clone> Join<'_, L, R, K> {
    fn numeric_error(&self, error: NumericError) -> RelationError<K> {
        RelationError::Numeric {
            alignment: Box::new(self.alignment.clone()),
            error,
        }
    }
    pub fn difference(
        &self,
        left: impl Fn(&L) -> f64,
        right: impl Fn(&R) -> f64,
        top: usize,
    ) -> Result<DifferenceReport<K>, RelationError<K>> {
        self.require_complete()
            .map_err(|a| RelationError::Alignment(Box::new(a.clone())))?;
        let summary = difference_summary(self.pairs().map(|(_, a, b)| (left(a), right(b))))
            .map_err(|error| self.numeric_error(error))?;
        let largest = self
            .largest(left, right, top)
            .map_err(|error| self.numeric_error(error))?;
        Ok(DifferenceReport { summary, largest })
    }
    fn largest(
        &self,
        left: impl Fn(&L) -> f64,
        right: impl Fn(&R) -> f64,
        top: usize,
    ) -> Result<Vec<PairEvidence<K>>, NumericError> {
        let mut largest =
            BinaryHeap::<RankedPair<K>>::with_capacity(top.min(self.alignment.matched.len()));
        if top == 0 {
            return Ok(Vec::new());
        }
        for (order, (key, a, b)) in self.pairs().enumerate() {
            let (a, b) = (left(a), right(b));
            let signed_difference = checked(b - a, "difference evidence")?;
            let absolute_difference = signed_difference.abs();
            if largest.len() == top
                && absolute_difference <= largest.peek().unwrap().evidence.absolute_difference
            {
                continue;
            }
            let item = RankedPair {
                order,
                evidence: PairEvidence {
                    key: key.clone(),
                    left: a,
                    right: b,
                    signed_difference,
                    absolute_difference,
                },
            };
            if largest.len() == top {
                *largest.peek_mut().unwrap() = item;
            } else {
                largest.push(item);
            }
        }
        Ok(largest
            .into_sorted_vec()
            .into_iter()
            .map(|item| item.evidence)
            .collect())
    }
    pub fn numeric_relation(
        &self,
        left: impl Fn(&L) -> f64 + Clone,
        right: impl Fn(&R) -> f64 + Clone,
        top: usize,
    ) -> Result<RelationReport<K>, RelationError<K>> {
        self.require_complete()
            .map_err(|a| RelationError::Alignment(Box::new(a.clone())))?;
        let observed = numeric_relation(self.pairs().map(|(_, a, b)| (left(a), right(b))))
            .map_err(|error| self.numeric_error(error))?;
        let largest_disagreements = self
            .largest(left, right, top)
            .map_err(|error| self.numeric_error(error))?;
        Ok(RelationReport {
            alignment: self.alignment.clone(),
            observed,
            largest_disagreements,
        })
    }
}

/// A measurement that a consuming certification may constrain independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationMeasure {
    Pearson,
    Spearman,
    AbsolutePearson,
    AbsoluteSpearman,
    Slope,
    Intercept,
    RSquared,
    MeanAbsoluteDifference,
    P95AbsoluteDifference,
    MaxAbsoluteDifference,
    ResidualMeanAbsolute,
    ResidualMaxAbsolute,
}
/// Inclusive optional bounds; undefined observed values always fail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelationConstraint {
    pub measure: RelationMeasure,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
/// Scenario-owned expectations, not properties of an Observation type.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RelationExpectation {
    pub constraints: Vec<RelationConstraint>,
}
/// Structured statistical failure retaining the full expected and observed evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "relation_expectation_failure")]
pub struct RelationFailure<K> {
    pub expected: RelationExpectation,
    pub violated: Vec<RelationConstraint>,
    pub observed: RelationReport<K>,
}
impl<K: fmt::Debug> fmt::Display for RelationFailure<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} relation constraints failed for {} pairs",
            self.violated.len(),
            self.observed.observed.linear.count
        )
    }
}
impl<K: fmt::Debug> Error for RelationFailure<K> {}
impl RelationExpectation {
    pub fn positive(min_spearman: f64) -> Result<Self, NumericError> {
        if !(min_spearman > 0.0 && min_spearman <= 1.0) {
            return Err(NumericError::InvalidParameter {
                name: "positive minimum Spearman".into(),
                value: min_spearman.into(),
            });
        }
        Self::default().bound(RelationMeasure::Spearman, Some(min_spearman), None)
    }
    pub fn negative(max_spearman: f64) -> Result<Self, NumericError> {
        if !(-1.0..0.0).contains(&max_spearman) {
            return Err(NumericError::InvalidParameter {
                name: "negative maximum Spearman".into(),
                value: max_spearman.into(),
            });
        }
        Self::default().bound(RelationMeasure::Spearman, None, Some(max_spearman))
    }
    pub fn bound(
        mut self,
        measure: RelationMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        for v in [min, max].into_iter().flatten() {
            if !v.is_finite() {
                return Err(NumericError::InvalidParameter {
                    name: "expectation bound".into(),
                    value: v.into(),
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
        self.constraints
            .push(RelationConstraint { measure, min, max });
        Ok(self)
    }
    pub fn evaluate<K>(
        &self,
        observed: RelationReport<K>,
    ) -> Result<RelationReport<K>, Box<RelationFailure<K>>> {
        let violated: Vec<_> = self
            .constraints
            .iter()
            .filter(|c| {
                let value = measure(&observed.observed, c.measure);
                value.is_none_or(|v| {
                    !v.is_finite()
                        || c.min.is_some_and(|m| !m.is_finite() || v < m)
                        || c.max.is_some_and(|m| !m.is_finite() || v > m)
                })
            })
            .cloned()
            .collect();
        if violated.is_empty() {
            Ok(observed)
        } else {
            Err(Box::new(RelationFailure {
                expected: self.clone(),
                violated,
                observed,
            }))
        }
    }
}
fn measure(report: &RelationMetrics, measure: RelationMeasure) -> Option<f64> {
    match measure {
        RelationMeasure::Pearson => report.linear.pearson.value(),
        RelationMeasure::Spearman => report.spearman.value(),
        RelationMeasure::AbsolutePearson => report.linear.pearson.value().map(f64::abs),
        RelationMeasure::AbsoluteSpearman => report.spearman.value().map(f64::abs),
        RelationMeasure::Slope => report.linear.slope.value(),
        RelationMeasure::Intercept => report.linear.intercept.value(),
        RelationMeasure::RSquared => report.linear.r_squared.value(),
        RelationMeasure::MeanAbsoluteDifference => Some(report.difference.absolute.mean),
        RelationMeasure::P95AbsoluteDifference => Some(report.difference.p95_absolute),
        RelationMeasure::MaxAbsoluteDifference => Some(report.difference.absolute.max),
        RelationMeasure::ResidualMeanAbsolute => report.residual.as_ref().map(|r| r.absolute.mean),
        RelationMeasure::ResidualMaxAbsolute => report.residual.as_ref().map(|r| r.absolute.max),
    }
}

#[cfg(test)]
#[path = "relation_certification_test.rs"]
mod tests;
