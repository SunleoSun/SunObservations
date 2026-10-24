use super::*;
use std::collections::BTreeSet;

#[path = "domain_classification.rs"]
mod classification;
#[path = "domain_coverage.rs"]
mod coverage;

type Assignment = Vec<(InputPath, DomainValue)>;
const SEARCH_LIMIT: usize = 100_000;
const CONTEXT_LIMIT: usize = 4096;
const COLLECTION_LIMIT: usize = 1_000_000;
const TERCILE_STRATA: u64 = 64;
const RANGE_STRATA: u64 = 3 * TERCILE_STRATA;
const STRATIFIED_RESOLUTION: u64 = 1_000_003;
fn tercile_index(tercile: DomainTercile) -> u64 {
    match tercile {
        DomainTercile::Low => 0,
        DomainTercile::Mid => 1,
        DomainTercile::High => 2,
    }
}
fn tercile_span(length: u128, tercile: DomainTercile) -> (u128, u128) {
    let lo1 = length / 3;
    let lo2 = (length / 3) * 2 + (length % 3) * 2 / 3;
    match tercile {
        DomainTercile::Low => (0, lo1),
        DomainTercile::Mid => (lo1, lo2),
        DomainTercile::High => (lo2, length),
    }
}
fn tercile_stratum_boundary(tercile_length: u128, stratum: u128) -> u128 {
    let count = TERCILE_STRATA as u128;
    tercile_length / count * stratum + ((tercile_length % count) * stratum).div_ceil(count)
}
pub(super) fn range_stratum_bin(
    a: &DomainValue,
    b: &DomainValue,
    tercile: DomainTercile,
    value: &DomainValue,
) -> u64 {
    let base = tercile_index(tercile) * TERCILE_STRATA;
    let s = match (a, b, value) {
        (DomainValue::UInt(a), DomainValue::UInt(b), DomainValue::UInt(v)) => {
            let len = b - a + 1;
            let (t_start, t_end) = tercile_span(len, tercile);
            let t_len = t_end.saturating_sub(t_start);
            if t_len == 0 {
                0
            } else {
                let q = (v - a).saturating_sub(t_start).min(t_len - 1);
                ((q * TERCILE_STRATA as u128) / t_len).min(TERCILE_STRATA as u128 - 1) as u64
            }
        }
        (DomainValue::Int(a), DomainValue::Int(b), DomainValue::Int(v)) => {
            let len = b.saturating_sub(*a).saturating_add(1) as u128;
            let (t_start, t_end) = tercile_span(len, tercile);
            let t_len = t_end.saturating_sub(t_start);
            if t_len == 0 {
                0
            } else {
                let q = (v.saturating_sub(*a) as u128)
                    .saturating_sub(t_start)
                    .min(t_len - 1);
                ((q * TERCILE_STRATA as u128) / t_len).min(TERCILE_STRATA as u128 - 1) as u64
            }
        }
        (DomainValue::Real(a), DomainValue::Real(b), DomainValue::Real(v)) => {
            let t = if b == a {
                0.
            } else {
                ((v - a) / (b - a)).clamp(0., 1.)
            };
            let local = match tercile {
                DomainTercile::Low => (t * 3.).clamp(0., 1.),
                DomainTercile::Mid => ((t - 1. / 3.) * 3.).clamp(0., 1.),
                DomainTercile::High => ((t - 2. / 3.) * 3.).clamp(0., 1.),
            };
            ((local * TERCILE_STRATA as f64).floor() as u64).min(TERCILE_STRATA - 1)
        }
        _ => 0,
    };
    base + s
}
fn stratum_permutation(
    seed: u64,
    sector: &CaseDomainCoordinateSector,
    path: &InputPath,
) -> [u64; TERCILE_STRATA as usize] {
    let mut order: [u64; TERCILE_STRATA as usize] = std::array::from_fn(|i| i as u64);
    for i in (1..order.len()).rev() {
        let j = (local_seed(seed, &("stratum_order", sector, path, i)) as usize) % (i + 1);
        order.swap(i, j);
    }
    order
}
#[cfg(test)]
std::thread_local! {
    static SOLVE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ENUM_PAIR_CONTEXT_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(crate) fn reset_solve_calls() {
    SOLVE_CALLS.with(|c| c.set(0));
}
#[cfg(test)]
pub(crate) fn solve_calls() -> usize {
    SOLVE_CALLS.with(|c| c.get())
}
#[cfg(test)]
pub(crate) fn reset_enum_pair_context_checks() {
    ENUM_PAIR_CONTEXT_CHECKS.with(|c| c.set(0));
}
#[cfg(test)]
pub(crate) fn enum_pair_context_checks() -> usize {
    ENUM_PAIR_CONTEXT_CHECKS.with(|c| c.get())
}
fn pair_indices(len: usize, mut index: usize) -> Option<(usize, usize)> {
    for left in 0..len.saturating_sub(1) {
        let row = len - 1 - left;
        if index < row {
            return Some((left, left + 1 + index));
        }
        index -= row;
    }
    None
}

fn variant_requirements(path: &InputPath) -> Vec<(InputPath, usize)> {
    path.iter()
        .enumerate()
        .filter_map(|(index, step)| match step {
            DomainPathStep::Variant(tag) => Some((path[..index].to_vec(), *tag)),
            _ => None,
        })
        .collect()
}

fn merged_variant_requirements(paths: &[&InputPath]) -> Option<Vec<(InputPath, usize)>> {
    let mut merged = vec![];
    for path in paths {
        for (parent, tag) in variant_requirements(path) {
            if let Some((_, existing)) = merged
                .iter()
                .find(|(existing_parent, _)| existing_parent == &parent)
            {
                if *existing != tag {
                    return None;
                }
            } else {
                merged.push((parent, tag));
            }
        }
    }
    Some(merged)
}

fn required_variant(requirements: &[(InputPath, usize)], path: &InputPath) -> Option<usize> {
    requirements
        .iter()
        .find(|(parent, _)| parent == path)
        .map(|(_, tag)| *tag)
}

fn enum_choice_count(values: &[DomainValue], required: Option<usize>) -> usize {
    match required {
        Some(tag) => usize::from(
            values
                .iter()
                .any(|value| matches!(value, DomainValue::Enum(value_tag, _) if *value_tag == tag)),
        ),
        None => values.len(),
    }
}

fn enum_choice_at(
    values: &[DomainValue],
    required: Option<usize>,
    index: usize,
) -> Option<&DomainValue> {
    match required {
        Some(tag) if index == 0 => values
            .iter()
            .find(|value| matches!(value, DomainValue::Enum(value_tag, _) if *value_tag == tag)),
        Some(_) => None,
        None => values.get(index),
    }
}

fn push_value(values: &mut Vec<DomainValue>, value: DomainValue) {
    if !values.contains(&value) {
        values.push(value);
    }
}
fn normalize(schema: &DomainSchema, value: DomainValue) -> DomainValue {
    match (schema, value) {
        (DomainSchema::Real { max }, DomainValue::Real(v)) if *max == f32::MAX as f64 => {
            DomainValue::Real((v as f32) as f64)
        }
        (_, v) => v,
    }
}
fn adjacent(value: &DomainValue, schema: &DomainSchema, increasing: bool) -> Option<DomainValue> {
    let result = match value {
        DomainValue::UInt(v) => DomainValue::UInt(if increasing {
            v.checked_add(1)?
        } else {
            v.checked_sub(1)?
        }),
        DomainValue::Int(v) => DomainValue::Int(if increasing {
            v.checked_add(1)?
        } else {
            v.checked_sub(1)?
        }),
        DomainValue::Real(v) => {
            if matches!(schema,DomainSchema::Real{max} if *max==f32::MAX as f64) {
                let v = *v as f32;
                let next = if v == 0. {
                    if increasing {
                        f32::from_bits(1)
                    } else {
                        -f32::from_bits(1)
                    }
                } else {
                    let bits = v.to_bits();
                    f32::from_bits(if (v > 0.) == increasing {
                        bits + 1
                    } else {
                        bits - 1
                    })
                };
                DomainValue::Real(next as f64)
            } else {
                let next = if *v == 0. {
                    if increasing {
                        f64::from_bits(1)
                    } else {
                        -f64::from_bits(1)
                    }
                } else {
                    let bits = v.to_bits();
                    f64::from_bits(if (*v > 0.) == increasing {
                        bits + 1
                    } else {
                        bits - 1
                    })
                };
                DomainValue::Real(next)
            }
        }
        _ => return None,
    };
    schema.accepts(&result).then_some(result)
}
impl ValueSpace {
    fn contains(&self, value: &DomainValue) -> bool {
        match self {
            Self::Selected(v) => v.contains(value),
            Self::Enum(v) => v
                .iter()
                .any(|v| v.cmp_scalar(value) == Some(Ordering::Equal)),
            Self::Ranges(rs) => rs.iter().any(|(a, b)| {
                value.cmp_scalar(a).is_some_and(|c| c != Ordering::Less)
                    && value.cmp_scalar(b).is_some_and(|c| c != Ordering::Greater)
            }),
        }
    }
    fn anchors(&self) -> Vec<DomainValue> {
        match self {
            Self::Selected(v) | Self::Enum(v) => {
                let mut out = vec![];
                for v in v {
                    push_value(&mut out, v.clone());
                }
                out
            }
            Self::Ranges(rs) => {
                let mut out = vec![];
                if let (Some((a, _)), Some((_, b))) = (rs.first(), rs.last()) {
                    push_value(&mut out, a.clone());
                    push_value(&mut out, b.clone());
                }
                if !rs.is_empty() {
                    let mut lengths: Vec<_> = rs
                        .iter()
                        .map(|(a, b)| match (a, b) {
                            (DomainValue::Int(a), DomainValue::Int(b)) => {
                                (b.saturating_sub(*a).saturating_add(1)) as f64
                            }
                            (DomainValue::UInt(a), DomainValue::UInt(b)) => (b - a + 1) as f64,
                            (DomainValue::Real(a), DomainValue::Real(b)) => (b - a).max(0.),
                            _ => 0.,
                        })
                        .collect();
                    if !lengths.iter().sum::<f64>().is_finite() {
                        let maximum = lengths.iter().copied().fold(0.0, f64::max);
                        for length in &mut lengths {
                            *length /= maximum;
                        }
                    }
                    let total: f64 = lengths.iter().sum();
                    let mut center = total / 2.;
                    let mut chosen = rs[0].0.clone();
                    for ((a, b), length) in rs.iter().zip(lengths) {
                        if center < length || length == 0. {
                            chosen = match (a, b) {
                                (DomainValue::Int(a), DomainValue::Int(b)) => DomainValue::Int(
                                    a.saturating_add(center.floor() as i128).min(*b),
                                ),
                                (DomainValue::UInt(a), DomainValue::UInt(b)) => DomainValue::UInt(
                                    a.saturating_add(center.floor() as u128).min(*b),
                                ),
                                _ => a.interpolate(
                                    b,
                                    ((center / length.max(f64::MIN_POSITIVE)) * 1_000_000.) as u64,
                                    1_000_000,
                                ),
                            };
                            break;
                        }
                        center -= length;
                    }
                    push_value(&mut out, chosen);
                }
                for (a, b) in rs {
                    push_value(&mut out, a.clone());
                    push_value(&mut out, b.clone());
                    match (a, b) {
                        (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                            let len = b - a + 1;
                            for k in [1, 2] {
                                let start = a + (len / 3) * k + (len % 3) * k / 3;
                                if start > *a && start <= *b {
                                    push_value(&mut out, DomainValue::UInt(start - 1));
                                    push_value(&mut out, DomainValue::UInt(start));
                                }
                            }
                            for k in 0..3 {
                                let lo = a + (len / 3) * k + (len % 3) * k / 3;
                                let end = (len / 3) * (k + 1) + (len % 3) * (k + 1) / 3;
                                if end > 0 {
                                    let hi = a + (end - 1);
                                    if lo <= hi && lo <= *b {
                                        push_value(&mut out, DomainValue::UInt(lo + (hi - lo) / 2));
                                    }
                                }
                            }
                        }
                        (DomainValue::Int(a), DomainValue::Int(b)) => {
                            let len = b.saturating_sub(*a).saturating_add(1);
                            for k in [1, 2] {
                                let start = a.saturating_add((len / 3) * k + (len % 3) * k / 3);
                                if start > *a && start <= *b {
                                    push_value(&mut out, DomainValue::Int(start - 1));
                                    push_value(&mut out, DomainValue::Int(start));
                                }
                            }
                            for k in 0..3 {
                                let lo = a.saturating_add((len / 3) * k + (len % 3) * k / 3);
                                let hi = if k == 2 {
                                    *b
                                } else {
                                    a.saturating_add((len / 3) * (k + 1) + (len % 3) * (k + 1) / 3)
                                        .saturating_sub(1)
                                };
                                if lo <= hi && lo <= *b {
                                    push_value(&mut out, DomainValue::Int(lo + (hi - lo) / 2));
                                }
                            }
                        }
                        _ => {
                            for (n, d) in [(1, 3), (2, 3), (1, 6), (3, 6), (5, 6)] {
                                push_value(&mut out, a.interpolate(b, n, d));
                            }
                        }
                    }
                }
                out
            }
        }
    }
    fn negative(
        &self,
        schema: &DomainSchema,
    ) -> Vec<(DomainValue, DomainRegion, DomainNegativeDistance)> {
        let mut out = vec![];
        let mut add = |v: DomainValue, region: DomainRegion, distance: DomainNegativeDistance| {
            let v = normalize(schema, v);
            if schema.accepts(&v) && !self.contains(&v) && !out.iter().any(|(old, _, _)| old == &v)
            {
                out.push((v, region, distance));
            }
        };
        match self {
            Self::Ranges(ranges) => {
                if let (Some((a, _)), Some((_, b))) = (ranges.first(), ranges.last()) {
                    if let Some(v) = adjacent(a, schema, false) {
                        add(v, DomainRegion::Below, DomainNegativeDistance::Near)
                    }
                    if let Some(v) = adjacent(b, schema, true) {
                        add(v, DomainRegion::Above, DomainNegativeDistance::Near)
                    }
                    for (factor, distance) in [
                        (1, DomainNegativeDistance::Mid),
                        (8, DomainNegativeDistance::Far),
                    ] {
                        match (a, b) {
                            (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                                let span = (b - a).clamp(1, u128::MAX / 16) * factor as u128;
                                if let Some(v) = a.checked_sub(span) {
                                    add(DomainValue::UInt(v), DomainRegion::Below, distance)
                                }
                                if let Some(v) = b.checked_add(span) {
                                    add(DomainValue::UInt(v), DomainRegion::Above, distance)
                                }
                            }
                            (DomainValue::Int(a), DomainValue::Int(b)) => {
                                let span = b
                                    .saturating_sub(*a)
                                    .clamp(1, i128::MAX / 16)
                                    .saturating_mul(factor);
                                if let Some(v) = a.checked_sub(span) {
                                    add(DomainValue::Int(v), DomainRegion::Below, distance)
                                }
                                if let Some(v) = b.checked_add(span) {
                                    add(DomainValue::Int(v), DomainRegion::Above, distance)
                                }
                            }
                            (DomainValue::Real(a), DomainValue::Real(b)) => {
                                let epsilon = if matches!(schema,DomainSchema::Real{max} if *max==f32::MAX as f64)
                                {
                                    f32::EPSILON as f64
                                } else {
                                    f64::EPSILON
                                };
                                let span = (b - a).abs().max(a.abs().max(1.) * epsilon * 8.)
                                    * factor as f64;
                                add(DomainValue::Real(a - span), DomainRegion::Below, distance);
                                add(DomainValue::Real(b + span), DomainRegion::Above, distance)
                            }
                            _ => {}
                        }
                    }
                    for (i, pair) in ranges.windows(2).enumerate() {
                        let a = &pair[0].1;
                        let b = &pair[1].0;
                        for (value, distance) in [
                            (adjacent(a, schema, true), DomainNegativeDistance::Near),
                            (adjacent(b, schema, false), DomainNegativeDistance::Near),
                            (Some(a.interpolate(b, 1, 4)), DomainNegativeDistance::Mid),
                            (Some(a.interpolate(b, 3, 4)), DomainNegativeDistance::Mid),
                            (Some(a.interpolate(b, 1, 2)), DomainNegativeDistance::Far),
                        ] {
                            if let Some(value) = value {
                                if value.cmp_scalar(a) == Some(Ordering::Greater)
                                    && value.cmp_scalar(b) == Some(Ordering::Less)
                                {
                                    add(value, DomainRegion::Gap(i), distance)
                                }
                            }
                        }
                    }
                }
            }
            Self::Enum(values) => {
                if let DomainSchema::Enum(variants) = schema {
                    for tag in 0..variants.len() {
                        let value = DomainValue::Enum(tag, vec![]);
                        if !values
                            .iter()
                            .any(|v| v.cmp_scalar(&value) == Some(Ordering::Equal))
                        {
                            add(
                                value,
                                DomainRegion::ExcludedVariant,
                                DomainNegativeDistance::Near,
                            )
                        }
                    }
                }
            }
            Self::Selected(values) => {
                for value in values {
                    for increasing in [false, true] {
                        if let Some(v) = adjacent(value, schema, increasing) {
                            add(
                                v,
                                DomainRegion::OutsideSelection,
                                DomainNegativeDistance::Near,
                            )
                        }
                    }
                    for n in [-2, 2, -8, 8] {
                        if let Some(v) = value.offset(n) {
                            add(
                                v,
                                DomainRegion::OutsideSelection,
                                if n.abs() == 2 {
                                    DomainNegativeDistance::Mid
                                } else {
                                    DomainNegativeDistance::Far
                                },
                            )
                        }
                    }
                    if let DomainValue::Bool(v) = value {
                        add(
                            DomainValue::Bool(!v),
                            DomainRegion::OutsideSelection,
                            DomainNegativeDistance::Near,
                        )
                    }
                }
            }
        }
        out
    }
    fn region(&self, value: &DomainValue) -> DomainRegion {
        match self {
            Self::Enum(_) => {
                if matches!(value, DomainValue::Enum(_, _)) {
                    if self.contains(value) {
                        DomainRegion::Enum
                    } else {
                        DomainRegion::ExcludedVariant
                    }
                } else {
                    DomainRegion::OutsideSelection
                }
            }
            Self::Selected(vs) => vs
                .iter()
                .position(|v| v == value)
                .map(DomainRegion::Discrete)
                .unwrap_or(DomainRegion::OutsideSelection),
            Self::Ranges(rs) => {
                for (branch, (a, b)) in rs.iter().enumerate() {
                    if value.cmp_scalar(a).is_some_and(|c| c != Ordering::Less)
                        && value.cmp_scalar(b).is_some_and(|c| c != Ordering::Greater)
                    {
                        let tercile = match (a, b, value) {
                            (DomainValue::UInt(a), DomainValue::UInt(b), DomainValue::UInt(v)) => {
                                let len = b - a + 1;
                                let p = v - a;
                                if p < len / 3 {
                                    DomainTercile::Low
                                } else if p < (len / 3) * 2 + (len % 3) * 2 / 3 {
                                    DomainTercile::Mid
                                } else {
                                    DomainTercile::High
                                }
                            }
                            (DomainValue::Int(a), DomainValue::Int(b), DomainValue::Int(v)) => {
                                let len = b.saturating_sub(*a).saturating_add(1);
                                let p = v.saturating_sub(*a);
                                if p < len / 3 {
                                    DomainTercile::Low
                                } else if p < (len / 3) * 2 + (len % 3) * 2 / 3 {
                                    DomainTercile::Mid
                                } else {
                                    DomainTercile::High
                                }
                            }
                            _ => {
                                if value.cmp_scalar(&a.interpolate(b, 1, 3)) == Some(Ordering::Less)
                                {
                                    DomainTercile::Low
                                } else if value.cmp_scalar(&a.interpolate(b, 2, 3))
                                    == Some(Ordering::Less)
                                {
                                    DomainTercile::Mid
                                } else {
                                    DomainTercile::High
                                }
                            }
                        };
                        return DomainRegion::Range { branch, tercile };
                    }
                }
                if let Some((a, _)) = rs.first() {
                    if value.cmp_scalar(a) == Some(Ordering::Less) {
                        return DomainRegion::Below;
                    }
                }
                for (i, pair) in rs.windows(2).enumerate() {
                    if value.cmp_scalar(&pair[1].0) == Some(Ordering::Less) {
                        return DomainRegion::Gap(i);
                    }
                }
                DomainRegion::Above
            }
        }
    }
}
#[derive(Clone, Debug)]
struct Variable {
    path: InputPath,
    name: String,
    schema: DomainSchema,
    extras: Vec<DomainValue>,
}
#[derive(Clone, Debug)]
pub(crate) struct DomainTarget {
    forced: Assignment,
    violated: Vec<usize>,
    focus: Vec<InputPath>,
    composition: Option<(InputPath, InputPath, Vec<DomainValue>)>,
    pub(crate) depth: usize,
}
#[derive(Clone, Debug, PartialEq)]
struct CollectionCompositionGroup {
    forced: Assignment,
    collection_path: InputPath,
    value_path: InputPath,
    representatives: Vec<DomainValue>,
}

#[derive(Clone, Debug)]
struct EnumSourceBucket {
    path: InputPath,
    values: Vec<DomainValue>,
    contexts: Vec<Assignment>,
}

#[derive(Clone, Debug, Default)]
struct EnumPairShape {
    all: bool,
    rows: BTreeSet<usize>,
    columns: BTreeSet<usize>,
    pairs: BTreeSet<(usize, usize)>,
}
impl EnumPairShape {
    fn count(&self, left_len: usize, right_len: usize) -> usize {
        if self.all {
            return left_len.saturating_mul(right_len);
        }
        let rows = self.rows.len().min(left_len);
        let remaining_left = left_len.saturating_sub(rows);
        let base = rows.saturating_mul(right_len).saturating_add(
            self.columns
                .len()
                .min(right_len)
                .saturating_mul(remaining_left),
        );
        base.saturating_add(
            self.pairs
                .iter()
                .filter(|(left, right)| !self.rows.contains(left) && !self.columns.contains(right))
                .count(),
        )
    }

    fn pair_at(
        &self,
        left_len: usize,
        right_len: usize,
        mut index: usize,
    ) -> Option<(usize, usize)> {
        if self.all {
            return (index < left_len.saturating_mul(right_len))
                .then_some((index / right_len, index % right_len));
        }
        let rows: Vec<_> = self
            .rows
            .iter()
            .copied()
            .filter(|i| *i < left_len)
            .collect();
        let row_count = rows.len().saturating_mul(right_len);
        if index < row_count {
            return Some((rows[index / right_len], index % right_len));
        }
        index -= row_count;

        let remaining_left: Vec<_> = (0..left_len)
            .filter(|left| !self.rows.contains(left))
            .collect();
        let columns: Vec<_> = self
            .columns
            .iter()
            .copied()
            .filter(|i| *i < right_len)
            .collect();
        let column_count = columns.len().saturating_mul(remaining_left.len());
        if index < column_count {
            let column = columns[index / remaining_left.len()];
            let left = remaining_left[index % remaining_left.len()];
            return Some((left, column));
        }
        index -= column_count;

        self.pairs
            .iter()
            .copied()
            .filter(|(left, right)| {
                *left < left_len
                    && *right < right_len
                    && !self.rows.contains(left)
                    && !self.columns.contains(right)
            })
            .nth(index)
    }
}

#[derive(Clone, Debug)]
struct EnumPairBlock {
    left: usize,
    right: usize,
    shape: EnumPairShape,
    covered_by_anchor: bool,
}
/// Feasible semantic coverage, computed once from validated domain specifications.
#[derive(Clone, Debug, Default)]
pub(crate) struct DomainCoverageCatalog {
    pub positive_anchors: BTreeSet<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>,
    pub enum_values: BTreeSet<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>,
    pub pairwise_total: usize,
    pub negative_sectors: BTreeSet<CaseDomainCoordinateSector>,
}
/// Validated input schema and constraint-owned exploration spaces. No executor or evidence is stored here.
#[derive(Clone, Debug)]
pub struct CompiledDomain<T> {
    rules: Vec<DomainRule>,
    variables: Vec<Variable>,
    schema: DomainSchema,
    contexts: Vec<Assignment>,
    positive_anchors: Vec<DomainTarget>,
    enum_groups: Vec<(InputPath, Vec<DomainValue>)>,
    enum_source_buckets: Vec<EnumSourceBucket>,
    enum_pair_blocks: Vec<EnumPairBlock>,
    collection_composition_groups: Vec<CollectionCompositionGroup>,
    range_sectors: Vec<(usize, usize, DomainTercile)>,
    single_negatives: Vec<DomainTarget>,
    negative_ids: Vec<usize>,
    marker: PhantomData<fn() -> T>,
    catalog: DomainCoverageCatalog,
}
fn get<'a>(env: &'a [(InputPath, DomainValue)], path: &InputPath) -> Option<&'a DomainValue> {
    env.iter().find(|(p, _)| p == path).map(|(_, v)| v)
}
fn path_active(path: &InputPath, env: &[(InputPath, DomainValue)]) -> Option<bool> {
    let mut prefix = vec![];
    for step in path {
        match step {
            DomainPathStep::Variant(i) => {
                let DomainValue::Enum(tag, _) = get(env, &prefix)? else {
                    return Some(false);
                };
                if i != tag {
                    return Some(false);
                }
            }
            DomainPathStep::Each => {
                let mut length = prefix.clone();
                length.push(DomainPathStep::Length);
                if let Some(DomainValue::Int(0)) = get(env, &length) {
                    return Some(false);
                }
            }
            _ => {}
        }
        prefix.push(step.clone());
    }
    Some(true)
}
fn rule_active(rule: &DomainRule, env: &[(InputPath, DomainValue)]) -> Option<bool> {
    let paths = match &rule.kind {
        DomainRuleKind::Source(s) => vec![s.path.clone()],
        DomainRuleKind::Relation(r) => r.paths(),
    };
    let mut unknown = false;
    for p in paths {
        match path_active(&p, env) {
            Some(false) => return Some(false),
            None => unknown = true,
            _ => {}
        }
    }
    for gate in &rule.gates {
        match gate.evaluate(env) {
            Some(false) => return Some(false),
            None => unknown = true,
            _ => {}
        }
    }
    if unknown {
        None
    } else {
        Some(true)
    }
}
fn flatten(
    schema: &DomainSchema,
    path: InputPath,
    name: String,
    rules: &[DomainRule],
    out: &mut Vec<Variable>,
) {
    let complete=rules.iter().any(|r|matches!(&r.kind,DomainRuleKind::Source(s) if s.path==path && matches!(&s.space,ValueSpace::Selected(_))));
    match schema {
        DomainSchema::Struct(fields) if !complete => {
            for (i, (field, s)) in fields.iter().enumerate() {
                let mut p = path.clone();
                p.push(DomainPathStep::Field(i));
                flatten(
                    s,
                    p,
                    if name.is_empty() {
                        field.to_string()
                    } else {
                        format!("{name}.{field}")
                    },
                    rules,
                    out,
                )
            }
        }
        DomainSchema::Enum(variants) if !complete => {
            out.push(Variable {
                path: path.clone(),
                name: name.clone(),
                schema: schema.clone(),
                extras: vec![],
            });
            for (i, (variant, fields)) in variants.iter().enumerate() {
                for (j, (field, s)) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.extend([DomainPathStep::Variant(i), DomainPathStep::Field(j)]);
                    flatten(s, p, format!("{name}::{variant}.{field}"), rules, out)
                }
            }
        }
        DomainSchema::Sequence(each) if !complete => {
            let mut length = path.clone();
            length.push(DomainPathStep::Length);
            out.push(Variable {
                path: length,
                name: format!("{name}.length"),
                schema: usize::schema(),
                extras: vec![],
            });
            let mut p = path;
            p.push(DomainPathStep::Each);
            flatten(each, p, format!("{name}.each"), rules, out)
        }
        _ => out.push(Variable {
            path,
            name,
            schema: schema.clone(),
            extras: vec![],
        }),
    }
}
impl<T: DomainInput> CompiledDomain<T> {
    pub(crate) fn compile(domain: Domain<T>) -> Result<Self, AnalysisError> {
        let schema = T::schema();
        let mut variables = vec![];
        flatten(
            &schema,
            vec![],
            String::new(),
            &domain.rules,
            &mut variables,
        );
        for rule in &domain.rules {
            if let DomainRuleKind::Source(source) = &rule.kind {
                let var = variables
                    .iter()
                    .find(|v| v.path == source.path)
                    .ok_or_else(|| {
                        domain_error(
                            "value source conflicts with a recursive or fixed parent domain",
                        )
                    })?;
                match &source.space {
                    ValueSpace::Ranges(rs) => {
                        if rs.is_empty() {
                            return Err(domain_error(format!("{} has empty ranges", var.name)));
                        }
                        for (a, b) in rs {
                            let width_valid = match (a, b) {
                                (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                                    b.checked_sub(*a).and_then(|n| n.checked_add(1)).is_some()
                                }
                                (DomainValue::Int(a), DomainValue::Int(b)) => {
                                    b.checked_sub(*a).and_then(|n| n.checked_add(1)).is_some()
                                }
                                (DomainValue::Real(a), DomainValue::Real(b)) => (b - a).is_finite(),
                                _ => false,
                            };
                            if !width_valid
                                || !var.schema.accepts(a)
                                || !var.schema.accepts(b)
                                || a.cmp_scalar(b).is_none_or(|c| c == Ordering::Greater)
                            {
                                return Err(domain_error(format!(
                                    "{} has invalid range",
                                    var.name
                                )));
                            }
                        }
                        for pair in rs.windows(2) {
                            if pair[0].1.cmp_scalar(&pair[1].0) != Some(Ordering::Less) {
                                return Err(domain_error(format!(
                                    "{} ranges must be ordered and disjoint",
                                    var.name
                                )));
                            }
                        }
                    }
                    ValueSpace::Selected(vs) | ValueSpace::Enum(vs) => {
                        if vs.is_empty() || vs.iter().any(|v| !var.schema.accepts(v)) {
                            return Err(domain_error(format!(
                                "{} has an empty or invalid value specification",
                                var.name
                            )));
                        }
                    }
                }
                if source.path.last() == Some(&DomainPathStep::Length)
                            && source.space.anchors().iter().any(|v| {
                                !matches!(v, DomainValue::Int(n) if (0..=COLLECTION_LIMIT as i128).contains(n))
                            })
                        {
                            return Err(domain_error(
                                "collection Domain coverage exceeds materialization safety bound",
                            ));
                        }
            }
        }
        let mut ordered = vec![];
        let mut pending = variables;
        while !pending.is_empty() {
            let position = pending
                .iter()
                .position(|v| {
                    let mut deps = vec![];
                    for (i, step) in v.path.iter().enumerate() {
                        if matches!(step, DomainPathStep::Variant(_)) {
                            deps.push(v.path[..i].to_vec())
                        }
                        if matches!(step, DomainPathStep::Each) {
                            let mut p = v.path[..i].to_vec();
                            p.push(DomainPathStep::Length);
                            deps.push(p)
                        }
                    }
                    for r in &domain.rules {
                        if matches!(&r.kind,DomainRuleKind::Source(s) if s.path==v.path) {
                            for g in &r.gates {
                                deps.extend(g.paths())
                            }
                        }
                    }
                    deps.iter()
                        .all(|d| ordered.iter().any(|v: &Variable| &v.path == d))
                })
                .ok_or_else(|| {
                    domain_error("cyclic or unavailable conditional Domain dependency")
                })?;
            ordered.push(pending.remove(position));
        }
        let mut result = Self {
            rules: domain.rules,
            variables: ordered,
            schema,
            contexts: vec![],
            positive_anchors: vec![],
            enum_groups: vec![],
            enum_source_buckets: vec![],
            enum_pair_blocks: vec![],
            collection_composition_groups: vec![],
            range_sectors: vec![],
            single_negatives: vec![],
            negative_ids: vec![],
            marker: PhantomData,
            catalog: DomainCoverageCatalog::default(),
        };
        for i in 0..result.variables.len() {
            let p = result.variables[i].path.clone();
            let mut extras = vec![];
            for r in &result.rules {
                if let DomainRuleKind::Source(s) = &r.kind {
                    if s.path == p {
                        for v in s.space.anchors() {
                            push_value(&mut extras, v)
                        }
                    }
                }
                for g in &r.gates {
                    if g.paths().contains(&p) {
                        for o in [&g.left, &g.right] {
                            if let DomainOperand::Value(v) = o {
                                push_value(&mut extras, v.clone());
                                for increasing in [false, true] {
                                    if let Some(v) =
                                        adjacent(v, &result.variables[i].schema, increasing)
                                    {
                                        push_value(&mut extras, v)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            result.variables[i].extras = extras;
        }
        for _ in 0..result.variables.len().max(1) {
            let old: Vec<_> = result.variables.iter().map(|v| v.extras.clone()).collect();
            for rule in &result.rules {
                if let DomainRuleKind::Relation(r) = &rule.kind {
                    for p in r.paths() {
                        let Some(i) = result.variables.iter().position(|v| v.path == p) else {
                            return Err(domain_error(
                                "relation refers to unavailable scalar field",
                            ));
                        };
                        for o in [&r.left, &r.right] {
                            let candidates = match o {
                                DomainOperand::Value(v) => vec![v.clone()],
                                DomainOperand::Field(q) => result
                                    .variables
                                    .iter()
                                    .position(|v| &v.path == q)
                                    .map(|j| old[j].clone())
                                    .unwrap_or_default(),
                            };
                            for v in candidates {
                                if result.variables[i].schema.accepts(&v) {
                                    push_value(&mut result.variables[i].extras, v.clone());
                                    for increasing in [false, true] {
                                        if let Some(v) =
                                            adjacent(&v, &result.variables[i].schema, increasing)
                                        {
                                            if result.variables[i].schema.accepts(&v) {
                                                push_value(&mut result.variables[i].extras, v)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        result
            .solve(&[], &[])?
            .ok_or_else(|| domain_error("unsatisfiable Domain"))?;
        let mut contexts: Vec<Assignment> = vec![vec![]];
        for var in &result.variables {
            let mut deps = vec![];
            for (i, step) in var.path.iter().enumerate() {
                if matches!(step, DomainPathStep::Variant(_)) {
                    deps.push(var.path[..i].to_vec())
                }
                if matches!(step, DomainPathStep::Each) {
                    let mut p = var.path[..i].to_vec();
                    p.push(DomainPathStep::Length);
                    deps.push(p)
                }
            }
            for r in &result.rules {
                let touches = match &r.kind {
                    DomainRuleKind::Source(s) => s.path == var.path,
                    DomainRuleKind::Relation(rel) => rel.paths().contains(&var.path),
                };
                if touches {
                    for g in &r.gates {
                        deps.extend(g.paths())
                    }
                }
            }
            if matches!(var.schema, DomainSchema::Enum(_))
                && (result.variables.iter().any(|other| {
                    other.path != var.path
                        && other.path.len() > var.path.len()
                        && other.path[..var.path.len()] == var.path[..]
                        && matches!(other.path[var.path.len()], DomainPathStep::Variant(_))
                }) || result
                    .rules
                    .iter()
                    .any(|r| r.gates.iter().any(|g| g.paths().contains(&var.path))))
            {
                deps.push(var.path.clone());
            }
            let mut index = 0;
            while index < deps.len() {
                let dep = &deps[index].clone();
                for r in &result.rules {
                    if matches!(&r.kind,DomainRuleKind::Source(s) if &s.path==dep) {
                        for g in &r.gates {
                            for p in g.paths() {
                                if !deps.contains(&p) {
                                    deps.push(p)
                                }
                            }
                        }
                    }
                }
                for (i, s) in dep.iter().enumerate() {
                    if matches!(s, DomainPathStep::Variant(_)) {
                        let p = dep[..i].to_vec();
                        if !deps.contains(&p) {
                            deps.push(p)
                        }
                    }
                }
                index += 1;
            }
            deps.sort();
            deps.dedup();
            deps.sort_by_key(|p| {
                result
                    .variables
                    .iter()
                    .position(|v| &v.path == p)
                    .unwrap_or(usize::MAX)
            });
            let mut group: Vec<Assignment> = vec![vec![]];
            for dep in deps {
                let variable = result
                    .variables
                    .iter()
                    .find(|v| v.path == dep)
                    .ok_or_else(|| domain_error("unknown conditional field"))?;
                let mut choices = vec![];
                for r in &result.rules {
                    if let DomainRuleKind::Source(s) = &r.kind {
                        if s.path == dep {
                            for v in s.space.anchors() {
                                push_value(&mut choices, v)
                            }
                        }
                    }
                }
                for v in &variable.extras {
                    push_value(&mut choices, v.clone());
                }
                if dep.last() == Some(&DomainPathStep::Length) {
                    choices.sort_by(|a, b| a.cmp_scalar(b).unwrap_or(Ordering::Equal));
                    let positive = choices
                        .iter()
                        .find(|v| matches!(v,DomainValue::Int(n) if *n>0))
                        .cloned();
                    choices.retain(|v| matches!(v, DomainValue::Int(0)));
                    if let Some(v) = positive {
                        choices.push(v)
                    }
                }
                let mut next = vec![];
                for context in group {
                    if path_active(&dep, &context) == Some(false) {
                        next.push(context);
                        continue;
                    }
                    for choice in &choices {
                        let mut c = context.clone();
                        c.push((dep.clone(), choice.clone()));
                        next.push(c);
                        if next.len() > CONTEXT_LIMIT {
                            return Err(domain_error(
                                "Domain conditional preflight exceeds bounded solver capacity",
                            ));
                        }
                    }
                }
                group = next;
            }
            for context in group {
                if !contexts.contains(&context) {
                    contexts.push(context);
                    if contexts.len() > CONTEXT_LIMIT {
                        return Err(domain_error(
                            "Domain conditional preflight exceeds bounded solver capacity",
                        ));
                    }
                }
            }
        }
        for context in contexts {
            if result.solve(&context, &[])?.is_some() {
                result.contexts.push(context);
            } else if result.context_is_reachable(&context) {
                return Err(domain_error("unsatisfiable conditional Domain branch"));
            }
        }
        result.build_targets()?;
        result.catalog = result.make_catalog()?;
        Ok(result)
    }
    fn context_is_reachable(&self, c: &Assignment) -> bool {
        c.iter().all(|(p, v)| {
            path_active(p, c) != Some(false)
                && self.rules.iter().any(|r| {
                    matches!(&r.kind,DomainRuleKind::Source(s) if &s.path==p && s.space.contains(v))
                        && r.gates.iter().all(|g| g.evaluate(c) != Some(false))
                })
        })
    }
    fn candidates(
        &self,
        var: &Variable,
        source: &DomainSource,
        negative: bool,
    ) -> Vec<DomainValue> {
        let mut values = vec![];
        if negative {
            for (v, _, _) in source.space.negative(&var.schema) {
                push_value(&mut values, v)
            }
        } else {
            for v in source.space.anchors() {
                let v = normalize(&var.schema, v);
                if var.schema.accepts(&v) && source.space.contains(&v) {
                    push_value(&mut values, v)
                }
            }
        }
        for v in &var.extras {
            let v = normalize(&var.schema, v.clone());
            if var.schema.accepts(&v) && source.space.contains(&v) != negative {
                push_value(&mut values, v)
            }
        }
        values
    }
    fn possible_operand(
        &self,
        operand: &DomainOperand,
        env: &Assignment,
        forced: &[(InputPath, DomainValue)],
        violated: &[usize],
    ) -> Option<Vec<DomainValue>> {
        let path = match operand {
            DomainOperand::Value(v) => return Some(vec![v.clone()]),
            DomainOperand::Field(p) => p,
        };
        if let Some(v) = get(env, path) {
            return Some(vec![v.clone()]);
        }
        let var = self.variables.iter().find(|v| &v.path == path)?;
        let sources: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .filter_map(|(id, r)| {
                if let DomainRuleKind::Source(s) = &r.kind {
                    (&s.path == path && rule_active(r, env) != Some(false)).then_some((id, s))
                } else {
                    None
                }
            })
            .collect();
        if sources.is_empty() {
            return None;
        }
        let mut all = vec![];
        for (id, source) in sources {
            let negative = violated.contains(&id);
            if let Some(v) = get(forced, path) {
                if source.space.contains(v) != negative && var.schema.accepts(v) {
                    push_value(&mut all, v.clone())
                }
            } else {
                for v in self.candidates(var, source, negative) {
                    push_value(&mut all, v)
                }
            }
        }
        Some(all)
    }
    fn solve(
        &self,
        forced: &[(InputPath, DomainValue)],
        violated: &[usize],
    ) -> Result<Option<Assignment>, AnalysisError> {
        #[cfg(test)]
        SOLVE_CALLS.with(|c| c.set(c.get() + 1));
        let mut forced = forced.to_vec();
        let mut required: Vec<_> = forced.iter().map(|(p, _)| p.clone()).collect();
        for id in violated {
            match &self.rules[*id].kind {
                DomainRuleKind::Source(s) => required.push(s.path.clone()),
                DomainRuleKind::Relation(r) => required.extend(r.paths()),
            }
        }
        for path in required {
            for (i, step) in path.iter().enumerate() {
                if let DomainPathStep::Variant(tag) = step {
                    let parent = path[..i].to_vec();
                    let value = DomainValue::Enum(*tag, vec![]);
                    if get(&forced, &parent)
                        .is_some_and(|v| v.cmp_scalar(&value) != Some(Ordering::Equal))
                    {
                        return Ok(None);
                    }
                    if get(&forced, &parent).is_none() {
                        forced.push((parent, value))
                    }
                }
            }
        }
        fn dfs<T: DomainInput>(
            d: &CompiledDomain<T>,
            i: usize,
            env: &mut Assignment,
            forced: &[(InputPath, DomainValue)],
            violated: &[usize],
            steps: &mut usize,
        ) -> Result<Option<Assignment>, AnalysisError> {
            *steps += 1;
            if *steps > SEARCH_LIMIT {
                return Err(domain_error(
                    "Domain solver exhausted its bounded search; coverage cannot be certified",
                ));
            }
            let mut known = env.clone();
            for (p, v) in forced {
                if get(&known, p).is_none() {
                    known.push((p.clone(), v.clone()))
                }
            }
            if violated
                .iter()
                .any(|id| rule_active(&d.rules[*id], &known) == Some(false))
                || forced
                    .iter()
                    .any(|(p, _)| path_active(p, &known) == Some(false))
            {
                return Ok(None);
            }
            for (id, rule) in d.rules.iter().enumerate() {
                if let DomainRuleKind::Relation(r) = &rule.kind {
                    if rule_active(rule, &known) == Some(true) || violated.contains(&id) {
                        let desired = !violated.contains(&id);
                        if let Some(valid) = r.evaluate(&known) {
                            if valid != desired {
                                return Ok(None);
                            }
                        } else if let (Some(left), Some(right)) = (
                            d.possible_operand(&r.left, &known, forced, violated),
                            d.possible_operand(&r.right, &known, forced, violated),
                        ) {
                            if !left.iter().any(|a| {
                                right.iter().any(|b| {
                                    DomainRelation {
                                        left: DomainOperand::Value(a.clone()),
                                        right: DomainOperand::Value(b.clone()),
                                        kind: r.kind,
                                    }
                                    .evaluate(&[])
                                        == Some(desired)
                                })
                            }) {
                                return Ok(None);
                            }
                        }
                    }
                }
            }
            if i == d.variables.len() {
                if forced.iter().any(|(p, v)| get(env, p) != Some(v)) {
                    return Ok(None);
                }
                for id in violated {
                    let Some(rule) = d.rules.get(*id) else {
                        return Ok(None);
                    };
                    if rule_active(rule, env) != Some(true) {
                        return Ok(None);
                    }
                }
                return Ok(Some(env.clone()));
            }
            let var = &d.variables[i];
            if path_active(&var.path, env) == Some(false) {
                return dfs(d, i + 1, env, forced, violated, steps);
            }
            let active: Vec<_> = d
                .rules
                .iter()
                .enumerate()
                .filter_map(|(id, r)| {
                    if let DomainRuleKind::Source(s) = &r.kind {
                        (s.path == var.path && rule_active(r, env) == Some(true)).then_some((id, s))
                    } else {
                        None
                    }
                })
                .collect();
            if active.is_empty() {
                return Err(domain_error(format!(
                    "field {} has no value specification",
                    var.name
                )));
            }
            if active.len() != 1 {
                return Err(domain_error(format!(
                    "field {} has duplicate constructive value specifications",
                    var.name
                )));
            }
            let (id, source) = active[0];
            let negative = violated.contains(&id);
            let choices = if let Some(v) = get(forced, &var.path) {
                if !var.schema.accepts(v) || source.space.contains(v) == negative {
                    return Ok(None);
                }
                vec![v.clone()]
            } else {
                d.candidates(var, source, negative)
            };
            for value in choices {
                env.push((var.path.clone(), value));
                if let Some(found) = dfs(d, i + 1, env, forced, violated, steps)? {
                    return Ok(Some(found));
                }
                env.pop();
            }
            Ok(None)
        }
        dfs(self, 0, &mut vec![], &forced, violated, &mut 0).map_err(|e| {
            domain_error(format!(
                "{}; forced: {}; violated: {:?}",
                e.message,
                bounded_debug(&forced, 512),
                violated
            ))
        })
    }
    fn relation_targets(
        &self,
        id: usize,
        relation: &DomainRelation,
        context: &Assignment,
    ) -> Vec<DomainTarget> {
        let left = self
            .possible_operand(&relation.left, context, context, &[])
            .unwrap_or_default();
        let right = self
            .possible_operand(&relation.right, context, context, &[])
            .unwrap_or_default();
        let pairs = relation.invalid_pairs(&left, &right);
        if pairs.is_empty() {
            return vec![];
        }
        let mut result = vec![];
        for (depth, index) in [0, pairs.len() / 2, pairs.len() - 1]
            .into_iter()
            .enumerate()
        {
            let (a, b) = &pairs[index];
            let mut forced = context.clone();
            let mut compatible = true;
            for (operand, value) in [(&relation.left, a), (&relation.right, b)] {
                if let DomainOperand::Field(path) = operand {
                    if get(&forced, path).is_some_and(|v| v != value) {
                        compatible = false;
                        break;
                    }
                    if get(&forced, path).is_none() {
                        forced.push((path.clone(), value.clone()));
                    }
                }
            }
            if compatible {
                result.push(DomainTarget {
                    forced,
                    violated: vec![id],
                    focus: vec![],
                    composition: None,
                    depth,
                });
            }
        }
        result
    }
    fn rule_contexts(&self, rule: &DomainRule) -> Vec<Assignment> {
        let mut deps = vec![];
        let paths = match &rule.kind {
            DomainRuleKind::Source(s) => vec![s.path.clone()],
            DomainRuleKind::Relation(r) => r.paths(),
        };
        for p in &paths {
            for (i, step) in p.iter().enumerate() {
                if matches!(step, DomainPathStep::Variant(_)) {
                    deps.push(p[..i].to_vec());
                }
                if matches!(step, DomainPathStep::Each) {
                    let mut len_path = p[..i].to_vec();
                    len_path.push(DomainPathStep::Length);
                    deps.push(len_path);
                }
            }
            if matches!(&rule.kind, DomainRuleKind::Relation(_)) {
                for sr in &self.rules {
                    if matches!(&sr.kind, DomainRuleKind::Source(s) if &s.path == p) {
                        for g in &sr.gates {
                            deps.extend(g.paths());
                        }
                    }
                }
            }
        }
        for g in &rule.gates {
            deps.extend(g.paths());
        }
        let mut index = 0;
        while index < deps.len() {
            let dep = deps[index].clone();
            for sr in &self.rules {
                if matches!(&sr.kind, DomainRuleKind::Source(s) if s.path == dep) {
                    for g in &sr.gates {
                        for p in g.paths() {
                            if !deps.contains(&p) {
                                deps.push(p);
                            }
                        }
                    }
                }
            }
            for (i, s) in dep.iter().enumerate() {
                if matches!(s, DomainPathStep::Variant(_)) {
                    let p = dep[..i].to_vec();
                    if !deps.contains(&p) {
                        deps.push(p);
                    }
                }
            }
            index += 1;
        }
        deps.sort();
        deps.dedup();
        if deps.is_empty() {
            return vec![vec![]];
        }
        let mut out = vec![];
        for c in &self.contexts {
            if rule_active(rule, c) != Some(true)
                || paths.iter().any(|p| path_active(p, c) != Some(true))
            {
                continue;
            }
            let filtered: Assignment = c
                .iter()
                .filter(|(p, _)| deps.contains(p))
                .cloned()
                .collect();
            if !out.contains(&filtered) {
                out.push(filtered);
            }
        }
        if out.is_empty() {
            vec![vec![]]
        } else {
            out
        }
    }
    fn push_unique_target(targets: &mut Vec<DomainTarget>, target: DomainTarget) {
        if !targets.iter().any(|t| {
            t.forced == target.forced
                && t.violated == target.violated
                && t.focus == target.focus
                && t.composition == target.composition
        }) {
            targets.push(target);
        }
    }
    fn build_targets(&mut self) -> Result<(), AnalysisError> {
        let mut positives = vec![];
        let mut negatives = vec![];
        let rules = self.rules.clone();
        for var in self.variables.clone() {
            for (id, rule) in rules.iter().enumerate() {
                if let DomainRuleKind::Source(source) = &rule.kind {
                    if source.path != var.path {
                        continue;
                    }
                    let r_contexts = self.rule_contexts(rule);
                    for context in &r_contexts {
                        for (depth, value) in source.space.anchors().into_iter().enumerate() {
                            let value = normalize(&var.schema, value);
                            let mut forced = context.clone();
                            if get(&forced, &var.path).is_some_and(|v| v != &value) {
                                continue;
                            }
                            if get(&forced, &var.path).is_none() {
                                forced.push((var.path.clone(), value));
                            }
                            Self::push_unique_target(
                                &mut positives,
                                DomainTarget {
                                    forced,
                                    violated: vec![],
                                    focus: vec![var.path.clone()],
                                    composition: None,
                                    depth: if matches!(var.schema, DomainSchema::Enum(_)) {
                                        0
                                    } else {
                                        depth
                                    },
                                },
                            );
                        }
                        for (depth, (value, _, _distance)) in
                            source.space.negative(&var.schema).into_iter().enumerate()
                        {
                            let mut forced = context.clone();
                            forced.retain(|(p, _)| p != &var.path);
                            forced.push((var.path.clone(), value));
                            Self::push_unique_target(
                                &mut negatives,
                                DomainTarget {
                                    forced,
                                    violated: vec![id],
                                    focus: vec![var.path.clone()],
                                    composition: None,
                                    depth,
                                },
                            );
                        }
                    }
                }
            }
        }
        for (id, r) in rules.iter().enumerate() {
            if let DomainRuleKind::Relation(relation) = &r.kind {
                let r_contexts = self.rule_contexts(r);
                for context in &r_contexts {
                    for target in self.relation_targets(id, relation, context) {
                        Self::push_unique_target(&mut negatives, target);
                    }
                    Self::push_unique_target(
                        &mut negatives,
                        DomainTarget {
                            forced: context.clone(),
                            violated: vec![id],
                            focus: vec![],
                            composition: None,
                            depth: 0,
                        },
                    );
                }
            }
        }
        if positives.is_empty() {
            positives.push(DomainTarget {
                forced: vec![],
                violated: vec![],
                focus: vec![],
                composition: None,
                depth: 0,
            });
        }
        positives.sort_by_key(|t| t.depth);
        negatives.sort_by_key(|t| t.depth);
        self.positive_anchors = positives;
        self.single_negatives = negatives;

        for var in &self.variables {
            if matches!(var.schema, DomainSchema::Enum(_)) {
                let mut values = vec![];
                for rule in &self.rules {
                    if let DomainRuleKind::Source(source) = &rule.kind {
                        if source.path == var.path {
                            for value in source.space.anchors() {
                                push_value(&mut values, normalize(&var.schema, value));
                            }
                        }
                    }
                }
                if !values.is_empty() {
                    self.enum_groups.push((var.path.clone(), values));
                }
            }
        }

        let mut enum_source_buckets = vec![];
        for rule in &self.rules {
            let DomainRuleKind::Source(source) = &rule.kind else {
                continue;
            };
            let Some(var) = self.variables.iter().find(|var| var.path == source.path) else {
                continue;
            };
            if !matches!(var.schema, DomainSchema::Enum(_)) {
                continue;
            }
            let mut values = vec![];
            for value in source.space.anchors() {
                let value = normalize(&var.schema, value);
                if var.schema.accepts(&value) && source.space.contains(&value) {
                    push_value(&mut values, value);
                }
            }
            if values.is_empty() {
                continue;
            }
            enum_source_buckets.push(EnumSourceBucket {
                path: var.path.clone(),
                values,
                contexts: self.rule_contexts(rule),
            });
        }
        self.enum_source_buckets = enum_source_buckets;

        let mut enum_pair_blocks = vec![];
        for left_index in 0..self.enum_source_buckets.len() {
            for right_index in left_index + 1..self.enum_source_buckets.len() {
                let left = &self.enum_source_buckets[left_index];
                let right = &self.enum_source_buckets[right_index];
                if left.path == right.path {
                    continue;
                }
                let mut shape = EnumPairShape::default();
                for left_context in &left.contexts {
                    for right_context in &right.contexts {
                        #[cfg(test)]
                        ENUM_PAIR_CONTEXT_CHECKS.with(|c| c.set(c.get() + 1));
                        let Some(merged) = Self::merge_assignments(left_context, right_context)
                        else {
                            continue;
                        };
                        let left_required = match get(&merged, &left.path) {
                            None => Some(None),
                            Some(required) => left
                                .values
                                .iter()
                                .position(|value| {
                                    value == required
                                        || value.cmp_scalar(required) == Some(Ordering::Equal)
                                })
                                .map(Some),
                        };
                        let right_required = match get(&merged, &right.path) {
                            None => Some(None),
                            Some(required) => right
                                .values
                                .iter()
                                .position(|value| {
                                    value == required
                                        || value.cmp_scalar(required) == Some(Ordering::Equal)
                                })
                                .map(Some),
                        };
                        let (Some(left_required), Some(right_required)) =
                            (left_required, right_required)
                        else {
                            continue;
                        };
                        match (left_required, right_required) {
                            (None, None) => shape.all = true,
                            (Some(left), None) => {
                                shape.rows.insert(left);
                            }
                            (None, Some(right)) => {
                                shape.columns.insert(right);
                            }
                            (Some(left), Some(right)) => {
                                shape.pairs.insert((left, right));
                            }
                        }
                    }
                }
                if shape.count(left.values.len(), right.values.len()) > 0 {
                    enum_pair_blocks.push(EnumPairBlock {
                        left: left_index,
                        right: right_index,
                        shape,
                        covered_by_anchor: Self::enum_pair_is_covered_by_individual_anchors(
                            &left.path,
                            &right.path,
                        ),
                    });
                }
            }
        }
        self.enum_pair_blocks = enum_pair_blocks;

        let mut composition_groups = vec![];
        for var in &self.variables {
            let Some(each_pos) = var
                .path
                .iter()
                .rposition(|s| matches!(s, DomainPathStep::Each))
            else {
                continue;
            };
            let collection_path = var.path[..each_pos].to_vec();
            for rule in &self.rules {
                let DomainRuleKind::Source(source) = &rule.kind else {
                    continue;
                };
                if source.path != var.path {
                    continue;
                }
                let representatives = match &source.space {
                    ValueSpace::Enum(_) | ValueSpace::Selected(_) => source
                        .space
                        .anchors()
                        .into_iter()
                        .map(|v| normalize(&var.schema, v))
                        .collect::<Vec<_>>(),
                    ValueSpace::Ranges(rs) => {
                        let mut representatives = vec![];
                        for branch in 0..rs.len() {
                            for tercile in
                                [DomainTercile::Low, DomainTercile::High, DomainTercile::Mid]
                            {
                                if let Some(value) = source
                                    .space
                                    .anchors()
                                    .into_iter()
                                    .map(|v| normalize(&var.schema, v))
                                    .find(|v| {
                                        source.space.region(v)
                                            == DomainRegion::Range { branch, tercile }
                                    })
                                {
                                    push_value(&mut representatives, value);
                                }
                            }
                        }
                        representatives
                    }
                };
                if representatives.len() < 2 {
                    continue;
                }
                for forced in self.rule_contexts(rule) {
                    let group = CollectionCompositionGroup {
                        forced,
                        collection_path: collection_path.clone(),
                        value_path: var.path.clone(),
                        representatives: representatives.clone(),
                    };
                    if !composition_groups.contains(&group) {
                        composition_groups.push(group);
                    }
                }
            }
        }
        self.collection_composition_groups = composition_groups;

        for (id, r) in self.rules.iter().enumerate() {
            if let DomainRuleKind::Source(s) = &r.kind {
                if let ValueSpace::Ranges(rs) = &s.space {
                    for branch in 0..rs.len() {
                        for tercile in [DomainTercile::Low, DomainTercile::Mid, DomainTercile::High]
                        {
                            self.range_sectors.push((id, branch, tercile));
                        }
                    }
                }
            }
        }

        self.negative_ids = self
            .rules
            .iter()
            .enumerate()
            .filter_map(|(id, rule)| match &rule.kind {
                DomainRuleKind::Relation(_) => Some(id),
                DomainRuleKind::Source(source) => self
                    .variables
                    .iter()
                    .find(|v| v.path == source.path)
                    .and_then(|v| (!source.space.negative(&v.schema).is_empty()).then_some(id)),
            })
            .collect();
        Ok(())
    }
    fn enum_choice_contexts(&self, path: &InputPath, value: &DomainValue) -> Vec<Assignment> {
        let mut contexts = vec![];
        for rule in &self.rules {
            let DomainRuleKind::Source(source) = &rule.kind else {
                continue;
            };
            if &source.path != path || !source.space.contains(value) {
                continue;
            }
            for mut context in self.rule_contexts(rule) {
                if let Some((_, existing)) = context.iter().find(|(p, _)| p == path) {
                    if existing != value {
                        continue;
                    }
                } else {
                    context.push((path.clone(), value.clone()));
                }
                if !contexts.contains(&context) {
                    contexts.push(context);
                }
            }
        }
        contexts
    }
    fn merge_assignments(left: &Assignment, right: &Assignment) -> Option<Assignment> {
        let mut merged = left.clone();
        for (path, value) in right {
            if let Some((_, existing)) = merged.iter().find(|(p, _)| p == path) {
                if existing != value {
                    return None;
                }
            } else {
                merged.push((path.clone(), value.clone()));
            }
        }
        Some(merged)
    }
    fn force_enum_value(
        mut assignment: Assignment,
        path: &InputPath,
        value: &DomainValue,
    ) -> Option<Assignment> {
        if let Some((_, existing)) = assignment.iter().find(|(p, _)| p == path) {
            if existing != value && existing.cmp_scalar(value) != Some(Ordering::Equal) {
                return None;
            }
        } else {
            assignment.push((path.clone(), value.clone()));
        }
        Some(assignment)
    }

    fn enum_pair_block_forced(
        &self,
        block: &EnumPairBlock,
        left_index: usize,
        right_index: usize,
    ) -> Option<Assignment> {
        let left = &self.enum_source_buckets[block.left];
        let right = &self.enum_source_buckets[block.right];
        let left_value = left.values.get(left_index)?;
        let right_value = right.values.get(right_index)?;
        for left_context in &left.contexts {
            for right_context in &right.contexts {
                let Some(merged) = Self::merge_assignments(left_context, right_context) else {
                    continue;
                };
                let Some(merged) = Self::force_enum_value(merged, &left.path, left_value) else {
                    continue;
                };
                if let Some(merged) = Self::force_enum_value(merged, &right.path, right_value) {
                    return Some(merged);
                }
            }
        }
        None
    }

    fn enum_pair_coverage_count(&self) -> usize {
        self.enum_pair_blocks
            .iter()
            .map(|block| {
                let left = &self.enum_source_buckets[block.left];
                let right = &self.enum_source_buckets[block.right];
                block.shape.count(left.values.len(), right.values.len())
            })
            .fold(0usize, usize::saturating_add)
    }

    fn enum_pair_is_covered_by_individual_anchors(
        left_path: &InputPath,
        right_path: &InputPath,
    ) -> bool {
        variant_requirements(left_path)
            .iter()
            .any(|(parent, _)| parent == right_path)
            || variant_requirements(right_path)
                .iter()
                .any(|(parent, _)| parent == left_path)
    }

    fn enum_pair_target_count(&self) -> usize {
        self.enum_pair_blocks
            .iter()
            .filter(|block| !block.covered_by_anchor)
            .map(|block| {
                let left = &self.enum_source_buckets[block.left];
                let right = &self.enum_source_buckets[block.right];
                block.shape.count(left.values.len(), right.values.len())
            })
            .fold(0usize, usize::saturating_add)
    }

    fn enum_pair_target(&self, mut index: usize) -> Option<DomainTarget> {
        for block in self
            .enum_pair_blocks
            .iter()
            .filter(|block| !block.covered_by_anchor)
        {
            let left = &self.enum_source_buckets[block.left];
            let right = &self.enum_source_buckets[block.right];
            let count = block.shape.count(left.values.len(), right.values.len());
            if index >= count {
                index -= count;
                continue;
            }
            let (left_index, right_index) =
                block
                    .shape
                    .pair_at(left.values.len(), right.values.len(), index)?;
            let forced = self.enum_pair_block_forced(block, left_index, right_index)?;
            return Some(DomainTarget {
                forced,
                violated: vec![],
                focus: vec![left.path.clone(), right.path.clone()],
                composition: None,
                depth: 1,
            });
        }
        None
    }
    fn higher_order_enum_requirements(&self) -> Option<Vec<(InputPath, usize)>> {
        if self.enum_groups.len() < 3 {
            return None;
        }
        let paths: Vec<_> = self.enum_groups.iter().map(|(path, _)| path).collect();
        merged_variant_requirements(&paths)
    }
    fn higher_order_enum_count(&self) -> usize {
        let Some(requirements) = self.higher_order_enum_requirements() else {
            return 0;
        };
        self.enum_groups
            .iter()
            .fold(1usize, |count, (path, values)| {
                count.saturating_mul(enum_choice_count(
                    values,
                    required_variant(&requirements, path),
                ))
            })
            .min(CONTEXT_LIMIT)
    }
    fn higher_order_enum_target(&self, mut index: usize) -> Option<DomainTarget> {
        if index >= self.higher_order_enum_count() {
            return None;
        }
        let requirements = self.higher_order_enum_requirements()?;
        let mut forced = vec![];
        let mut focus = vec![];
        for (path, values) in &self.enum_groups {
            let required = required_variant(&requirements, path);
            let count = enum_choice_count(values, required);
            if count == 0 {
                return None;
            }
            let choice = index % count;
            index /= count;
            let value = enum_choice_at(values, required, choice)?;
            let contexts = self.enum_choice_contexts(path, value);
            let mut merged = None;
            for context in contexts {
                if let Some(candidate) = Self::merge_assignments(&forced, &context) {
                    merged = Some(candidate);
                    break;
                }
            }
            forced = merged?;
            focus.push(path.clone());
        }
        (index == 0).then_some(DomainTarget {
            forced,
            violated: vec![],
            focus,
            composition: None,
            depth: 1,
        })
    }
    fn collection_composition_pair_count(&self) -> usize {
        self.collection_composition_groups
            .iter()
            .map(|group| {
                group
                    .representatives
                    .len()
                    .saturating_mul(group.representatives.len().saturating_sub(1))
                    / 2
            })
            .fold(0usize, usize::saturating_add)
    }
    fn collection_composition_spread_count(&self) -> usize {
        self.collection_composition_groups
            .iter()
            .filter(|group| group.representatives.len() >= 3)
            .count()
    }
    fn collection_composition_count(&self) -> usize {
        self.collection_composition_pair_count()
            .saturating_add(self.collection_composition_spread_count())
    }
    fn collection_composition_target(&self, mut index: usize) -> Option<DomainTarget> {
        let pair_total = self.collection_composition_pair_count();
        if index < pair_total {
            for group in &self.collection_composition_groups {
                let len = group.representatives.len();
                let count = len.saturating_mul(len.saturating_sub(1)) / 2;
                if index < count {
                    let (left, right) = pair_indices(len, index)?;
                    return Some(DomainTarget {
                        forced: group.forced.clone(),
                        violated: vec![],
                        focus: vec![group.value_path.clone()],
                        composition: Some((
                            group.collection_path.clone(),
                            group.value_path.clone(),
                            vec![
                                group.representatives[left].clone(),
                                group.representatives[right].clone(),
                            ],
                        )),
                        depth: 1,
                    });
                }
                index -= count;
            }
            return None;
        }
        index -= pair_total;
        for group in self
            .collection_composition_groups
            .iter()
            .filter(|group| group.representatives.len() >= 3)
        {
            if index == 0 {
                return Some(DomainTarget {
                    forced: group.forced.clone(),
                    violated: vec![],
                    focus: vec![group.value_path.clone()],
                    composition: Some((
                        group.collection_path.clone(),
                        group.value_path.clone(),
                        group.representatives.iter().take(3).cloned().collect(),
                    )),
                    depth: 2,
                });
            }
            index -= 1;
        }
        None
    }
    pub fn classify(&self, input: &T) -> Result<CaseDomainCoordinate, AnalysisError> {
        self.coordinate(&input.to_domain_value())
    }
    fn composition_element_forced(
        &self,
        base_env: &Assignment,
        target: &DomainTarget,
        each_prefix: &InputPath,
        value_path: &InputPath,
        value: &DomainValue,
    ) -> Assignment {
        let mut forced: Assignment = base_env
            .iter()
            .filter(|(path, _)| !path.starts_with(each_prefix))
            .cloned()
            .collect();
        for (path, forced_value) in &target.forced {
            if path.starts_with(each_prefix) && path != value_path {
                if let Some((_, existing)) = forced.iter_mut().find(|(p, _)| p == path) {
                    *existing = forced_value.clone();
                } else {
                    forced.push((path.clone(), forced_value.clone()));
                }
            }
        }
        forced.retain(|(path, _)| path != value_path);
        forced.push((value_path.clone(), value.clone()));
        forced
    }
    fn solve_target(&self, target: &DomainTarget) -> Result<Option<Assignment>, AnalysisError> {
        let Some((coll_path, var_path, values)) = &target.composition else {
            return self.solve(&target.forced, &target.violated);
        };
        if values.is_empty() {
            return self.solve(&target.forced, &target.violated);
        }
        let mut length_path = coll_path.clone();
        length_path.push(DomainPathStep::Length);
        let Some(len_var) = self.variables.iter().find(|v| v.path == length_path) else {
            return Ok(None);
        };
        let mut lengths = vec![];
        if let Some(fixed_length) = get(&target.forced, &length_path).cloned() {
            match fixed_length {
                DomainValue::Int(length) if length >= values.len() as i128 => {
                    lengths.push(DomainValue::Int(length));
                }
                DomainValue::Int(_) => return Ok(None),
                _ => return Err(domain_error("invalid forced collection length")),
            }
        } else {
            for rule in &self.rules {
                if let DomainRuleKind::Source(source) = &rule.kind {
                    if source.path == length_path {
                        for value in self.candidates(len_var, source, false) {
                            if let DomainValue::Int(length) = value {
                                if length >= values.len() as i128 {
                                    push_value(&mut lengths, DomainValue::Int(length));
                                }
                            }
                        }
                        if lengths.is_empty() {
                            if let ValueSpace::Ranges(ranges) = &source.space {
                                let need = DomainValue::Int(values.len() as i128);
                                for (start, end) in ranges {
                                    let candidate =
                                        if need.cmp_scalar(start) == Some(Ordering::Less) {
                                            start.clone()
                                        } else {
                                            need.clone()
                                        };
                                    if candidate.cmp_scalar(end) != Some(Ordering::Greater) {
                                        push_value(&mut lengths, candidate);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut each_prefix = coll_path.clone();
        each_prefix.push(DomainPathStep::Each);
        for length in lengths {
            let mut forced = target.forced.clone();
            forced.retain(|(path, _)| path != var_path);
            if get(&forced, &length_path).is_none() {
                forced.push((length_path.clone(), length));
            }
            let mut first = forced;
            first.push((var_path.clone(), values[0].clone()));
            let Some(base_env) = self.solve(&first, &target.violated)? else {
                continue;
            };
            let mut all_ok = true;
            for value in &values[1..] {
                let forced = self.composition_element_forced(
                    &base_env,
                    target,
                    &each_prefix,
                    var_path,
                    value,
                );
                if self.solve(&forced, &target.violated)?.is_none() {
                    all_ok = false;
                    break;
                }
            }
            if all_ok {
                return Ok(Some(base_env));
            }
        }
        Ok(None)
    }
    fn materialize_value(
        &self,
        env: &Assignment,
        target: &DomainTarget,
        seed: u64,
    ) -> Result<DomainValue, AnalysisError> {
        fn build<T: DomainInput>(
            domain: &CompiledDomain<T>,
            schema: &DomainSchema,
            path: &InputPath,
            env: &Assignment,
            target: &DomainTarget,
            seed: u64,
        ) -> Result<DomainValue, AnalysisError> {
            if let Some(value) = get(env, path) {
                if !matches!(schema, DomainSchema::Enum(_))
                    || matches!(value, DomainValue::Enum(_, v) if !v.is_empty())
                {
                    return Ok(value.clone());
                }
            }
            let value = match schema {
                DomainSchema::Struct(fields) => {
                    let mut values = vec![];
                    for (i, (_, s)) in fields.iter().enumerate() {
                        let mut p = path.clone();
                        p.push(DomainPathStep::Field(i));
                        values.push(build(domain, s, &p, env, target, seed)?)
                    }
                    DomainValue::Struct(values)
                }
                DomainSchema::Enum(variants) => {
                    let Some(DomainValue::Enum(tag, _)) = get(env, path) else {
                        return Err(domain_error("missing enum selection"));
                    };
                    let mut values = vec![];
                    for (i, (_, s)) in variants[*tag].1.iter().enumerate() {
                        let mut p = path.clone();
                        p.extend([DomainPathStep::Variant(*tag), DomainPathStep::Field(i)]);
                        values.push(build(domain, s, &p, env, target, seed)?)
                    }
                    DomainValue::Enum(*tag, values)
                }
                DomainSchema::Sequence(s) => {
                    let mut length = path.clone();
                    length.push(DomainPathStep::Length);
                    let Some(DomainValue::Int(n)) = get(env, &length) else {
                        return Err(domain_error("missing collection length"));
                    };
                    let n = usize::try_from(*n)
                        .map_err(|_| domain_error("invalid collection length"))?;
                    if n > COLLECTION_LIMIT {
                        return Err(domain_error(
                            "collection materialization exceeds safety bound",
                        ));
                    }
                    let mut each = path.clone();
                    each.push(DomainPathStep::Each);
                    if n == 0 {
                        DomainValue::Sequence(vec![])
                    } else if let Some((coll_path, var_path, comp_values)) = &target.composition {
                        if coll_path == path && comp_values.len() >= 2 && target.violated.is_empty()
                        {
                            let mut templates = Vec::with_capacity(comp_values.len());
                            for (idx, v) in comp_values.iter().enumerate() {
                                let elem_env = if idx == 0 {
                                    env.clone()
                                } else {
                                    let forced = domain.composition_element_forced(
                                        env, target, &each, var_path, v,
                                    );
                                    domain.solve(&forced, &[])?.ok_or_else(|| {
                                        domain_error(
                                            "validated collection composition lost its witness",
                                        )
                                    })?
                                };
                                templates.push(build(domain, s, &each, &elem_env, target, seed)?);
                            }
                            let mut items: Vec<_> = (0..n)
                                .map(|i| templates[i % templates.len()].clone())
                                .collect();
                            let comp_sig = format!("{comp_values:?}");
                            for i in (1..items.len()).rev() {
                                let j = (local_seed(
                                    seed,
                                    &("collection_comp", path, var_path, &comp_sig, i),
                                ) as usize)
                                    % (i + 1);
                                items.swap(i, j);
                            }
                            DomainValue::Sequence(items)
                        } else {
                            DomainValue::Sequence(vec![
                                build(domain, s, &each, env, target, seed)?;
                                n
                            ])
                        }
                    } else {
                        DomainValue::Sequence(vec![build(domain, s, &each, env, target, seed)?; n])
                    }
                }
                _ => return Err(domain_error("missing scalar value")),
            };
            Ok(value)
        }
        build(self, &self.schema, &vec![], env, target, seed)
    }
    pub(crate) fn target_counts(&self) -> [usize; 3] {
        [
            self.positive_anchors
                .len()
                .saturating_add(self.enum_pair_target_count())
                .saturating_add(self.collection_composition_count())
                .max(1),
            usize::from(!self.single_negatives.is_empty()),
            usize::from(self.negative_ids.len() >= 2),
        ]
    }
    pub(crate) fn coverage_catalog(&self) -> &DomainCoverageCatalog {
        &self.catalog
    }
    fn make_catalog(&self) -> Result<DomainCoverageCatalog, AnalysisError> {
        let mut anchors = BTreeSet::new();
        let mut enums = BTreeSet::new();
        let mut negative = BTreeSet::new();
        for var in &self.variables {
            let is_enum = matches!(var.schema, DomainSchema::Enum(_));
            for (id, rule) in self.rules.iter().enumerate() {
                if let DomainRuleKind::Source(source) = &rule.kind {
                    if source.path != var.path {
                        continue;
                    }
                    for value in source.space.anchors() {
                        let value = normalize(&var.schema, value);
                        if !var.schema.accepts(&value) || !source.space.contains(&value) {
                            continue;
                        }
                        let region = source.space.region(&value);
                        let slot = source.space.slot(&value, &var.schema);
                        let sector = CaseDomainCoordinateSector {
                            constraint: id,
                            region,
                        };
                        anchors.insert((sector.clone(), slot.clone()));
                        if is_enum {
                            enums.insert((sector, slot));
                        }
                    }
                    for (_value, region, _distance) in source.space.negative(&var.schema) {
                        negative.insert(CaseDomainCoordinateSector {
                            constraint: id,
                            region,
                        });
                    }
                }
            }
        }
        for (id, rule) in self.rules.iter().enumerate() {
            if matches!(rule.kind, DomainRuleKind::Relation(_)) {
                negative.insert(CaseDomainCoordinateSector {
                    constraint: id,
                    region: DomainRegion::Relation,
                });
            }
        }
        let pairwise_total = self.enum_pair_coverage_count();
        Ok(DomainCoverageCatalog {
            positive_anchors: anchors,
            enum_values: enums,
            pairwise_total,
            negative_sectors: negative,
        })
    }
    pub(crate) fn next(
        &self,
        category: usize,
        cursor: &mut usize,
        seed: u64,
        occupied: &BTreeSet<Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>>,
    ) -> Result<Option<(T, CaseDomainCoordinate)>, AnalysisError> {
        match category {
            0 => {
                let anchors_len = self.positive_anchors.len();
                let pairs_len = self.enum_pair_target_count();
                let comp_len = self.collection_composition_count();
                let planned_len = anchors_len + pairs_len + comp_len;
                let free_limit = self.free_capacity().saturating_mul(2);
                while *cursor < planned_len + free_limit {
                    let index = *cursor;
                    *cursor += 1;
                    let target = if index < anchors_len {
                        self.positive_anchors[index].clone()
                    } else if index < anchors_len + pairs_len {
                        let Some(target) = self.enum_pair_target(index - anchors_len) else {
                            continue;
                        };
                        target
                    } else if index < planned_len {
                        let Some(target) =
                            self.collection_composition_target(index - anchors_len - pairs_len)
                        else {
                            continue;
                        };
                        target
                    } else {
                        let Some(t) = self.free_target(index - planned_len, seed)? else {
                            continue;
                        };
                        t
                    };
                    if let Some(env) = self.solve_target(&target)? {
                        let domain_value = self.materialize_value(&env, &target, seed)?;
                        let coordinate = self.coordinate(&domain_value)?;
                        if coordinate.violated != target.violated {
                            return Err(domain_error(
                                    "materialized coordinate does not match requested constraint violations",
                                ));
                        }
                        if !occupied.contains(&coordinate_key(&coordinate))
                            && !target.is_covered(&coordinate, occupied)
                        {
                            let input = T::from_domain_value(domain_value)?;
                            return Ok(Some((input, coordinate)));
                        }
                    }
                }
                Ok(None)
            }
            1 => {
                while *cursor < self.single_negatives.len() {
                    let index = *cursor;
                    *cursor += 1;
                    let target = &self.single_negatives[index];
                    if let Some(env) = self.solve(&target.forced, &target.violated)? {
                        let domain_value = self.materialize_value(&env, target, seed)?;
                        let coordinate = self.coordinate(&domain_value)?;
                        if coordinate.violated != target.violated {
                            return Err(domain_error(
                                    "materialized coordinate does not match requested constraint violations",
                                ));
                        }
                        if !occupied.contains(&coordinate_key(&coordinate))
                            && !target.is_covered(&coordinate, occupied)
                        {
                            let input = T::from_domain_value(domain_value)?;
                            return Ok(Some((input, coordinate)));
                        }
                    }
                }
                Ok(None)
            }
            2 => {
                let n = self.negative_ids.len();
                let total_pairs = n.saturating_mul(n.saturating_sub(1)) / 2;
                while *cursor < total_pairs {
                    let pair_idx = *cursor;
                    *cursor += 1;
                    let mut rem = pair_idx;
                    let mut i = 0;
                    while i + 1 < n {
                        let row_len = n - 1 - i;
                        if rem < row_len {
                            break;
                        }
                        rem -= row_len;
                        i += 1;
                    }
                    let a = self.negative_ids[i];
                    let b = self.negative_ids[i + 1 + rem];
                    let already_covered = occupied.iter().any(|entries| {
                        let neg: BTreeSet<_> = entries
                            .iter()
                            .filter(|(s, _)| s.region.is_negative())
                            .map(|(s, _)| s.constraint)
                            .collect();
                        neg.len() == 2 && neg.contains(&a) && neg.contains(&b)
                    });
                    if already_covered {
                        continue;
                    }
                    let target = DomainTarget {
                        forced: vec![],
                        violated: vec![a, b],
                        focus: vec![],
                        composition: None,
                        depth: 0,
                    };
                    if let Some(env) = self.solve(&target.forced, &target.violated)? {
                        let domain_value = self.materialize_value(&env, &target, seed)?;
                        let coordinate = self.coordinate(&domain_value)?;
                        if coordinate.violated != target.violated {
                            return Err(domain_error(
                                    "materialized coordinate does not match requested constraint violations",
                                ));
                        }
                        if !occupied.contains(&coordinate_key(&coordinate))
                            && !target.is_covered(&coordinate, occupied)
                        {
                            let input = T::from_domain_value(domain_value)?;
                            return Ok(Some((input, coordinate)));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
    fn free_capacity(&self) -> usize {
        let ranges = self.range_sectors.len() * TERCILE_STRATA as usize;
        ranges.max(self.higher_order_enum_count())
    }
    fn free_target(&self, index: usize, seed: u64) -> Result<Option<DomainTarget>, AnalysisError> {
        let level = index / 2;
        if index % 2 == 1 {
            return Ok(self.higher_order_enum_target(level));
        }
        if self.range_sectors.is_empty() {
            return Ok(None);
        }
        let sector_index = level % self.range_sectors.len();
        let round = level / self.range_sectors.len();
        if round >= TERCILE_STRATA as usize {
            return Ok(None);
        }
        let (id, branch, tercile) = self.range_sectors[sector_index];
        let DomainRuleKind::Source(source) = &self.rules[id].kind else {
            return Ok(None);
        };
        let ValueSpace::Ranges(rs) = &source.space else {
            return Ok(None);
        };
        let (a, b) = &rs[branch];
        let sector = CaseDomainCoordinateSector {
            constraint: id,
            region: DomainRegion::Range { branch, tercile },
        };
        let perm = stratum_permutation(seed, &sector, &source.path);
        let var = self
            .variables
            .iter()
            .find(|v| v.path == source.path)
            .unwrap();
        let anchors: Vec<_> = source
            .space
            .anchors()
            .into_iter()
            .map(|v| normalize(&var.schema, v))
            .collect();
        let mut non_anchor_rank = 0usize;
        let mut selected = None;
        for stratum_in_tercile in perm {
            let bin = tercile_index(tercile) * TERCILE_STRATA + stratum_in_tercile;
            let slot = CaseDomainCoordinateSlot::Stratum(bin);
            let h = local_seed(seed, &(&sector, &slot, &source.path));
            let candidate = match (a, b) {
                (DomainValue::UInt(a), DomainValue::UInt(b)) => {
                    let len = b - a + 1;
                    let (t_start, t_end) = tercile_span(len, tercile);
                    let t_len = t_end.saturating_sub(t_start);
                    if t_len == 0 {
                        continue;
                    }
                    let start =
                        t_start + tercile_stratum_boundary(t_len, stratum_in_tercile as u128);
                    let end =
                        t_start + tercile_stratum_boundary(t_len, stratum_in_tercile as u128 + 1);
                    if start >= end {
                        continue;
                    }
                    let lo = a + start;
                    let hi = a + (end - 1);
                    let width = hi - lo + 1;
                    let mut chosen = DomainValue::UInt(lo + (h as u128) % width);
                    if anchors.contains(&chosen) {
                        for offset in 1..width.min(16) {
                            let alt = DomainValue::UInt(lo + ((h as u128) + offset) % width);
                            if !anchors.contains(&alt) {
                                chosen = alt;
                                break;
                            }
                        }
                    }
                    if anchors.contains(&chosen) {
                        continue;
                    }
                    chosen
                }
                (DomainValue::Int(a), DomainValue::Int(b)) => {
                    let len = b
                        .checked_sub(*a)
                        .and_then(|n| n.checked_add(1))
                        .ok_or_else(|| {
                            domain_error("range width exceeds constructive arithmetic")
                        })?;
                    let (t_start, t_end) = tercile_span(len as u128, tercile);
                    let t_len = t_end.saturating_sub(t_start);
                    if t_len == 0 {
                        continue;
                    }
                    let start = (t_start
                        + tercile_stratum_boundary(t_len, stratum_in_tercile as u128))
                        as i128;
                    let end = (t_start
                        + tercile_stratum_boundary(t_len, stratum_in_tercile as u128 + 1))
                        as i128;
                    if start >= end {
                        continue;
                    }
                    let lo = a + start;
                    let hi = a + (end - 1);
                    let width = hi - lo + 1;
                    let mut chosen = DomainValue::Int(lo + (h as i128) % width);
                    if anchors.contains(&chosen) {
                        for offset in 1..width.min(16) {
                            let alt = DomainValue::Int(lo + ((h as i128) + offset) % width);
                            if !anchors.contains(&alt) {
                                chosen = alt;
                                break;
                            }
                        }
                    }
                    if anchors.contains(&chosen) {
                        continue;
                    }
                    chosen
                }
                _ => {
                    let jitter = (h % (STRATIFIED_RESOLUTION - 2)) + 1;
                    let chosen = normalize(
                        &var.schema,
                        a.interpolate(
                            b,
                            bin * STRATIFIED_RESOLUTION + jitter,
                            RANGE_STRATA * STRATIFIED_RESOLUTION,
                        ),
                    );
                    if anchors.contains(&chosen) {
                        continue;
                    }
                    chosen
                }
            };
            if non_anchor_rank == round {
                selected = Some(normalize(&var.schema, candidate));
                break;
            }
            non_anchor_rank += 1;
        }
        let Some(value) = selected else {
            return Ok(None);
        };
        for context in self.rule_contexts(&self.rules[id]) {
            if get(&context, &var.path).is_some_and(|v| v != &value) {
                continue;
            }
            let mut forced = context;
            if get(&forced, &var.path).is_none() {
                forced.push((var.path.clone(), value.clone()));
            }
            return Ok(Some(DomainTarget {
                forced,
                violated: vec![],
                focus: vec![var.path.clone()],
                composition: None,
                depth: index,
            }));
        }
        Ok(None)
    }
}

pub(crate) fn coordinate_key(
    c: &CaseDomainCoordinate,
) -> Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)> {
    c.entries
        .iter()
        .map(|e| (e.sector.clone(), e.slot.clone()))
        .collect()
}
pub(crate) fn local_seed(seed: u64, value: &impl std::hash::Hash) -> u64 {
    struct Stable(u64);
    impl std::hash::Hasher for Stable {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= *b as u64;
                self.0 = self.0.wrapping_mul(0x100000001b3)
            }
        }
    }
    let mut h = Stable(seed ^ 0xcbf29ce484222325);
    std::hash::Hash::hash(value, &mut h);
    std::hash::Hasher::finish(&h)
}
