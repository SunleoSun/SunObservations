use crate::certifications::*;
use std::fmt;

impl<T: PartialEq + fmt::Debug> Expectation<T> for T {
    type Violation = Violation;
    fn assess(&self, observed: &T) -> Result<Vec<Violation>, AnalysisError> {
        Equal(self).assess(&observed)
    }
    fn describe(&self) -> String {
        Equal(self).describe()
    }
}
impl Expectation<String> for &str {
    type Violation = Violation;
    fn assess(&self, observed: &String) -> Result<Vec<Violation>, AnalysisError> {
        Equal(*self).assess(&observed.as_str())
    }
    fn describe(&self) -> String {
        Equal(*self).describe()
    }
}
/// Finite approximate equality: absolute error <= absolute + relative * abs(expected).
#[derive(Clone, Copy, Debug)]
pub struct CloseTo {
    expected: f64,
    absolute: f64,
    relative: f64,
}
pub fn close_to(expected: f64) -> Result<CloseTo, NumericError> {
    if !expected.is_finite() {
        return Err(NumericError::InvalidParameter {
            name: "expected value".into(),
            value: expected.into(),
        });
    }
    Ok(CloseTo {
        expected,
        absolute: 1e-9,
        relative: 1e-9,
    })
}
impl CloseTo {
    fn validate_tolerance(&self) -> Result<(), NumericError> {
        let tolerance = self.absolute + self.relative * self.expected.abs();
        if !tolerance.is_finite() {
            return Err(NumericError::InvalidParameter {
                name: "combined tolerance".into(),
                value: tolerance.into(),
            });
        }
        Ok(())
    }
    pub fn absolute_tolerance(mut self, tolerance: f64) -> Result<Self, NumericError> {
        self.absolute = NonNegativeFinite::new(tolerance)?.get();
        self.validate_tolerance()?;
        Ok(self)
    }
    pub fn relative_tolerance(mut self, tolerance: f64) -> Result<Self, NumericError> {
        self.relative = NonNegativeFinite::new(tolerance)?.get();
        self.validate_tolerance()?;
        Ok(self)
    }
}
impl Expectation<f64> for CloseTo {
    type Violation = Violation;
    fn assess(&self, observed: &f64) -> Result<Vec<Violation>, AnalysisError> {
        let tolerance = self.absolute + self.relative * self.expected.abs();
        let delta = (observed - self.expected).abs();
        if !observed.is_finite() || !delta.is_finite() {
            return Err(AnalysisError {
                message:
                    "close_to requires finite evidence and nonnegative finite tolerances/arithmetic"
                        .into(),
            });
        }
        Ok(if delta <= tolerance {
            vec![]
        } else {
            vec![Violation {
                expected: self.describe(),
            }]
        })
    }
    fn describe(&self) -> String {
        format!(
            "close_to {} (absolute {}, relative {})",
            self.expected, self.absolute, self.relative
        )
    }
}
