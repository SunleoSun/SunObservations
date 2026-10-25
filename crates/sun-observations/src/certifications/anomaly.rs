use crate::certifications::{
    checked, finite, nonnegative, NumericError, NumericSeries, RobustSummary,
};
use serde::{Deserialize, Serialize};

/// A finite nonnegative threshold, validated before analysis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct NonNegativeFinite(f64);
impl NonNegativeFinite {
    pub fn new(value: f64) -> Result<Self, NumericError> {
        nonnegative(value, "nonnegative threshold")?;
        Ok(Self(value))
    }
    pub fn get(self) -> f64 {
        self.0
    }
}
impl TryFrom<f64> for NonNegativeFinite {
    type Error = NumericError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<NonNegativeFinite> for f64 {
    fn from(value: NonNegativeFinite) -> Self {
        value.0
    }
}
impl ZeroMadPolicy {
    pub fn absolute_tolerance(value: f64) -> Result<Self, NumericError> {
        NonNegativeFinite::new(value).map(Self::AbsoluteTolerance)
    }
}
const MODIFIED_Z_SCALE: f64 = 0.6744897501960817;

/// Explicit zero-MAD policy. A tolerance is in the original numeric units, not z-score units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ZeroMadPolicy {
    ExactMedian,
    AbsoluteTolerance(NonNegativeFinite),
}
/// Suspicion thresholds; modified z = 0.6744897501960817 * (value - median) / MAD.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutlierConfig {
    threshold: NonNegativeFinite,
    zero_mad: ZeroMadPolicy,
}
impl OutlierConfig {
    pub fn new(threshold: f64, zero_mad: ZeroMadPolicy) -> Result<Self, NumericError> {
        Ok(Self {
            threshold: NonNegativeFinite::new(threshold)?,
            zero_mad,
        })
    }
    pub fn threshold(self) -> f64 {
        self.threshold.get()
    }
    pub fn zero_mad(self) -> ZeroMadPolicy {
        self.zero_mad
    }
}
impl Default for OutlierConfig {
    fn default() -> Self {
        Self {
            threshold: NonNegativeFinite(3.5),
            zero_mad: ZeroMadPolicy::ExactMedian,
        }
    }
}
/// Zero-MAD evidence is represented explicitly instead of infinity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum RobustScore {
    ModifiedZ(f64),
    ZeroMadDeviation { signed_deviation: f64 },
}
/// A suspicious value's original series-local index and numeric evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outlier {
    pub index: usize,
    pub value: f64,
    pub score: RobustScore,
}
/// Robust suspicion report; a consuming certification decides whether findings are failures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "mad_outliers")]
pub struct OutlierReport {
    pub config: OutlierConfig,
    pub summary: RobustSummary,
    pub suspicious: Vec<Outlier>,
}
#[derive(Deserialize)]
struct JumpConfigInput {
    absolute_threshold: Option<f64>,
    relative_threshold: Option<f64>,
    denominator_floor: f64,
}
impl TryFrom<JumpConfigInput> for JumpConfig {
    type Error = NumericError;
    fn try_from(input: JumpConfigInput) -> Result<Self, Self::Error> {
        Self::new(
            input.absolute_threshold,
            input.relative_threshold,
            input.denominator_floor,
        )
    }
}
impl JumpConfig {
    pub fn new(
        absolute_threshold: Option<f64>,
        relative_threshold: Option<f64>,
        denominator_floor: f64,
    ) -> Result<Self, NumericError> {
        if absolute_threshold.is_none() && relative_threshold.is_none() {
            return Err(NumericError::InvalidParameter {
                name: "missing jump threshold".into(),
                value: 0.0.into(),
            });
        }
        let config = Self {
            absolute_threshold,
            relative_threshold,
            denominator_floor,
        };
        for (value, name) in [
            (config.absolute_threshold, "absolute threshold"),
            (config.relative_threshold, "relative threshold"),
        ] {
            if let Some(v) = value {
                nonnegative(v, name)?;
            }
        }
        if !config.denominator_floor.is_finite() || config.denominator_floor <= 0.0 {
            return Err(NumericError::InvalidParameter {
                name: "denominator_floor".into(),
                value: config.denominator_floor.into(),
            });
        }
        Ok(config)
    }
    pub fn absolute_threshold(self) -> Option<f64> {
        self.absolute_threshold
    }
    pub fn relative_threshold(self) -> Option<f64> {
        self.relative_threshold
    }
    pub fn denominator_floor(self) -> f64 {
        self.denominator_floor
    }
}
/// Thresholds use strict greater-than; either enabled threshold can flag a transition.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "JumpConfigInput")]
pub struct JumpConfig {
    absolute_threshold: Option<f64>,
    relative_threshold: Option<f64>,
    /// Relative jump = abs(next - previous) / max(abs(previous), denominator_floor).
    denominator_floor: f64,
}
/// Both endpoints and absolute/relative magnitudes of one adjacent transition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Jump {
    pub from_index: usize,
    pub to_index: usize,
    pub from_value: f64,
    pub to_value: f64,
    pub signed_change: f64,
    pub absolute_change: f64,
    pub relative_change: f64,
}
/// Ordered transition evidence, with the earliest maximum retained on ties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "jumps")]
pub struct JumpReport {
    pub config: JumpConfig,
    pub record_count: usize,
    pub transition_count: usize,
    pub maximum_absolute: Option<Jump>,
    pub maximum_relative: Option<Jump>,
    pub suspicious: Vec<Jump>,
}
impl<I: Iterator<Item = f64> + Clone> NumericSeries<I> {
    pub fn outliers(&self, config: OutlierConfig) -> Result<OutlierReport, NumericError> {
        let tolerance = match config.zero_mad {
            ZeroMadPolicy::ExactMedian => 0.0,
            ZeroMadPolicy::AbsoluteTolerance(v) => v.get(),
        };
        let summary = self.robust_summary()?;
        let mut suspicious = Vec::new();
        for (index, value) in self.iter().enumerate() {
            let deviation = checked(value - summary.median, "robust deviation")?;
            let score = if summary.mad == 0.0 {
                if deviation.abs() <= tolerance {
                    continue;
                }
                RobustScore::ZeroMadDeviation {
                    signed_deviation: deviation,
                }
            } else {
                let score = checked(
                    MODIFIED_Z_SCALE * (deviation / summary.mad),
                    "modified z-score",
                )?;
                if score.abs() <= config.threshold.get() {
                    continue;
                }
                RobustScore::ModifiedZ(score)
            };
            suspicious.push(Outlier {
                index,
                value,
                score,
            });
        }
        Ok(OutlierReport {
            config,
            summary,
            suspicious,
        })
    }
    pub fn jumps(&self, config: JumpConfig) -> Result<JumpReport, NumericError> {
        let mut report = JumpReport {
            config,
            record_count: 0,
            transition_count: 0,
            maximum_absolute: None,
            maximum_relative: None,
            suspicious: Vec::new(),
        };
        let mut previous: Option<f64> = None;
        for (index, value) in self.iter().enumerate() {
            finite(value, index)?;
            report.record_count += 1;
            if let Some(from_value) = previous {
                let signed_change = checked(value - from_value, "adjacent difference")?;
                let absolute_change = signed_change.abs();
                let relative_change = checked(
                    absolute_change / from_value.abs().max(config.denominator_floor),
                    "relative jump",
                )?;
                let jump = Jump {
                    from_index: index - 1,
                    to_index: index,
                    from_value,
                    to_value: value,
                    signed_change,
                    absolute_change,
                    relative_change,
                };
                report.transition_count += 1;
                if report
                    .maximum_absolute
                    .as_ref()
                    .is_none_or(|j| absolute_change > j.absolute_change)
                {
                    report.maximum_absolute = Some(jump.clone());
                }
                if report
                    .maximum_relative
                    .as_ref()
                    .is_none_or(|j| relative_change > j.relative_change)
                {
                    report.maximum_relative = Some(jump.clone());
                }
                if config
                    .absolute_threshold
                    .is_some_and(|t| absolute_change > t)
                    || config
                        .relative_threshold
                        .is_some_and(|t| relative_change > t)
                {
                    report.suspicious.push(jump);
                }
            }
            previous = Some(value);
        }
        Ok(report)
    }
}

#[cfg(test)]
#[path = "anomaly_certification.rs"]
mod certification;
