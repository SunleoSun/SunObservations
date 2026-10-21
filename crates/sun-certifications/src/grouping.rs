use crate::{NumericError, NumericSeries, PreparedSource, RobustSummary, SourceEntry, Summary};
use serde::{Deserialize, Serialize};

/// Sorted key/index membership over a prepared view, without copying values or sources.
/// Groups ascend by key; members retain their original typed-view order.
pub struct GroupedView<'a, T, K> {
    view: PreparedSource<'a, T>,
    members: Vec<(K, usize)>,
}

/// One nonempty group borrowing all its typed values and source evidence.
pub struct Group<'a, T, K> {
    pub key: &'a K,
    view: PreparedSource<'a, T>,
    members: &'a [(K, usize)],
}

impl<'a, T> PreparedSource<'a, T> {
    pub fn group_by<K: Ord>(&self, key: impl Fn(&T) -> K) -> GroupedView<'a, T, K> {
        let mut members: Vec<_> = self
            .values()
            .iter()
            .enumerate()
            .map(|(i, v)| (key(v), i))
            .collect();
        crate::join::sort_indices(&mut members);
        GroupedView {
            view: self.clone(),
            members,
        }
    }
}
impl<T, K: Ord> GroupedView<'_, T, K> {
    pub fn iter(&self) -> impl Iterator<Item = Group<'_, T, K>> + Clone {
        self.members
            .chunk_by(|a, b| a.0 == b.0)
            .map(|members| Group {
                key: &members[0].0,
                view: self.view.clone(),
                members,
            })
    }
    pub fn numeric(&self, project: impl Fn(&T) -> f64) -> GroupedNumeric<'_, '_, T, K> {
        GroupedNumeric {
            groups: self,
            values: self.view.values().iter().map(project).collect(),
        }
    }
}
impl<'a, T, K> Group<'a, T, K> {
    pub fn len(&self) -> usize {
        self.members.len()
    }
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = SourceEntry<'a, T>> + DoubleEndedIterator + Clone + '_ {
        self.members.iter().map(|(_, i)| self.view.entry(*i))
    }
    pub fn values(
        &self,
    ) -> impl ExactSizeIterator<Item = &'a T> + DoubleEndedIterator + Clone + '_ {
        self.members.iter().map(|(_, i)| &self.view.values()[*i])
    }
    pub fn numeric<'b, F: Fn(&T) -> f64 + Clone + 'b>(
        &'b self,
        project: F,
    ) -> NumericSeries<impl Iterator<Item = f64> + Clone + 'b> {
        NumericSeries::new(self.values().map(project))
    }
}

/// A reusable numeric projection over deterministic groups.
pub struct GroupedNumeric<'g, 'a, T, K> {
    groups: &'g GroupedView<'a, T, K>,
    values: Vec<f64>,
}
/// Group identity and explicit per-group numeric success or failure; robust work is opt-in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupSummary<K> {
    pub key: K,
    pub member_count: usize,
    pub summary: Result<Summary, NumericError>,
    pub robust: Option<Result<RobustSummary, NumericError>>,
}
/// Machine-readable group summaries, ordered by ascending key even when a group fails.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "analysis_kind", rename = "grouped_summary")]
pub struct GroupedSummaryReport<K> {
    pub groups: Vec<GroupSummary<K>>,
}
impl<T, K: Ord + Clone> GroupedNumeric<'_, '_, T, K> {
    pub fn summaries(&self) -> GroupedSummaryReport<K> {
        self.report(false)
    }
    pub fn summaries_with_robust(&self) -> GroupedSummaryReport<K> {
        self.report(true)
    }
    fn report(&self, robust: bool) -> GroupedSummaryReport<K> {
        GroupedSummaryReport {
            groups: self
                .groups
                .iter()
                .map(|group| {
                    let series =
                        NumericSeries::new(group.members.iter().map(|(_, i)| self.values[*i]));
                    GroupSummary {
                        key: group.key.clone(),
                        member_count: group.len(),
                        summary: series.summary(),
                        robust: robust.then(|| series.robust_summary()),
                    }
                })
                .collect(),
        }
    }
}
