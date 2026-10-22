use crate::certifications::join::{key_group, merge_groups, sort_indices};
use crate::certifications::{finite, pearson, spearman, KeyGroup, Metric, NumericError, Side};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

/// Maximum number of candidate shifts in one diagnostic search.
pub const MAX_LAG_CANDIDATES: i128 = 4097;
/// Validated inclusive domain-key shifts bounded by MAX_LAG_CANDIDATES.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LagRangeInput")]
pub struct LagRange {
    start: i64,
    end: i64,
}
#[derive(Deserialize)]
struct LagRangeInput {
    start: i64,
    end: i64,
}
impl TryFrom<LagRangeInput> for LagRange {
    type Error = LagError;
    fn try_from(input: LagRangeInput) -> Result<Self, Self::Error> {
        Self::new(input.start, input.end)
    }
}
impl LagRange {
    pub fn new(start: i64, end: i64) -> Result<Self, LagError> {
        let width = i128::from(end) - i128::from(start) + 1;
        if !(1..=MAX_LAG_CANDIDATES).contains(&width) {
            return Err(LagError::InvalidRange { start, end });
        }
        Ok(Self { start, end })
    }
    pub fn start(self) -> i64 {
        self.start
    }
    pub fn end(self) -> i64 {
        self.end
    }
}
/// Correlation owner used for each candidate; strength is its absolute value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LagMeasure {
    Pearson,
    Spearman,
}
/// Finite search configuration, independent of scenario acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LagConfig {
    pub range: LagRange,
    pub measure: LagMeasure,
}
/// Counts over unique domain keys. Boundary rows lie outside the opposite shifted extent;
/// missing rows are unmatched keys inside that extent, never silently treated as complete.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LagAlignment {
    pub pair_count: usize,
    pub boundary_left: usize,
    pub boundary_right: usize,
    pub right_only: usize,
    pub left_only: usize,
}
impl LagAlignment {
    pub fn has_interior_gaps(&self) -> bool {
        self.right_only != 0 || self.left_only != 0
    }
}
/// Matched-subset diagnostic with explicit completeness evidence and undefined metrics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LagCandidate {
    pub lag: i64,
    pub alignment: LagAlignment,
    pub relation: Metric,
}
/// Strongest defined candidate; exact ties prefer smallest |lag|, then smallest signed lag.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BestLag {
    pub lag: i64,
    pub relation: f64,
    pub strength: f64,
}
/// Machine-readable bounded shift search. Positive d compares left k with right k+d.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "lag_relation")]
pub struct LagReport {
    pub config: LagConfig,
    pub left_source: Option<String>,
    pub right_source: Option<String>,
    pub left_count: usize,
    pub right_count: usize,
    pub candidates: Vec<LagCandidate>,
    pub best: Option<BestLag>,
}
impl LagReport {
    pub fn zero_lag(&self) -> Option<&LagCandidate> {
        self.candidates.iter().find(|c| c.lag == 0)
    }
}
/// Hard prerequisites; duplicate indices are original side-local coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LagError {
    InvalidRange {
        start: i64,
        end: i64,
    },
    DuplicateKeys {
        left: Vec<KeyGroup<i64>>,
        right: Vec<KeyGroup<i64>>,
    },
    Projection {
        side: Side,
        error: NumericError,
    },
    Numeric {
        lag: i64,
        error: NumericError,
    },
}
impl fmt::Display for LagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lag analysis prerequisite failed: {self:?}")
    }
}
impl Error for LagError {}

struct LagInput {
    keys: Vec<(i64, usize)>,
    values: Vec<f64>,
}
impl LagInput {
    fn prepare<T>(
        values: &[T],
        project: impl Fn(&T) -> (i64, f64),
        side: Side,
    ) -> Result<Self, LagError> {
        let mut keys = Vec::with_capacity(values.len());
        let mut numbers = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            let (key, number) = project(value);
            numbers
                .push(finite(number, index).map_err(|error| LagError::Projection { side, error })?);
            keys.push((key, index));
        }
        sort_indices(&mut keys);
        Ok(Self {
            keys,
            values: numbers,
        })
    }
    fn duplicates(&self) -> Vec<KeyGroup<i64>> {
        merge_groups(&self.keys, &[], Ord::cmp)
            .filter_map(|(r, _)| (r.len() > 1).then(|| key_group(&self.keys, r.start, r.end)))
            .collect()
    }
}

/// Compare left key k with right key k+d for each inclusive candidate d, using i128 shift
/// arithmetic so i64 boundary keys cannot wrap. Keys/values are projected once, then sorted
/// once if needed. Pearson costs O(w(n+m)) time and O(n+m+w) storage; Spearman additionally
/// ranks each matched subset. No matched-pair vector is allocated for Pearson.
/// Best ranks |correlation| even with interior gaps: it is diagnostic, not acceptance.
pub fn lag_relation_by<L, R>(
    left: &[L],
    right: &[R],
    left_project: impl Fn(&L) -> (i64, f64),
    right_project: impl Fn(&R) -> (i64, f64),
    config: LagConfig,
) -> Result<LagReport, LagError> {
    let width = i128::from(config.range.end) - i128::from(config.range.start) + 1;
    let a = LagInput::prepare(left, left_project, Side::Left)?;
    let b = LagInput::prepare(right, right_project, Side::Right)?;
    let (duplicate_left, duplicate_right) = (a.duplicates(), b.duplicates());
    if !duplicate_left.is_empty() || !duplicate_right.is_empty() {
        return Err(LagError::DuplicateKeys {
            left: duplicate_left,
            right: duplicate_right,
        });
    }
    let mut report = LagReport {
        config,
        left_source: None,
        right_source: None,
        left_count: left.len(),
        right_count: right.len(),
        candidates: Vec::with_capacity(width as usize),
        best: None,
    };
    for lag in config.range.start..=config.range.end {
        let shift = |key: i64| i128::from(key) + i128::from(lag);
        let mut alignment = LagAlignment::default();
        let pairs = merge_groups(&a.keys, &b.keys, |x, y| shift(*x).cmp(&i128::from(*y)))
            .filter_map(|(ar, br)| {
                if !ar.is_empty() && !br.is_empty() {
                    alignment.pair_count += 1;
                    return Some((a.values[a.keys[ar.start].1], b.values[b.keys[br.start].1]));
                }
                if !ar.is_empty() {
                    let key = shift(a.keys[ar.start].0);
                    if b.keys
                        .first()
                        .zip(b.keys.last())
                        .is_none_or(|(lo, hi)| key < i128::from(lo.0) || key > i128::from(hi.0))
                    {
                        alignment.boundary_left += 1;
                    } else {
                        alignment.left_only += 1;
                    }
                } else {
                    let key = i128::from(b.keys[br.start].0);
                    if a.keys
                        .first()
                        .zip(a.keys.last())
                        .is_none_or(|(lo, hi)| key < shift(lo.0) || key > shift(hi.0))
                    {
                        alignment.boundary_right += 1;
                    } else {
                        alignment.right_only += 1;
                    }
                }
                None
            });
        let relation = match config.measure {
            LagMeasure::Pearson => pearson(pairs),
            LagMeasure::Spearman => spearman(pairs),
        }
        .map_err(|error| LagError::Numeric { lag, error })?;
        if let Metric::Defined(value) = relation {
            let strength = value.abs();
            if report.best.as_ref().is_none_or(|best| {
                strength > best.strength
                    || (strength == best.strength
                        && (lag.unsigned_abs(), lag) < (best.lag.unsigned_abs(), best.lag))
            }) {
                report.best = Some(BestLag {
                    lag,
                    relation: value,
                    strength,
                });
            }
        }
        report.candidates.push(LagCandidate {
            lag,
            alignment,
            relation,
        });
    }
    Ok(report)
}

/// Scenario-owned expected best shift; undefined or interior-incomplete best candidates fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LagExpectation {
    pub best_lag: i64,
}
/// Why diagnostic evidence did not establish the expected alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LagViolation {
    NoDefinedCandidate,
    UnexpectedBestLag,
    InteriorMissingKeys,
}

impl LagExpectation {
    pub(crate) fn violations(&self, observed: &LagReport) -> Vec<LagViolation> {
        let mut violated = Vec::new();
        match &observed.best {
            None => violated.push(LagViolation::NoDefinedCandidate),
            Some(best) => {
                if best.lag != self.best_lag {
                    violated.push(LagViolation::UnexpectedBestLag);
                }
                if observed
                    .candidates
                    .iter()
                    .find(|c| c.lag == best.lag)
                    .is_none_or(|c| c.alignment.has_interior_gaps())
                {
                    violated.push(LagViolation::InteriorMissingKeys);
                }
            }
        }
        violated
    }
}

#[cfg(test)]
#[path = "lag_certification.rs"]
mod tests;
