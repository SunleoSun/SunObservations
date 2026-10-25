use crate::certifications::{
    finite, group_end, key_group, key_indices, sort_indices, validate_bounds, KeyGroup,
    NumericError, NumericSeries, NumericValue,
};
use serde::{Deserialize, Serialize};

/// Structural count requirements, independent of statistical expectations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CountRule {
    Exact(usize),
    Minimum(usize),
    NonEmpty,
}
/// Finite, inclusive numeric range validated before evidence analysis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "(f64, f64)", into = "(f64, f64)")]
pub struct NumericRange {
    min: f64,
    max: f64,
}
impl NumericRange {
    pub fn new(min: f64, max: f64) -> Result<Self, NumericError> {
        validate_bounds(Some(min), Some(max))?;
        Ok(Self { min, max })
    }
}
impl TryFrom<(f64, f64)> for NumericRange {
    type Error = NumericError;
    fn try_from((min, max): (f64, f64)) -> Result<Self, Self::Error> {
        Self::new(min, max)
    }
}
impl From<NumericRange> for (f64, f64) {
    fn from(range: NumericRange) -> Self {
        (range.min, range.max)
    }
}
impl NumericRule {
    pub fn between(min: f64, max: f64) -> Result<Self, NumericError> {
        Ok(Self::Range(NumericRange::new(min, max)?))
    }
}
/// Finite numeric constraints, with inclusive range bounds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum NumericRule {
    Finite,
    Range(NumericRange),
    Positive,
    NonNegative,
}
/// Hard integrity evidence. Numeric non-finites retain their identity in JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum IntegrityIssue<K> {
    Count {
        expected: CountRule,
        actual: usize,
    },
    Duplicate(KeyGroup<K>),
    UnexpectedKey {
        key: K,
    },
    MissingRange {
        start: u64,
        end: u64,
    },
    InvalidKeyRange {
        start: u64,
        end: u64,
    },
    Numeric {
        index: usize,
        value: NumericValue,
        expected: NumericRule,
    },
}
/// A complete list of hard structural violations, never statistical suspicion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "integrity")]
pub struct IntegrityReport<K = ()> {
    pub record_count: usize,
    pub issues: Vec<IntegrityIssue<K>>,
}
impl<K> IntegrityReport<K> {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}
pub fn check_count(actual: usize, expected: CountRule) -> IntegrityReport {
    let valid = match expected {
        CountRule::Exact(n) => actual == n,
        CountRule::Minimum(n) => actual >= n,
        CountRule::NonEmpty => actual > 0,
    };
    IntegrityReport {
        record_count: actual,
        issues: if valid {
            Vec::new()
        } else {
            vec![IntegrityIssue::Count { expected, actual }]
        },
    }
}
pub fn check_unique<T, K: Ord + Clone>(values: &[T], key: impl Fn(&T) -> K) -> IntegrityReport<K> {
    let mut keys = key_indices(values, key);
    sort_indices(&mut keys);
    IntegrityReport {
        record_count: values.len(),
        issues: duplicate_issues(&keys),
    }
}
fn duplicate_issues<K: Ord + Clone>(keys: &[(K, usize)]) -> Vec<IntegrityIssue<K>> {
    let mut issues = Vec::new();
    let mut i = 0;
    while i < keys.len() {
        let end = group_end(keys, i);
        if end - i > 1 {
            issues.push(IntegrityIssue::Duplicate(key_group(keys, i, end)));
        }
        i = end;
    }
    issues
}
/// Exact sets also reject duplicates on either side through the canonical comparison contract.
pub fn check_same_keys<L, R, K: Ord + Clone>(
    left: &[L],
    right: &[R],
    left_key: impl Fn(&L) -> K,
    right_key: impl Fn(&R) -> K,
) -> crate::certifications::Comparison<K> {
    crate::certifications::join_by(left, right, left_key, right_key).comparison
}
/// Checks the inclusive expected range without enumerating potentially huge missing ranges.
pub fn check_contiguous<T>(
    values: &[T],
    key: impl Fn(&T) -> u64,
    start: u64,
    end: u64,
) -> IntegrityReport<u64> {
    let mut keys = key_indices(values, key);
    sort_indices(&mut keys);
    let mut report = IntegrityReport {
        record_count: values.len(),
        issues: duplicate_issues(&keys),
    };
    if start > end {
        report
            .issues
            .push(IntegrityIssue::InvalidKeyRange { start, end });
        return report;
    }
    let mut next = Some(start);
    for (k, _) in keys {
        if k < start || k > end {
            report.issues.push(IntegrityIssue::UnexpectedKey { key: k });
            continue;
        }
        if let Some(expected) = next {
            if k > expected {
                report.issues.push(IntegrityIssue::MissingRange {
                    start: expected,
                    end: k - 1,
                });
            }
            if k >= expected {
                next = k.checked_add(1);
            }
        }
    }
    if let Some(expected) = next {
        if expected <= end {
            report.issues.push(IntegrityIssue::MissingRange {
                start: expected,
                end,
            });
        }
    }
    report
}
impl<I: Iterator<Item = f64> + Clone> NumericSeries<I> {
    pub fn integrity(&self, expected: NumericRule) -> Result<IntegrityReport, NumericError> {
        let mut report = IntegrityReport {
            record_count: 0,
            issues: Vec::new(),
        };
        for (index, value) in self.iter().enumerate() {
            report.record_count += 1;
            let valid = finite(value, index).is_ok()
                && match expected {
                    NumericRule::Finite => true,
                    NumericRule::Range(NumericRange { min, max }) => value >= min && value <= max,
                    NumericRule::Positive => value > 0.0,
                    NumericRule::NonNegative => value >= 0.0,
                };
            if !valid {
                report.issues.push(IntegrityIssue::Numeric {
                    index,
                    value: value.into(),
                    expected,
                });
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
#[path = "integrity_certification.rs"]
mod certification;
