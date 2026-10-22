use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, error::Error, fmt};

/// The side whose ordering or cardinality violated a join contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}
/// An explicitly requested sorted join rejects decreasing keys instead of silently sorting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinError {
    pub side: Side,
    pub index: usize,
}
impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} key order decreases at {}", self.side, self.index)
    }
}
impl Error for JoinError {}
/// All original side-local indices for one missing or duplicated domain key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyGroup<K> {
    pub key: K,
    pub indices: Vec<usize>,
}
/// A unique 1:1 match; indices refer to the respective prepared input slices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchedKey<K> {
    pub key: K,
    pub left_index: usize,
    pub right_index: usize,
}
/// Read-only typed-key correspondence produced by joining real inputs, including one-sided and duplicate evidence.
///
/// ```compile_fail
/// use sun_observations::certifications::Comparison;
/// let _: Comparison<u64> = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Comparison<K> {
    pub(crate) left_source: Option<String>,
    pub(crate) right_source: Option<String>,
    pub(crate) left_count: usize,
    pub(crate) right_count: usize,
    pub(crate) matched: Vec<MatchedKey<K>>,
    /// Keys present on the right but absent on the left.
    pub(crate) right_only: Vec<KeyGroup<K>>,
    /// Keys present on the left but absent on the right.
    pub(crate) left_only: Vec<KeyGroup<K>>,
    pub(crate) duplicate_left: Vec<KeyGroup<K>>,
    pub(crate) duplicate_right: Vec<KeyGroup<K>>,
}
impl<K> Comparison<K> {
    pub fn left_source(&self) -> Option<&str> {
        self.left_source.as_deref()
    }
    pub fn right_source(&self) -> Option<&str> {
        self.right_source.as_deref()
    }
    pub fn left_count(&self) -> usize {
        self.left_count
    }
    pub fn right_count(&self) -> usize {
        self.right_count
    }
    pub fn matched(&self) -> &[MatchedKey<K>] {
        &self.matched
    }
    pub fn left_only(&self) -> &[KeyGroup<K>] {
        &self.left_only
    }
    pub fn right_only(&self) -> &[KeyGroup<K>] {
        &self.right_only
    }
    pub fn duplicate_left(&self) -> &[KeyGroup<K>] {
        &self.duplicate_left
    }
    pub fn duplicate_right(&self) -> &[KeyGroup<K>] {
        &self.duplicate_right
    }
    fn new(left_count: usize, right_count: usize) -> Self {
        Self {
            left_source: None,
            right_source: None,
            left_count,
            right_count,
            matched: Vec::with_capacity(left_count.min(right_count)),
            right_only: Vec::new(),
            left_only: Vec::new(),
            duplicate_left: Vec::new(),
            duplicate_right: Vec::new(),
        }
    }
    pub fn is_complete(&self) -> bool {
        self.matched.len() == self.left_count
            && self.matched.len() == self.right_count
            && self.right_only.is_empty()
            && self.left_only.is_empty()
            && self.duplicate_left.is_empty()
            && self.duplicate_right.is_empty()
    }
}
/// Borrowed aligned values with owned diagnostics. Duplicate keys are never paired.
pub struct Join<'a, L, R, K> {
    left: &'a [L],
    right: &'a [R],
    pub(crate) comparison: Comparison<K>,
}
impl<'a, L, R, K> Join<'a, L, R, K> {
    pub fn set_source_names(&mut self, left: &str, right: &str) {
        self.comparison.left_source = Some(left.into());
        self.comparison.right_source = Some(right.into());
    }
    pub fn comparison(&self) -> &Comparison<K> {
        &self.comparison
    }
    pub fn matched_pairs(&self) -> impl Iterator<Item = (&K, &L, &R)> + Clone {
        self.comparison
            .matched
            .iter()
            .map(|p| (&p.key, &self.left[p.left_index], &self.right[p.right_index]))
    }
    /// Explicitly require a complete bijection before using matched-subset statistics.
    pub fn require_complete(&self) -> Result<(), &Comparison<K>> {
        if self.comparison.is_complete() {
            Ok(())
        } else {
            Err(&self.comparison)
        }
    }
}
pub(crate) fn key_indices<T, K: Ord>(values: &[T], key: impl Fn(&T) -> K) -> Vec<(K, usize)> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| (key(v), i))
        .collect()
}
pub(crate) fn first_decrease<K: Ord>(keys: &[(K, usize)]) -> Option<usize> {
    keys.windows(2).position(|w| w[0].0 > w[1].0).map(|i| i + 1)
}
pub(crate) fn sort_indices<K: Ord>(keys: &mut [(K, usize)]) {
    if first_decrease(keys).is_some() {
        keys.sort_unstable();
    }
}
pub(crate) fn group_end<K: Ord>(keys: &[(K, usize)], start: usize) -> usize {
    let mut end = start + 1;
    while end < keys.len() && keys[end].0 == keys[start].0 {
        end += 1;
    }
    end
}
pub(crate) fn key_group<K: Clone>(keys: &[(K, usize)], start: usize, end: usize) -> KeyGroup<K> {
    KeyGroup {
        key: keys[start].0.clone(),
        indices: keys[start..end].iter().map(|(_, i)| *i).collect(),
    }
}
/// Adaptive sorted-key/index vectors: O(n+m) when ordered, otherwise O(n log n + m log m).
pub fn join_by<'a, L, R, K: Ord + Clone>(
    left: &'a [L],
    right: &'a [R],
    left_key: impl Fn(&L) -> K,
    right_key: impl Fn(&R) -> K,
) -> Join<'a, L, R, K> {
    let mut a = key_indices(left, left_key);
    let mut b = key_indices(right, right_key);
    sort_indices(&mut a);
    sort_indices(&mut b);
    merge(left, right, &a, &b)
}
/// Validates both input orders, then uses the same linear merge and duplicate semantics.
pub fn join_presorted_by<'a, L, R, K: Ord + Clone>(
    left: &'a [L],
    right: &'a [R],
    left_key: impl Fn(&L) -> K,
    right_key: impl Fn(&R) -> K,
) -> Result<Join<'a, L, R, K>, JoinError> {
    let a = key_indices(left, left_key);
    let b = key_indices(right, right_key);
    if let Some(index) = first_decrease(&a) {
        return Err(JoinError {
            side: Side::Left,
            index,
        });
    }
    if let Some(index) = first_decrease(&b) {
        return Err(JoinError {
            side: Side::Right,
            index,
        });
    }
    Ok(merge(left, right, &a, &b))
}
pub(crate) fn merge<'a, L, R, K: Ord + Clone>(
    left: &'a [L],
    right: &'a [R],
    a: &[(K, usize)],
    b: &[(K, usize)],
) -> Join<'a, L, R, K> {
    let mut comparison = Comparison::new(left.len(), right.len());
    for (ar, br) in merge_groups(a, b, Ord::cmp) {
        if ar.len() > 1 {
            comparison
                .duplicate_left
                .push(key_group(a, ar.start, ar.end));
        }
        if br.len() > 1 {
            comparison
                .duplicate_right
                .push(key_group(b, br.start, br.end));
        }
        if br.is_empty() {
            comparison.left_only.push(key_group(a, ar.start, ar.end));
        }
        if ar.is_empty() {
            comparison.right_only.push(key_group(b, br.start, br.end));
        }
        if ar.len() == 1 && br.len() == 1 {
            comparison.matched.push(MatchedKey {
                key: a[ar.start].0.clone(),
                left_index: a[ar.start].1,
                right_index: b[br.start].1,
            });
        }
    }
    Join {
        left,
        right,
        comparison,
    }
}

/// Integer-domain join that automatically chooses ordered merge, bounded dense indexing, or sorting.
/// Dense indexing is used only for unordered unique keys spanning at most n+m positions.
#[doc(hidden)]
pub fn join_u64_by<'a, L, R>(
    left: &'a [L],
    right: &'a [R],
    left_key: impl Fn(&L) -> u64,
    right_key: impl Fn(&R) -> u64,
) -> Join<'a, L, R, u64> {
    let mut a = key_indices(left, left_key);
    let mut b = key_indices(right, right_key);
    if first_decrease(&a).is_none() && first_decrease(&b).is_none() {
        return merge(left, right, &a, &b);
    }
    let min = a.iter().chain(&b).map(|v| v.0).min().unwrap();
    let max = a.iter().chain(&b).map(|v| v.0).max().unwrap();
    let span = max
        .checked_sub(min)
        .and_then(|v| v.checked_add(1))
        .and_then(|v| usize::try_from(v).ok());
    if let Some(span) = span.filter(|&s| s <= a.len().saturating_add(b.len())) {
        let mut dense_left = vec![usize::MAX; span];
        let mut dense_right = vec![usize::MAX; span];
        let mut unique = true;
        for (keys, dense) in [(&a, &mut dense_left), (&b, &mut dense_right)] {
            for &(key, index) in keys {
                let slot = &mut dense[(key - min) as usize];
                if *slot != usize::MAX {
                    unique = false;
                    break;
                }
                *slot = index;
            }
        }
        if unique {
            drop(a);
            drop(b);
            let mut comparison = Comparison::new(left.len(), right.len());
            for (offset, (i, j)) in dense_left.into_iter().zip(dense_right).enumerate() {
                let key = min + offset as u64;
                match (i != usize::MAX, j != usize::MAX) {
                    (true, true) => comparison.matched.push(MatchedKey {
                        key,
                        left_index: i,
                        right_index: j,
                    }),
                    (true, false) => comparison.left_only.push(KeyGroup {
                        key,
                        indices: vec![i],
                    }),
                    (false, true) => comparison.right_only.push(KeyGroup {
                        key,
                        indices: vec![j],
                    }),
                    (false, false) => (),
                }
            }
            return Join {
                left,
                right,
                comparison,
            };
        }
    }
    sort_indices(&mut a);
    sort_indices(&mut b);
    merge(left, right, &a, &b)
}

/// Shared ordered comparison walk; comparison must preserve each side's key ordering.
pub(crate) fn merge_groups<'a, K: Ord, F: Fn(&K, &K) -> Ordering + Clone + 'a>(
    a: &'a [(K, usize)],
    b: &'a [(K, usize)],
    compare: F,
) -> impl Iterator<Item = (std::ops::Range<usize>, std::ops::Range<usize>)> + Clone + 'a {
    let (mut i, mut j) = (0, 0);
    std::iter::from_fn(move || {
        if i == a.len() && j == b.len() {
            return None;
        }
        let order = match (a.get(i), b.get(j)) {
            (Some(a), Some(b)) => compare(&a.0, &b.0),
            (Some(_), None) => Ordering::Less,
            _ => Ordering::Greater,
        };
        let ai = if order != Ordering::Greater && i < a.len() {
            group_end(a, i)
        } else {
            i
        };
        let bj = if order != Ordering::Less && j < b.len() {
            group_end(b, j)
        } else {
            j
        };
        let ranges = (i..ai, j..bj);
        i = ai;
        j = bj;
        Some(ranges)
    })
}

#[cfg(test)]
#[path = "join_certification.rs"]
mod tests;
