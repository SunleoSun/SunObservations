use std::fmt::Debug;

pub fn close(actual: f64, expected: f64) {
    assert!(actual.is_finite() && expected.is_finite());
    assert!(
        (actual - expected).abs() <= 1e-10 * (1.0 + expected.abs()),
        "{actual} != {expected}"
    );
}
pub fn shuffle<T>(values: &mut [T], mut state: u64) {
    for i in (1..values.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.swap(i, (state as usize) % (i + 1));
    }
}
pub fn median(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if values.len() % 2 == 1 {
        values[values.len() / 2]
    } else {
        (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
    }
}
/// Shifted two-pass centering keeps large offsets out of the independent covariance reference.
pub fn reference_linear(values: &[(f64, f64)]) -> (f64, f64, f64, f64, f64) {
    let n = values.len() as f64;
    let (origin_x, origin_y) = values[0];
    let mx = values.iter().map(|p| p.0 - origin_x).sum::<f64>() / n;
    let my = values.iter().map(|p| p.1 - origin_y).sum::<f64>() / n;
    let xx = values
        .iter()
        .map(|p| (p.0 - origin_x - mx).powi(2))
        .sum::<f64>();
    let yy = values
        .iter()
        .map(|p| (p.1 - origin_y - my).powi(2))
        .sum::<f64>();
    let xy = values
        .iter()
        .map(|p| (p.0 - origin_x - mx) * (p.1 - origin_y - my))
        .sum::<f64>();
    (
        xy / (xx * yy).sqrt(),
        xy / n,
        xy / xx,
        (origin_y - xy / xx * origin_x) + (my - xy / xx * mx),
        xy * xy / (xx * yy),
    )
}
#[test]
fn two_pass_reference_recovers_affine_intercept_at_large_offsets() {
    let pairs: Vec<_> = (0..30)
        .map(|i| {
            let x = (1i64 << 40) + 3 * i + i64::from(i == 0);
            (x as f64 * 0.125, (3 * x + 5) as f64 * 0.25)
        })
        .collect();
    let (_, _, slope, intercept, _) = reference_linear(&pairs);
    assert!((slope - 6.).abs() <= ulps(6.));
    assert!((intercept - 1.25).abs() <= ulps(pairs[0].1));
}
pub fn reference_rank(values: &[f64]) -> Vec<f64> {
    values
        .iter()
        .map(|x| {
            let less = values.iter().filter(|v| *v < x).count();
            let equal = values.iter().filter(|v| *v == x).count();
            less as f64 + (equal as f64 + 1.0) / 2.0
        })
        .collect()
}

/// Deterministic SplitMix64 stream; scenario materialization depends only on the supplied seed.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
    pub fn between(&mut self, low: i64, high: i64) -> i64 {
        low + self.below((high - low + 1) as u64) as i64
    }
}

/// Collects every disagreement of one semantic check so a failing case reports all of them at once.
#[derive(Default)]
pub struct Mismatches(Vec<String>);
impl Mismatches {
    pub fn within(&mut self, label: &str, actual: f64, expected: f64, tolerance: f64) {
        let delta = (actual - expected).abs();
        if !(actual.is_finite()
            && expected.is_finite()
            && tolerance.is_finite()
            && delta <= tolerance)
        {
            self.0.push(format!(
                "{label}: actual {actual:e}, expected {expected:e}, tolerance {tolerance:e}"
            ));
        }
    }
    pub fn equal<T: PartialEq + Debug>(&mut self, label: &str, actual: &T, expected: &T) {
        if actual != expected {
            self.0
                .push(format!("{label}: actual {actual:?}, expected {expected:?}"));
        }
    }
    pub fn require(&mut self, label: &str, condition: bool) {
        if !condition {
            self.0.push(label.to_string());
        }
    }
    pub fn finish(self) -> Vec<String> {
        self.0
    }
}

/// Rounding allowance of a few units in the last place of the stated magnitude.
pub fn ulps(magnitude: f64) -> f64 {
    8.0 * f64::EPSILON * magnitude.abs()
}

#[derive(Clone, Debug, PartialEq, crate::certifications::DomainInput)]
pub enum KeyShape {
    Empty,
    Singleton,
    Complete,
    Holes,
    Duplicates,
    Unexpected,
    Mixed,
}
#[derive(Clone, Debug, PartialEq, crate::certifications::DomainInput)]
pub enum KeyOrigin {
    Zero,
    Ordinary,
    Maximum,
}
#[derive(Clone, Debug, PartialEq, crate::certifications::DomainInput)]
pub enum RowOrder {
    Sorted,
    Reversed,
    Shuffled,
}
#[derive(crate::certifications::DomainInput)]
pub struct KeyScenario {
    pub shape: KeyShape,
    pub origin: KeyOrigin,
    pub order: RowOrder,
}
impl KeyScenario {
    pub fn domain() -> crate::certifications::Domain<Self> {
        crate::certifications::domain!(KeyScenario { shape.all_enum_values(), origin.all_enum_values(), order.all_enum_values() })
    }
    pub fn materialize(&self) -> (Vec<u64>, u64, u64) {
        let start = match self.origin {
            KeyOrigin::Zero => 0,
            KeyOrigin::Ordinary => 10,
            KeyOrigin::Maximum => u64::MAX - 8,
        };
        let end = start + 8;
        let mut keys: Vec<u64> = match self.shape {
            KeyShape::Empty => vec![],
            KeyShape::Singleton => vec![start],
            KeyShape::Holes | KeyShape::Mixed => (start..=end)
                .filter(|k| ![1, 2, 5, 7].contains(&(k - start)))
                .collect(),
            _ => (start..=end).collect(),
        };
        if matches!(self.shape, KeyShape::Duplicates | KeyShape::Mixed) {
            keys.extend([start, start, start + 4]);
        }
        if matches!(self.shape, KeyShape::Unexpected | KeyShape::Mixed) {
            if let Some(k) = start.checked_sub(1) {
                keys.extend([k, k]);
            }
            if let Some(k) = end.checked_add(1) {
                keys.push(k);
            }
        }
        match self.order {
            RowOrder::Sorted => keys.sort_unstable(),
            RowOrder::Reversed => {
                keys.sort_unstable();
                keys.reverse();
            }
            RowOrder::Shuffled => shuffle(&mut keys, 463),
        }
        (keys, start, end)
    }
}
