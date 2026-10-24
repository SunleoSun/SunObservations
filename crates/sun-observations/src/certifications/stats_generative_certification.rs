use super::*;
use crate::certifications::test_support::{
    assert_pairwise_certified, shuffle, ulps, Mismatches, Rng,
};
use crate::certifications::{certification, domain, fact, Domain, DomainInput};

#[derive(Clone, Debug, PartialEq, DomainInput)]
enum SampleSize {
    Singleton,
    SmallOdd,
    SmallEven,
    LargeOdd,
    LargeEven,
}
impl SampleSize {
    fn len(&self) -> usize {
        match self {
            Self::Singleton => 1,
            Self::SmallOdd => 7,
            Self::SmallEven => 8,
            Self::LargeOdd => 63,
            Self::LargeEven => 64,
        }
    }
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Order {
    Sorted,
    Reversed,
    Shuffled,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Shape {
    Distinct,
    Ties,
    Constant,
    Skewed,
    Cancelling,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Offset {
    Zero,
    Large,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Scale {
    Unit,
    Tiny,
    Huge,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum QuantileSite {
    Minimum,
    LowerQuartile,
    Median,
    Interpolated,
    UpperQuartile,
    Maximum,
}
impl QuantileSite {
    fn probability(&self) -> f64 {
        match self {
            Self::Minimum => 0.,
            Self::LowerQuartile => 0.25,
            Self::Median => 0.5,
            Self::Interpolated => 1. / 3.,
            Self::UpperQuartile => 0.75,
            Self::Maximum => 1.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
struct StatsScenario {
    size: SampleSize,
    order: Order,
    shape: Shape,
    offset: Offset,
    scale: Scale,
    site: QuantileSite,
}
fn scenario_domain() -> Domain<StatsScenario> {
    domain!(StatsScenario {
        size.all_enum_values(),
        order.all_enum_values(),
        shape.all_enum_values(),
        offset.all_enum_values(),
        scale.all_enum_values(),
        site.all_enum_values(),
    })
}

const LARGE_OFFSET: i64 = 1 << 46;
const CANCELLING_TERM: i64 = 1 << 52;
const TRANSLATION_STEPS: i64 = 1 << 20;
const POSITIVE_SCALE: f64 = 3.;

fn lattice(scenario: &StatsScenario, salt: u64) -> (Vec<i64>, f64) {
    let mut rng = Rng::new(salt);
    let base = match scenario.offset {
        Offset::Zero => 0,
        Offset::Large => LARGE_OFFSET,
    };
    let len = scenario.size.len() as i64;
    let mut units: Vec<i64> = (0..len)
        .map(|i| match scenario.shape {
            Shape::Distinct => base + 3 * i + rng.between(0, 2),
            Shape::Ties => base + 5 * (i / 2),
            Shape::Constant => base + 11,
            Shape::Skewed => base + (i + 1).pow(4),
            Shape::Cancelling => {
                let magnitude = CANCELLING_TERM + base + 7 * i + rng.between(0, 6);
                if i < (len + 1) / 2 {
                    magnitude
                } else {
                    -magnitude
                }
            }
        })
        .collect();
    match scenario.order {
        Order::Sorted => units.sort_unstable(),
        Order::Reversed => units.sort_unstable_by(|a, b| b.cmp(a)),
        Order::Shuffled => shuffle(&mut units, salt ^ 0x5eed),
    }
    let scale = match scenario.scale {
        Scale::Unit => 1.,
        Scale::Tiny => 2f64.powi(-100),
        Scale::Huge => 2f64.powi(100),
    };
    (units, 0.125 * scale)
}

struct Probe {
    summary: Summary,
    robust: RobustSummary,
    quantile: f64,
}
fn probe(values: &[f64], probability: f64) -> Result<Probe, NumericError> {
    let series = NumericSeries::new(values.iter().copied());
    Ok(Probe {
        summary: series.summary()?,
        robust: series.robust_summary()?,
        quantile: series.quantile(probability)?,
    })
}
struct StatsRun {
    lattice: Vec<i64>,
    unit: f64,
    probability: f64,
    shift: f64,
    finite: FiniteCount,
    base: Probe,
    permuted: Probe,
    translated: Probe,
    scaled: Probe,
    reflected: Probe,
}
impl StatsRun {
    fn max_abs(&self) -> f64 {
        self.lattice.iter().map(|m| m.unsigned_abs()).max().unwrap() as f64 * self.unit
    }
    fn len(&self) -> usize {
        self.lattice.len()
    }
}
fn run(scenario: &StatsScenario, salt: u64) -> Result<StatsRun, NumericError> {
    let (lattice, unit) = lattice(scenario, salt);
    let values: Vec<f64> = lattice.iter().map(|m| *m as f64 * unit).collect();
    let probability = scenario.site.probability();
    let mut permuted = values.clone();
    shuffle(&mut permuted, salt ^ 0xa5);
    permuted.rotate_left(values.len() / 3 + 1);
    let shift = TRANSLATION_STEPS as f64 * unit;
    let translated: Vec<f64> = values.iter().map(|v| v + shift).collect();
    let scaled: Vec<f64> = values.iter().map(|v| v * POSITIVE_SCALE).collect();
    let reflected: Vec<f64> = values.iter().map(|v| -v).collect();
    Ok(StatsRun {
        finite: NumericSeries::new(values.iter().copied()).finite_count(),
        base: probe(&values, probability)?,
        permuted: probe(&permuted, probability)?,
        translated: probe(&translated, probability)?,
        scaled: probe(&scaled, probability)?,
        reflected: probe(&reflected, 1. - probability)?,
        lattice,
        unit,
        probability,
        shift,
    })
}

struct Reference {
    count: usize,
    unit: f64,
    sum: i128,
    squares: i128,
    sorted: Vec<f64>,
}
impl Reference {
    fn new(run: &StatsRun) -> Self {
        let mut sorted: Vec<f64> = run.lattice.iter().map(|m| *m as f64).collect();
        sorted.sort_by(f64::total_cmp);
        Self {
            count: run.len(),
            unit: run.unit,
            sum: run.lattice.iter().map(|m| i128::from(*m)).sum(),
            squares: run.lattice.iter().map(|m| i128::from(*m).pow(2)).sum(),
            sorted,
        }
    }
    fn sum(&self) -> f64 {
        self.sum as f64 * self.unit
    }
    fn mean(&self) -> f64 {
        self.sum as f64 / self.count as f64 * self.unit
    }
    fn dispersion(&self) -> i128 {
        self.count as i128 * self.squares - self.sum * self.sum
    }
    fn population_variance(&self) -> f64 {
        self.dispersion() as f64 / (self.count * self.count) as f64 * (self.unit * self.unit)
    }
    fn sample_variance(&self) -> Option<f64> {
        (self.count > 1).then(|| {
            self.dispersion() as f64 / (self.count * (self.count - 1)) as f64
                * (self.unit * self.unit)
        })
    }
    fn interpolate(sorted: &[f64], probability: f64) -> f64 {
        let position = (sorted.len() - 1) as f64 * probability;
        let low = position.floor() as usize;
        let high = (low + 1).min(sorted.len() - 1);
        sorted[low] + (position - low as f64) * (sorted[high] - sorted[low])
    }
    fn quantile(&self, probability: f64) -> f64 {
        Self::interpolate(&self.sorted, probability) * self.unit
    }
    fn median_absolute_deviation(&self) -> f64 {
        let median = Self::interpolate(&self.sorted, 0.5);
        let mut deviations: Vec<f64> = self.sorted.iter().map(|v| (v - median).abs()).collect();
        deviations.sort_by(f64::total_cmp);
        Self::interpolate(&deviations, 0.5) * self.unit
    }
}

fn variance_tolerance(expected: f64) -> f64 {
    1e-9 * expected
}

fn counts_are_exact(run: &StatsRun) -> Vec<String> {
    let mut m = Mismatches::default();
    let count = run.len();
    m.equal("summary count", &run.base.summary.count, &count);
    m.equal(
        "summary finite count",
        &run.base.summary.finite_count,
        &count,
    );
    m.equal("robust count", &run.base.robust.count, &count);
    m.equal(
        "finite count",
        &run.finite,
        &FiniteCount {
            count,
            finite_count: count,
        },
    );
    m.finish()
}
fn extrema_sum_and_mean_match_exact_lattice_reference(run: &StatsRun) -> Vec<String> {
    let reference = Reference::new(run);
    let summary = &run.base.summary;
    let mut m = Mismatches::default();
    m.equal(
        "min",
        &summary.min,
        &(*run.lattice.iter().min().unwrap() as f64 * run.unit),
    );
    m.equal(
        "max",
        &summary.max,
        &(*run.lattice.iter().max().unwrap() as f64 * run.unit),
    );
    let sum_tolerance = 0.25 * run.unit + ulps(reference.sum());
    m.within("sum", summary.sum, reference.sum(), sum_tolerance);
    m.within(
        "mean",
        summary.mean,
        reference.mean(),
        sum_tolerance / run.len() as f64 + ulps(reference.mean()),
    );
    m.finish()
}
fn variances_match_exact_integer_reference(run: &StatsRun) -> Vec<String> {
    let reference = Reference::new(run);
    let summary = &run.base.summary;
    let mut m = Mismatches::default();
    let variance = reference.population_variance();
    m.within(
        "population variance",
        summary.variance,
        variance,
        variance_tolerance(variance),
    );
    m.within(
        "standard deviation",
        summary.standard_deviation,
        variance.sqrt(),
        variance_tolerance(variance.sqrt()),
    );
    match (summary.sample_variance, reference.sample_variance()) {
        (Some(actual), Some(expected)) => m.within(
            "sample variance",
            actual,
            expected,
            variance_tolerance(expected),
        ),
        (actual, expected) => m.equal("sample variance presence", &actual, &expected),
    }
    m.finish()
}
fn robust_statistics_match_sorted_reference(run: &StatsRun) -> Vec<String> {
    let reference = Reference::new(run);
    let robust = &run.base.robust;
    let tolerance = 2. * ulps(run.max_abs());
    let (q1, q3) = (reference.quantile(0.25), reference.quantile(0.75));
    let mut m = Mismatches::default();
    m.within("median", robust.median, reference.quantile(0.5), tolerance);
    m.within("q1", robust.q1, q1, tolerance);
    m.within("q3", robust.q3, q3, tolerance);
    m.within("iqr", robust.iqr, q3 - q1, 2. * tolerance);
    m.within(
        "mad",
        robust.mad,
        reference.median_absolute_deviation(),
        2. * tolerance,
    );
    m.finish()
}
fn requested_quantile_matches_interpolation_reference(run: &StatsRun) -> Vec<String> {
    let mut m = Mismatches::default();
    m.within(
        "quantile",
        run.base.quantile,
        Reference::new(run).quantile(run.probability),
        2. * ulps(run.max_abs()),
    );
    m.finish()
}
fn permutation_preserves_order_free_statistics(run: &StatsRun) -> Vec<String> {
    let (a, b) = (&run.base, &run.permuted);
    let mut m = Mismatches::default();
    m.equal("robust summary", &a.robust, &b.robust);
    m.equal("quantile", &a.quantile, &b.quantile);
    m.equal("count", &a.summary.count, &b.summary.count);
    m.equal("min", &a.summary.min, &b.summary.min);
    m.equal("max", &a.summary.max, &b.summary.max);
    let sum_tolerance = 2. * (0.25 * run.unit + ulps(a.summary.sum));
    m.within("sum", b.summary.sum, a.summary.sum, sum_tolerance);
    m.within(
        "mean",
        b.summary.mean,
        a.summary.mean,
        sum_tolerance / run.len() as f64 + ulps(a.summary.mean),
    );
    m.within(
        "variance",
        b.summary.variance,
        a.summary.variance,
        variance_tolerance(a.summary.variance),
    );
    m.finish()
}
fn translation_shifts_location_and_preserves_dispersion(run: &StatsRun) -> Vec<String> {
    let (a, b) = (&run.base, &run.translated);
    let shift = run.shift;
    let rounding = 4. * ulps(run.max_abs() + shift.abs());
    let mean_tolerance = 0.5 * run.unit + rounding;
    let mut m = Mismatches::default();
    m.within(
        "mean",
        b.summary.mean,
        a.summary.mean + shift,
        mean_tolerance,
    );
    m.within("min", b.summary.min, a.summary.min + shift, rounding);
    m.within("max", b.summary.max, a.summary.max + shift, rounding);
    m.within("median", b.robust.median, a.robust.median + shift, rounding);
    m.within("q1", b.robust.q1, a.robust.q1 + shift, rounding);
    m.within("q3", b.robust.q3, a.robust.q3 + shift, rounding);
    m.within("quantile", b.quantile, a.quantile + shift, rounding);
    m.within("iqr", b.robust.iqr, a.robust.iqr, 2. * rounding);
    m.within("mad", b.robust.mad, a.robust.mad, 2. * rounding);
    m.within(
        "variance",
        b.summary.variance,
        a.summary.variance,
        variance_tolerance(a.summary.variance),
    );
    m.finish()
}
fn positive_scaling_scales_location_dispersion_and_squared_spread(run: &StatsRun) -> Vec<String> {
    let (a, b) = (&run.base, &run.scaled);
    let k = POSITIVE_SCALE;
    let rounding = 4. * ulps(k * run.max_abs());
    let element_rounding = run.len() as f64 * 4. * f64::EPSILON * (k * run.max_abs());
    let sum_tolerance = k * 0.25 * run.unit + element_rounding + ulps(k * a.summary.sum);
    let mut m = Mismatches::default();
    m.equal("min", &b.summary.min, &(k * a.summary.min));
    m.equal("max", &b.summary.max, &(k * a.summary.max));
    m.within("sum", b.summary.sum, k * a.summary.sum, sum_tolerance);
    m.within(
        "mean",
        b.summary.mean,
        k * a.summary.mean,
        sum_tolerance / run.len() as f64 + ulps(k * a.summary.mean),
    );
    m.within(
        "variance",
        b.summary.variance,
        k * k * a.summary.variance,
        variance_tolerance(k * k * a.summary.variance),
    );
    m.within(
        "standard deviation",
        b.summary.standard_deviation,
        k * a.summary.standard_deviation,
        variance_tolerance(k * a.summary.standard_deviation),
    );
    for (label, scaled, base) in [
        ("median", b.robust.median, a.robust.median),
        ("q1", b.robust.q1, a.robust.q1),
        ("q3", b.robust.q3, a.robust.q3),
        ("iqr", b.robust.iqr, a.robust.iqr),
        ("mad", b.robust.mad, a.robust.mad),
        ("quantile", b.quantile, a.quantile),
    ] {
        m.within(label, scaled, k * base, 2. * rounding);
    }
    m.finish()
}
fn reflection_mirrors_quantiles_and_preserves_spread(run: &StatsRun) -> Vec<String> {
    let (a, b) = (&run.base, &run.reflected);
    let rounding = 4. * ulps(run.max_abs());
    let mut m = Mismatches::default();
    m.equal("min", &b.summary.min, &-a.summary.max);
    m.equal("max", &b.summary.max, &-a.summary.min);
    m.within("median", b.robust.median, -a.robust.median, rounding);
    m.within("q1", b.robust.q1, -a.robust.q3, rounding);
    m.within("q3", b.robust.q3, -a.robust.q1, rounding);
    m.within("iqr", b.robust.iqr, a.robust.iqr, 2. * rounding);
    m.within("mad", b.robust.mad, a.robust.mad, 2. * rounding);
    m.within("mirrored quantile", b.quantile, -a.quantile, rounding);
    m.within(
        "variance",
        b.summary.variance,
        a.summary.variance,
        variance_tolerance(a.summary.variance),
    );
    m.finish()
}

#[test]
fn generated_distribution_scenarios_match_exact_references_and_metamorphic_invariants() {
    let mut salt = 0;
    let mut c = certification("statistics over generated distribution scenarios").executor(
        move |scenario: &StatsScenario| {
            salt += 1;
            run(scenario, salt)
        },
    );
    c.domain(scenario_domain());
    c.seed(184729361).cases_budget(256);
    let case = c.case();
    c.check(
        fact!(counts_are_exact(case.result()))
            .expect(Vec::<String>::new())
            .description("count and finite count"),
    );
    c.check(
        fact!(extrema_sum_and_mean_match_exact_lattice_reference(
            case.result()
        ))
        .expect(Vec::<String>::new())
        .description("min, max, compensated sum and mean"),
    );
    c.check(
        fact!(variances_match_exact_integer_reference(case.result()))
            .expect(Vec::<String>::new())
            .description("population variance, sample variance, standard deviation"),
    );
    c.check(
        fact!(robust_statistics_match_sorted_reference(case.result()))
            .expect(Vec::<String>::new())
            .description("median, quartiles, IQR, MAD"),
    );
    c.check(
        fact!(requested_quantile_matches_interpolation_reference(
            case.result()
        ))
        .expect(Vec::<String>::new())
        .description("type-7 quantile interpolation"),
    );
    c.check(
        fact!(permutation_preserves_order_free_statistics(case.result()))
            .expect(Vec::<String>::new())
            .description("permutation invariance"),
    );
    c.check(
        fact!(translation_shifts_location_and_preserves_dispersion(
            case.result()
        ))
        .expect(Vec::<String>::new())
        .description("translation equivariance"),
    );
    c.check(
        fact!(positive_scaling_scales_location_dispersion_and_squared_spread(case.result()))
            .expect(Vec::<String>::new())
            .description("positive scaling"),
    );
    c.check(
        fact!(reflection_mirrors_quantiles_and_preserves_spread(
            case.result()
        ))
        .expect(Vec::<String>::new())
        .description("reflection"),
    );
    let report = c.report();
    assert_pairwise_certified(&report);
    assert_eq!(report.cases_executed(), 256);
    assert_eq!(report.checks.len(), 9);
}

#[test]
fn naive_summation_is_detected_by_cancelling_scenarios() {
    let mut salt = 0;
    let mut c =
        certification("naive summation control").executor(move |scenario: &StatsScenario| {
            salt += 1;
            run(scenario, salt).map(|mut run| {
                let naive: f64 = run.lattice.iter().map(|m| *m as f64 * run.unit).sum();
                run.base.summary.sum = naive;
                run
            })
        });
    c.domain(scenario_domain());
    c.seed(184729361).cases_budget(256);
    c.failure_budget(64);
    let case = c.case();
    c.check(
        fact!(extrema_sum_and_mean_match_exact_lattice_reference(
            case.result()
        ))
        .expect(Vec::<String>::new())
        .description("min, max, compensated sum and mean"),
    );
    let report = c.report();
    assert!(!report.is_pass());
    assert!(report
        .failures
        .iter()
        .any(|f| f.diagnostic.contains("sum:")));
}
