use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

/// JSON-safe evidence for finite and non-finite numeric input.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum NumericValue {
    Finite(f64),
    Nan,
    PositiveInfinity,
    NegativeInfinity,
}
impl From<f64> for NumericValue {
    fn from(value: f64) -> Self {
        if value.is_nan() {
            Self::Nan
        } else if value == f64::INFINITY {
            Self::PositiveInfinity
        } else if value == f64::NEG_INFINITY {
            Self::NegativeInfinity
        } else {
            Self::Finite(value)
        }
    }
}
/// Invalid input or unrepresentable arithmetic, never a silent NaN result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NumericError {
    Empty,
    NonFinite { index: usize, value: NumericValue },
    InvalidParameter { name: String, value: NumericValue },
    ArithmeticOverflow { operation: String },
    ArithmeticUnderflow { operation: String },
}
impl fmt::Display for NumericError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "numeric analysis: {self:?}")
    }
}
impl Error for NumericError {}
pub(crate) fn checked(value: f64, operation: &str) -> Result<f64, NumericError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(NumericError::ArithmeticOverflow {
            operation: operation.into(),
        })
    }
}
pub(crate) fn finite(value: f64, index: usize) -> Result<f64, NumericError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(NumericError::NonFinite {
            index,
            value: value.into(),
        })
    }
}
pub(crate) fn nonnegative(value: f64, name: &str) -> Result<(), NumericError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(NumericError::InvalidParameter {
            name: name.into(),
            value: value.into(),
        })
    }
}

/// Full count and finite subset count without rejecting invalid values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FiniteCount {
    pub count: usize,
    pub finite_count: usize,
}
/// Population variance divides by n; sample variance is absent for n < 2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub count: usize,
    pub finite_count: usize,
    pub min: f64,
    pub max: f64,
    pub sum: f64,
    pub mean: f64,
    pub variance: f64,
    pub sample_variance: Option<f64>,
    pub standard_deviation: f64,
}
/// Type-7 linearly interpolated quartiles and unscaled median absolute deviation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RobustSummary {
    pub count: usize,
    pub median: f64,
    pub q1: f64,
    pub q3: f64,
    pub iqr: f64,
    pub mad: f64,
}

/// Reusable numeric projection. Cloning its iterator must replay the same ordered values.
#[derive(Clone)]
pub struct NumericSeries<I> {
    iter: I,
}
impl<I: Iterator<Item = f64> + Clone> NumericSeries<I> {
    pub fn new(iter: I) -> Self {
        Self { iter }
    }
    pub fn iter(&self) -> I {
        self.iter.clone()
    }
    pub fn finite_count(&self) -> FiniteCount {
        self.iter().fold(
            FiniteCount {
                count: 0,
                finite_count: 0,
            },
            |mut n, v| {
                n.count += 1;
                n.finite_count += usize::from(v.is_finite());
                n
            },
        )
    }
    pub fn summary(&self) -> Result<Summary, NumericError> {
        let mut moments = Moments::default();
        for (i, v) in self.iter().enumerate() {
            moments.push(finite(v, i)?)?;
        }
        moments.finish()
    }
    pub fn quantile(&self, probability: f64) -> Result<f64, NumericError> {
        if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
            return Err(NumericError::InvalidParameter {
                name: "probability".into(),
                value: probability.into(),
            });
        }
        let sorted = self.sorted()?;
        Ok(quantile_sorted(&sorted, probability))
    }
    pub fn robust_summary(&self) -> Result<RobustSummary, NumericError> {
        let mut sorted = self.sorted()?;
        let median = quantile_sorted(&sorted, 0.5);
        let q1 = quantile_sorted(&sorted, 0.25);
        let q3 = quantile_sorted(&sorted, 0.75);
        for v in &mut sorted {
            *v = checked((*v - median).abs(), "absolute deviation")?;
        }
        sorted.sort_unstable_by(f64::total_cmp);
        Ok(RobustSummary {
            count: sorted.len(),
            median,
            q1,
            q3,
            iqr: checked(q3 - q1, "IQR")?,
            mad: quantile_sorted(&sorted, 0.5),
        })
    }
    pub(crate) fn sorted(&self) -> Result<Vec<f64>, NumericError> {
        let mut values = self
            .iter()
            .enumerate()
            .map(|(i, v)| finite(v, i))
            .collect::<Result<Vec<_>, _>>()?;
        if values.is_empty() {
            return Err(NumericError::Empty);
        }
        values.sort_unstable_by(f64::total_cmp);
        Ok(values)
    }
}

pub(crate) fn quantile_sorted(sorted: &[f64], probability: f64) -> f64 {
    let position = (sorted.len() - 1) as f64 * probability;
    let low = position.floor() as usize;
    let high = position.ceil() as usize;
    let fraction = position - low as f64;
    sorted[low] * (1.0 - fraction) + sorted[high] * fraction
}

/// Shifted online moments avoid accumulating large common offsets into centered deltas.
#[derive(Default)]
pub(crate) struct Moments {
    count: usize,
    min: f64,
    max: f64,
    sum: f64,
    compensation: f64,
    origin: f64,
    mean: f64,
    m2: f64,
}
impl Moments {
    pub(crate) fn push(&mut self, value: f64) -> Result<(), NumericError> {
        self.count += 1;
        if self.count == 1 {
            self.min = value;
            self.max = value;
            self.origin = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        let sum = checked(self.sum + value, "sum")?;
        self.compensation += if self.sum.abs() >= value.abs() {
            (self.sum - sum) + value
        } else {
            (value - sum) + self.sum
        };
        self.sum = sum;
        let centered = checked(value - self.origin, "shifted value")?;
        let delta = checked(centered - self.mean, "mean delta")?;
        self.mean = checked(self.mean + delta / self.count as f64, "mean")?;
        self.m2 = checked(self.m2 + delta * (centered - self.mean), "variance")?;
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<Summary, NumericError> {
        if self.count == 0 {
            return Err(NumericError::Empty);
        }
        let variance = (self.m2 / self.count as f64).max(0.0);
        if variance == 0.0 && self.min != self.max {
            return Err(NumericError::ArithmeticUnderflow {
                operation: "variance".into(),
            });
        }
        let sum = checked(self.sum + self.compensation, "compensated sum")?;
        Ok(Summary {
            count: self.count,
            finite_count: self.count,
            min: self.min,
            max: self.max,
            sum,
            mean: sum / self.count as f64,
            variance,
            sample_variance: (self.count > 1).then(|| (self.m2 / (self.count - 1) as f64).max(0.0)),
            standard_deviation: variance.sqrt(),
        })
    }
}

#[cfg(test)]
#[path = "stats_certification.rs"]
mod tests;
