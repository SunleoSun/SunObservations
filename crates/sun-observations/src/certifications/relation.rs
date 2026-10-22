use crate::certifications::{
    checked, finite, quantile_sorted, NumericError, NumericSeries, Summary,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BinaryHeap};

/// A mathematically undefined metric is explicit rather than NaN or JSON null.
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
pub(crate) fn largest_pairs<'a, K: Clone + 'a>(
    pairs: impl Iterator<Item = (&'a K, f64, f64)>,
    top: usize,
) -> Result<Vec<PairEvidence<K>>, NumericError> {
    let mut largest = BinaryHeap::<RankedPair<K>>::new();
    if top == 0 {
        return Ok(Vec::new());
    }
    for (order, (key, a, b)) in pairs.enumerate() {
        finite(a, order)?;
        finite(b, order)?;
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
        .map(|p| p.evidence)
        .collect())
}

/// Relation metrics contain no domain acceptance decision.
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
    let fit = crate::certifications::linear_fit(pairs.clone())?;
    Ok(RelationMetrics {
        linear: fit.linear,
        residual: fit.residual,
        spearman: spearman(pairs.clone())?,
        difference: difference_summary(pairs)?,
    })
}

/// A metric that a consuming certification may constrain independently.
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
/// Inclusive bounds on composite relation measures; undefined evidence prevents evaluation.
pub type RelationConstraint = crate::certifications::MetricConstraint<RelationMeasure>;
/// Scenario-owned expectations, not properties of production evidence types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RelationExpectationInput")]
pub struct RelationExpectation {
    pub(super) constraints: Vec<RelationConstraint>,
}

#[derive(Deserialize)]
struct RelationExpectationInput {
    constraints: Vec<RelationConstraint>,
}
impl TryFrom<RelationExpectationInput> for RelationExpectation {
    type Error = NumericError;
    fn try_from(input: RelationExpectationInput) -> Result<Self, Self::Error> {
        if input.constraints.is_empty() {
            return Err(NumericError::InvalidParameter {
                name: "empty expectation".into(),
                value: 0.0.into(),
            });
        }
        for constraint in &input.constraints {
            validate_relation_bounds(constraint.measure, constraint.min, constraint.max)?;
        }
        Ok(Self {
            constraints: input.constraints,
        })
    }
}

fn validate_relation_bounds(
    measure: RelationMeasure,
    min: Option<f64>,
    max: Option<f64>,
) -> Result<(), NumericError> {
    use RelationMeasure::*;
    match measure {
        Pearson | Spearman => crate::certifications::numerical::validate_domain_bounds(
            min,
            max,
            Some(-1.0),
            Some(1.0),
            "correlation bound",
        ),
        AbsolutePearson | AbsoluteSpearman | RSquared => {
            crate::certifications::numerical::validate_domain_bounds(
                min,
                max,
                Some(0.0),
                Some(1.0),
                "bounded relation metric",
            )
        }
        MeanAbsoluteDifference
        | P95AbsoluteDifference
        | MaxAbsoluteDifference
        | ResidualMeanAbsolute
        | ResidualMaxAbsolute => crate::certifications::numerical::validate_domain_bounds(
            min,
            max,
            Some(0.0),
            None,
            "absolute relation metric",
        ),
        Slope | Intercept => Ok(()),
    }
}
impl RelationExpectation {
    pub fn spearman_at_least(min: f64) -> Result<Self, NumericError> {
        Self::with_bound(RelationMeasure::Spearman, Some(min), None)
    }
    pub fn spearman_at_most(max: f64) -> Result<Self, NumericError> {
        Self::with_bound(RelationMeasure::Spearman, None, Some(max))
    }
    pub fn with_bound(
        measure: RelationMeasure,
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
        measure: RelationMeasure,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<Self, NumericError> {
        let constraint = RelationConstraint::new(measure, min, max)?;
        validate_relation_bounds(measure, min, max)?;
        self.constraints.push(constraint);
        Ok(self)
    }
    pub(crate) fn violations(
        &self,
        observed: &RelationMetrics,
    ) -> Result<Vec<RelationConstraint>, crate::certifications::AnalysisError> {
        crate::certifications::numerical::assess(&self.constraints, |m| measure(observed, *m))
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
#[path = "relation_certification.rs"]
mod tests;
