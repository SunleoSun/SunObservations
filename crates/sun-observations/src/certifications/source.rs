use crate::certifications::*;
use std::{fmt, sync::Arc};

/// Optional adapter-owned coordinates; generic analysis never interprets them.
pub trait SourceProvenance {
    fn describe(&self, index: usize, out: &mut dyn fmt::Write) -> fmt::Result;
}
/// A borrowed value with source-local identity and optional external provenance.
pub struct SourceEntry<'a, T> {
    pub value: &'a T,
    pub index: usize,
    pub provenance: Option<&'a dyn SourceProvenance>,
}
/// A named, borrowed collection. Cloning shares values and provenance, never payloads.
pub struct PreparedSource<'a, T> {
    name: Arc<str>,
    values: &'a [T],
    provenance: Option<&'a dyn SourceProvenance>,
}
impl<T> Clone for PreparedSource<'_, T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            values: self.values,
            provenance: self.provenance,
        }
    }
}
pub fn source<T>(name: impl Into<Arc<str>>, value: &T) -> PreparedSource<'_, T> {
    source_slice(name, std::slice::from_ref(value))
}
pub fn source_slice<T>(name: impl Into<Arc<str>>, values: &[T]) -> PreparedSource<'_, T> {
    PreparedSource {
        name: name.into(),
        values,
        provenance: None,
    }
}
impl<'a, T> PreparedSource<'a, T> {
    pub fn with_provenance(mut self, provenance: &'a dyn SourceProvenance) -> Self {
        self.provenance = Some(provenance);
        self
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn values(&self) -> &'a [T] {
        self.values
    }
    pub fn entry(&self, index: usize) -> SourceEntry<'a, T> {
        SourceEntry {
            value: &self.values[index],
            index,
            provenance: self.provenance,
        }
    }
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = SourceEntry<'a, T>> + DoubleEndedIterator + Clone + '_ {
        (0..self.values.len()).map(|i| self.entry(i))
    }
    pub fn count(&self) -> Fact<usize> {
        fact(self.name(), self.values.len())
    }
    pub fn select<V>(&self, project: impl FnOnce(&T) -> V) -> Fact<V> {
        if self.values.len() != 1 {
            return try_fact(
                self.name(),
                Err::<V, _>("selection requires exactly one value"),
            );
        }
        fact(self.name(), project(&self.values[0]))
    }
    pub fn project<V>(&self, project: impl Fn(&T) -> V) -> PreparedValues<V> {
        PreparedValues {
            name: self.name.clone(),
            values: self.values.iter().map(project).collect(),
        }
    }
    pub fn numeric(&self, project: impl Fn(&T) -> f64) -> PreparedNumeric {
        PreparedNumeric {
            data: self.project(project),
        }
    }
    pub fn unique<K: Ord + Clone>(&self, key: impl Fn(&T) -> K) -> Fact<IntegrityReport<K>> {
        fact(self.name(), check_unique(self.values, key))
    }
    pub fn contiguous(
        &self,
        key: impl Fn(&T) -> u64,
        start: u64,
        end: u64,
    ) -> Fact<IntegrityReport<u64>> {
        fact(self.name(), check_contiguous(self.values, key, start, end))
    }
    pub fn matching_count(&self, predicate: impl Fn(&T) -> bool) -> Fact<usize> {
        fact(
            self.name(),
            self.values.iter().filter(|v| predicate(v)).count(),
        )
    }
    pub fn keys<K: Ord + Clone>(&self, key: impl Fn(&T) -> K) -> PreparedKeys<'a, T, K> {
        let mut keys = crate::certifications::join::key_indices(self.values, key);
        crate::certifications::join::sort_indices(&mut keys);
        PreparedKeys {
            source: self.clone(),
            keys,
        }
    }
    pub fn compare_with<R>(&self, right: &PreparedSource<'a, R>) -> ComparisonBuilder<'a, T, R> {
        ComparisonBuilder {
            left: self.clone(),
            right: right.clone(),
        }
    }
    pub fn lag_relation_by<R>(
        &self,
        right: &PreparedSource<'_, R>,
        left: impl Fn(&T) -> (i64, f64),
        rp: impl Fn(&R) -> (i64, f64),
        config: LagConfig,
    ) -> Fact<LagReport> {
        let result = lag_relation_by(self.values, right.values, left, rp, config);
        let details = result
            .as_ref()
            .map(|r| {
                format!(
                    "left={} right={} candidates={} best={} zero_lag={}",
                    r.left_count,
                    r.right_count,
                    r.candidates.len(),
                    bounded_debug(&r.best, 256),
                    bounded_debug(&r.zero_lag(), 384)
                )
            })
            .unwrap_or_default();
        try_fact(format!("{} -> {}", self.name(), right.name()), result).details(details)
    }
}
/// Reusable sorted key projection for correspondence with several sources.
pub struct PreparedKeys<'a, T, K> {
    source: PreparedSource<'a, T>,
    keys: Vec<(K, usize)>,
}
impl<'a, T, K: Ord + Clone> PreparedKeys<'a, T, K> {
    pub fn indices(&self) -> &[(K, usize)] {
        &self.keys
    }
    pub fn compare_with<R>(
        &self,
        right: &PreparedKeys<'a, R, K>,
    ) -> PreparedComparison<'a, T, R, K> {
        let joined = crate::certifications::join::merge(
            self.source.values,
            right.source.values,
            &self.keys,
            &right.keys,
        );
        PreparedComparison {
            left: self.source.clone(),
            right: right.source.clone(),
            joined,
        }
    }
}
/// Owned projections permit expensive field extraction once and cheap repeated analyses.
pub struct PreparedValues<T> {
    name: Arc<str>,
    values: Vec<T>,
}
impl<T> PreparedValues<T> {
    pub fn source(&self) -> PreparedSource<'_, T> {
        source_slice(self.name.clone(), &self.values)
    }
    pub fn values(&self) -> &[T] {
        &self.values
    }
}
/// Cached numeric projection; analysis scratch is allocated only when required by a metric.
pub struct PreparedNumeric {
    data: PreparedValues<f64>,
}
impl PreparedNumeric {
    pub fn values(&self) -> &[f64] {
        self.data.values()
    }
    pub fn series(&self) -> NumericSeries<impl Iterator<Item = f64> + Clone + '_> {
        NumericSeries::new(self.values().iter().copied())
    }
    pub fn summary(&self) -> Fact<Summary> {
        try_fact(&*self.data.name, self.series().summary())
    }
    pub fn robust_summary(&self) -> Fact<RobustSummary> {
        try_fact(&*self.data.name, self.series().robust_summary())
    }
    pub fn quantile(&self, q: f64) -> Fact<f64> {
        try_fact(&*self.data.name, self.series().quantile(q))
    }
    pub fn check(&self, rule: NumericRule) -> Fact<IntegrityReport> {
        try_fact(&*self.data.name, self.series().check(rule))
    }
    pub fn outliers(&self, config: OutlierConfig) -> Fact<OutlierReport> {
        try_fact(&*self.data.name, self.series().outliers(config))
    }
    pub fn jumps(&self, config: JumpConfig) -> Fact<JumpReport> {
        try_fact(&*self.data.name, self.series().jumps(config))
    }
}
/// Two sources awaiting explicit domain identity; order is never correspondence.
pub struct ComparisonBuilder<'a, L, R> {
    left: PreparedSource<'a, L>,
    right: PreparedSource<'a, R>,
}
impl<'a, L, R> ComparisonBuilder<'a, L, R> {
    pub fn on<K: Ord + Clone>(
        self,
        left: impl Fn(&L) -> K,
        right: impl Fn(&R) -> K,
    ) -> PreparedComparison<'a, L, R, K> {
        let joined = join_by(self.left.values, self.right.values, left, right);
        PreparedComparison {
            left: self.left,
            right: self.right,
            joined,
        }
    }
    pub fn on_u64(
        self,
        left: impl Fn(&L) -> u64,
        right: impl Fn(&R) -> u64,
    ) -> PreparedComparison<'a, L, R, u64> {
        let joined = join_u64_by(self.left.values, self.right.values, left, right);
        PreparedComparison {
            left: self.left,
            right: self.right,
            joined,
        }
    }
    pub fn on_sorted<K: Ord + Clone>(
        self,
        left: impl Fn(&L) -> K,
        right: impl Fn(&R) -> K,
    ) -> Result<PreparedComparison<'a, L, R, K>, JoinError> {
        let joined = join_sorted_by(self.left.values, self.right.values, left, right)?;
        Ok(PreparedComparison {
            left: self.left,
            right: self.right,
            joined,
        })
    }
}
/// One sorted/indexed correspondence reused by all field comparisons and provenance lookups.
pub struct PreparedComparison<'a, L, R, K> {
    left: PreparedSource<'a, L>,
    right: PreparedSource<'a, R>,
    joined: Join<'a, L, R, K>,
}
/// Bounded custom predicate evidence; full comparison remains on the prepared comparison.
#[derive(Debug)]
pub struct PairCheck<K> {
    pub matched: usize,
    pub failures: usize,
    pub examples: Vec<K>,
}
impl<L, R, K: Clone + fmt::Debug> PreparedComparison<'_, L, R, K> {
    pub fn name(&self) -> String {
        format!("{} -> {}", self.left.name(), self.right.name())
    }
    pub fn comparison(&self) -> &Comparison<K> {
        self.joined.comparison()
    }
    pub fn pairs(&self) -> impl Iterator<Item = (&K, SourceEntry<'_, L>, SourceEntry<'_, R>)> {
        self.comparison().matched.iter().map(|p| {
            (
                &p.key,
                self.left.entry(p.left_index),
                self.right.entry(p.right_index),
            )
        })
    }
    pub fn comparison_fact(&self) -> Fact<&Comparison<K>> {
        let a = self.comparison();
        fact(self.name(),a).details(format!("left={} right={} matched={} missing left={} missing right={} duplicate left={} duplicate right={} examples: missing_left={} missing_right={} duplicates_left={} duplicates_right={}", a.left_count,a.right_count,a.matched.len(),a.missing_left.len(),a.missing_right.len(),a.duplicate_left.len(),a.duplicate_right.len(),bounded_debug(&a.missing_left,256),bounded_debug(&a.missing_right,256),bounded_debug(&a.duplicate_left,256),bounded_debug(&a.duplicate_right,256)))
    }
    pub fn check_pairs(
        &self,
        predicate: impl Fn(&L, &R) -> bool,
        top: usize,
    ) -> Fact<PairCheck<K>> {
        if !self.comparison().is_complete() {
            return try_fact(
                self.name(),
                Err::<PairCheck<K>, _>("incomplete comparison; inspect comparison_fact"),
            );
        }
        let mut report = PairCheck {
            matched: self.comparison().matched.len(),
            failures: 0,
            examples: Vec::new(),
        };
        let mut coordinates = String::new();
        for (key, l, r) in self.pairs() {
            if !predicate(l.value, r.value) {
                report.failures += 1;
                if report.examples.len() < top.min(32) {
                    report.examples.push(key.clone());
                    use fmt::Write;
                    let _ = write!(
                        coordinates,
                        " key={} left index={} right index={}",
                        bounded_debug(key, 80),
                        l.index,
                        r.index
                    );
                    if let Some(p) = l.provenance {
                        let mut out = crate::certifications::certification::LimitedWriter::new(256);
                        let _ = p.describe(l.index, &mut out);
                        coordinates.push_str(&out.finish());
                    }
                    if let Some(p) = r.provenance {
                        let mut out = crate::certifications::certification::LimitedWriter::new(256);
                        let _ = p.describe(r.index, &mut out);
                        coordinates.push_str(&out.finish());
                    }
                }
            }
        }
        let details = if report.failures == 0 {
            String::new()
        } else {
            format!(
                "matched={} mismatches={} examples:{coordinates}",
                report.matched, report.failures
            )
        };
        fact(self.name(), report).details(details)
    }
    pub fn equal<V: PartialEq>(
        &self,
        left: impl Fn(&L) -> V,
        right: impl Fn(&R) -> V,
        top: usize,
    ) -> Fact<PairCheck<K>> {
        self.check_pairs(|l, r| left(l) == right(r), top)
    }
    pub fn numeric(
        &self,
        left: impl Fn(&L) -> f64,
        right: impl Fn(&R) -> f64,
    ) -> PreparedNumericComparison<'_, K> {
        let values = self
            .joined
            .pairs()
            .map(|(_, l, r)| (left(l), right(r)))
            .collect();
        PreparedNumericComparison {
            name: self.name(),
            comparison: self.comparison(),
            values,
            linear_cache: std::sync::OnceLock::new(),
            rank_cache: std::sync::OnceLock::new(),
            difference_cache: std::sync::OnceLock::new(),
            fit_cache: std::sync::OnceLock::new(),
        }
    }
    pub fn lag(
        &self,
        left: impl Fn(&L) -> (i64, f64),
        right: impl Fn(&R) -> (i64, f64),
        config: LagConfig,
    ) -> Fact<LagReport> {
        self.left.lag_relation_by(&self.right, left, right, config)
    }
}
#[cfg(test)]
#[path = "source_certification.rs"]
mod certification;

/// Numeric field pair cached in aligned-key order; no new join or field projection per metric.
/// Compact analysis results (including errors) are initialized once; sorting scratch is not retained.
pub struct PreparedNumericComparison<'a, K> {
    name: String,
    comparison: &'a Comparison<K>,
    values: Vec<(f64, f64)>,
    linear_cache: std::sync::OnceLock<Result<LinearMetrics, NumericError>>,
    rank_cache: std::sync::OnceLock<Result<Metric, NumericError>>,
    difference_cache: std::sync::OnceLock<Result<DifferenceSummary, NumericError>>,
    fit_cache: std::sync::OnceLock<Result<LinearFitMetrics, NumericError>>,
}
impl<K: Clone + fmt::Debug> PreparedNumericComparison<'_, K> {
    pub fn values(&self) -> &[(f64, f64)] {
        &self.values
    }
    fn analyze<T>(&self, op: impl FnOnce() -> Result<T, NumericError>) -> Fact<T> {
        if !self.comparison.is_complete() {
            return try_fact(
                &self.name,
                Err::<T, _>("incomplete comparison; inspect comparison_fact"),
            );
        }
        try_fact(&self.name, op())
    }
    fn cached_linear(&self) -> Result<LinearMetrics, NumericError> {
        self.linear_cache
            .get_or_init(|| linear_metrics(self.values.iter().copied()))
            .clone()
    }
    fn cached_rank(&self) -> Result<Metric, NumericError> {
        self.rank_cache
            .get_or_init(|| spearman(self.values.iter().copied()))
            .clone()
    }
    fn cached_difference(&self) -> Result<DifferenceSummary, NumericError> {
        self.difference_cache
            .get_or_init(|| difference_summary(self.values.iter().copied()))
            .clone()
    }
    fn cached_fit(&self) -> Result<LinearFitMetrics, NumericError> {
        self.fit_cache
            .get_or_init(|| {
                crate::certifications::numerical::fit_with_linear(
                    self.cached_linear()?,
                    self.values.iter().copied(),
                )
            })
            .clone()
    }
    pub fn correlation(&self) -> Fact<CorrelationMetrics> {
        self.analyze(|| {
            Ok(CorrelationMetrics {
                pearson: self.cached_linear()?.pearson,
                spearman: self.cached_rank()?,
            })
        })
    }
    pub fn linear_fit(&self) -> Fact<LinearFitMetrics> {
        self.analyze(|| self.cached_fit())
    }
    pub fn relation(&self) -> Fact<RelationMetrics> {
        self.analyze(|| {
            let fit = self.cached_fit()?;
            Ok(RelationMetrics {
                linear: fit.linear,
                residual: fit.residual,
                spearman: self.cached_rank()?,
                difference: self.cached_difference()?,
            })
        })
    }
    pub fn difference(&self) -> Fact<DifferenceSummary> {
        self.analyze(|| self.cached_difference())
    }
    pub fn linear(&self) -> Fact<LinearMetrics> {
        self.analyze(|| self.cached_linear())
    }
    pub fn pearson(&self) -> Fact<Metric> {
        self.analyze(|| Ok(self.cached_linear()?.pearson))
    }
    pub fn spearman(&self) -> Fact<Metric> {
        self.analyze(|| self.cached_rank())
    }
    pub fn largest(&self, top: usize) -> Fact<Vec<PairEvidence<K>>> {
        self.analyze(|| {
            crate::certifications::relation::largest_pairs(
                self.comparison
                    .matched
                    .iter()
                    .zip(&self.values)
                    .map(|(p, (a, b))| (&p.key, *a, *b)),
                top,
            )
        })
    }
}
