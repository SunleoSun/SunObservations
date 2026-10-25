use super::*;
use crate::certifications::test_support::{
    reference_linear, reference_rank, shuffle, ulps, Mismatches, Rng,
};
use crate::certifications::{
    certification, collection, domain, fact, linear_fit, CheckClass, CorrelationMetrics, Domain,
    DomainInput, LinearFitMetrics,
};

#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Association {
    PositiveAffine,
    NegativeAffine,
    MonotonicNonlinear,
    Noisy,
    ConstantLeft,
    ConstantRight,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Ties {
    Absent,
    Left,
    Both,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Order {
    Sorted,
    Reversed,
    Shuffled,
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
enum PairCount {
    Single,
    Pair,
    Few,
    Many,
}
impl PairCount {
    fn len(&self) -> usize {
        match self {
            Self::Single => 1,
            Self::Pair => 2,
            Self::Few => 9,
            Self::Many => 30,
        }
    }
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
struct RelationScenario {
    association: Association,
    ties: Ties,
    order: Order,
    offset: Offset,
    scale: Scale,
    count: PairCount,
}
fn scenario_domain() -> Domain<RelationScenario> {
    domain!(RelationScenario {
        association.all_enum_values(),
        ties.all_enum_values(),
        order.all_enum_values(),
        offset.all_enum_values(),
        scale.all_enum_values(),
        count.all_enum_values(),
    })
}

const LARGE_OFFSET: i64 = 1 << 40;
const UP_SLOPE: i64 = 3;
const UP_INTERCEPT: i64 = 5;
const DOWN_SLOPE: i64 = -2;
const DOWN_INTERCEPT: i64 = 7;
const RIGHT_UNIT_RATIO: f64 = 2.;
const SCALE_X: f64 = 3.;
const SCALE_Y: f64 = 5.;
const TRANSLATION_STEPS: i64 = 1 << 20;

#[derive(Debug, PartialEq)]
struct Generated {
    pairs: Vec<(f64, f64)>,
    order: Vec<usize>,
    left_unit: f64,
    right_unit: f64,
}
fn generate(scenario: &RelationScenario, seed: u64) -> Generated {
    let mut rng = Rng::new(seed);
    let n = scenario.count.len();
    let offset = match scenario.offset {
        Offset::Zero => 0,
        Offset::Large => LARGE_OFFSET,
    };
    let tied = scenario.ties != Ties::Absent;
    let steps: Vec<i64> = (0..n as i64)
        .map(|i| {
            if tied {
                3 * (i / 2)
            } else {
                3 * i + rng.between(0, 2)
            }
        })
        .collect();
    let x: Vec<i64> = match scenario.association {
        Association::ConstantLeft => vec![offset + 17; n],
        _ => steps.iter().map(|d| offset + d).collect(),
    };
    let noise = |rng: &mut Rng| {
        offset
            + if scenario.ties == Ties::Both {
                rng.between(0, 3)
            } else {
                rng.between(-40, 40)
            }
    };
    let y: Vec<i64> = match scenario.association {
        Association::PositiveAffine => x.iter().map(|x| UP_SLOPE * x + UP_INTERCEPT).collect(),
        Association::NegativeAffine => x.iter().map(|x| DOWN_SLOPE * x + DOWN_INTERCEPT).collect(),
        Association::MonotonicNonlinear => steps.iter().map(|d| offset + d.pow(3)).collect(),
        Association::Noisy | Association::ConstantLeft => (0..n).map(|_| noise(&mut rng)).collect(),
        Association::ConstantRight => vec![offset + 23; n],
    };
    let scale = match scenario.scale {
        Scale::Unit => 1.,
        Scale::Tiny => 2f64.powi(-100),
        Scale::Huge => 2f64.powi(100),
    };
    let (left_unit, right_unit) = (0.125 * scale, 0.125 * scale * RIGHT_UNIT_RATIO);
    let mut order: Vec<usize> = (0..n).collect();
    match scenario.order {
        Order::Sorted => {}
        Order::Reversed => order.reverse(),
        Order::Shuffled => shuffle(&mut order, seed ^ 0x5eed),
    }
    Generated {
        pairs: x
            .iter()
            .zip(&y)
            .map(|(x, y)| (*x as f64 * left_unit, *y as f64 * right_unit))
            .collect(),
        order,
        left_unit,
        right_unit,
    }
}

struct Probe {
    linear: LinearMetrics,
    spearman: Metric,
}
fn analyse(pairs: &[(f64, f64)]) -> Result<Probe, NumericError> {
    Ok(Probe {
        linear: linear_metrics(pairs.iter().copied())?,
        spearman: spearman(pairs.iter().copied())?,
    })
}
struct Row {
    key: u32,
    x: f64,
    y: f64,
}
struct Facade {
    relation: Result<RelationMetrics, String>,
    correlation: Result<CorrelationMetrics, String>,
    fit: Result<LinearFitMetrics, String>,
    difference: Result<DifferenceSummary, String>,
    left_source: Option<String>,
    right_source: Option<String>,
    matched_keys: Vec<u32>,
    expectation_class: CheckClass,
    expectation_violations: usize,
}
fn facade(pairs: &[(f64, f64)], order: &[usize]) -> Facade {
    let row = |i: usize| Row {
        key: i as u32,
        x: pairs[i].0,
        y: pairs[i].1,
    };
    let left: Vec<Row> = order.iter().map(|i| row(*i)).collect();
    let right: Vec<Row> = order.iter().rev().map(|i| row(*i)).collect();
    let left = collection("left", &left);
    let right = collection("right", &right);
    let paired = left.compare_with(&right).on(|r| r.key, |r| r.key);
    let numeric = paired.numeric(|r| r.x, |r| r.y);
    let constraint = numeric
        .relation()
        .expect(RelationExpectation::spearman_at_least(0.5).unwrap());
    Facade {
        relation: numeric.relation().observed.map_err(|e| e.message),
        correlation: numeric.correlation().observed.map_err(|e| e.message),
        fit: numeric.linear_fit().observed.map_err(|e| e.message),
        difference: numeric.difference().observed.map_err(|e| e.message),
        left_source: paired.comparison().left_source().map(str::to_owned),
        right_source: paired.comparison().right_source().map(str::to_owned),
        matched_keys: paired
            .comparison()
            .matched()
            .iter()
            .map(|m| m.key)
            .collect(),
        expectation_class: constraint.class(),
        expectation_violations: constraint.violations().len(),
    }
}

struct RelationRun {
    generated: Vec<(f64, f64)>,
    left_unit: f64,
    right_unit: f64,
    inserted: Probe,
    ordered: Probe,
    relation: RelationMetrics,
    swapped: Probe,
    negated: Probe,
    scaled: Probe,
    translated: Probe,
    translation: f64,
    facade: Facade,
}
fn run(scenario: &RelationScenario, seed: u64) -> Result<RelationRun, NumericError> {
    let generated = generate(scenario, seed);
    let pairs = &generated.pairs;
    let inserted: Vec<(f64, f64)> = generated.order.iter().map(|i| pairs[*i]).collect();
    let map = |f: &dyn Fn((f64, f64)) -> (f64, f64)| -> Vec<(f64, f64)> {
        pairs.iter().map(|p| f(*p)).collect()
    };
    let translation = TRANSLATION_STEPS as f64 * generated.right_unit;
    Ok(RelationRun {
        inserted: analyse(&inserted)?,
        ordered: analyse(pairs)?,
        relation: numeric_relation(pairs.iter().copied())?,
        swapped: analyse(&map(&|(x, y)| (y, x)))?,
        negated: analyse(&map(&|(x, y)| (x, -y)))?,
        scaled: analyse(&map(&|(x, y)| (SCALE_X * x, SCALE_Y * y)))?,
        translated: analyse(&map(&|(x, y)| (x, y + translation)))?,
        translation,
        facade: facade(pairs, &generated.order),
        left_unit: generated.left_unit,
        right_unit: generated.right_unit,
        generated: generated.pairs,
    })
}

fn undefined_reason(pairs: &[(f64, f64)]) -> Option<UndefinedReason> {
    if pairs.len() < 2 {
        Some(UndefinedReason::InsufficientPairs)
    } else if pairs.iter().all(|p| p.0 == pairs[0].0) {
        Some(UndefinedReason::ConstantLeft)
    } else if pairs.iter().all(|p| p.1 == pairs[0].1) {
        Some(UndefinedReason::ConstantRight)
    } else {
        None
    }
}
fn extent(values: impl Iterator<Item = f64> + Clone) -> (f64, f64) {
    let max = values.clone().fold(f64::MIN, f64::max);
    let min = values.clone().fold(f64::MAX, f64::min);
    (max - min, values.fold(0., |a, v| a.max(v.abs())))
}
struct Magnitudes {
    x_range: f64,
    y_range: f64,
    x_abs: f64,
    y_abs: f64,
}
fn magnitudes(pairs: &[(f64, f64)]) -> Magnitudes {
    let (x_range, x_abs) = extent(pairs.iter().map(|p| p.0));
    let (y_range, y_abs) = extent(pairs.iter().map(|p| p.1));
    Magnitudes {
        x_range,
        y_range,
        x_abs,
        y_abs,
    }
}
enum Expect {
    Defined(f64, f64),
    Undefined(UndefinedReason),
}
fn metric(m: &mut Mismatches, label: &str, actual: Metric, expected: Expect) {
    match expected {
        Expect::Undefined(reason) => m.equal(label, &actual, &Metric::Undefined(reason)),
        Expect::Defined(value, tolerance) => match actual {
            Metric::Defined(actual) => m.within(label, actual, value, tolerance),
            undefined => m.equal(label, &undefined, &Metric::Defined(value)),
        },
    }
}
fn rank_correlation(pairs: &[(f64, f64)]) -> f64 {
    let rx = reference_rank(&pairs.iter().map(|p| p.0).collect::<Vec<_>>());
    let ry = reference_rank(&pairs.iter().map(|p| p.1).collect::<Vec<_>>());
    reference_linear(&rx.into_iter().zip(ry).collect::<Vec<_>>()).0
}
fn intercept_tolerance(mag: &Magnitudes, slope: f64, expected: f64) -> f64 {
    1e-9 * (mag.y_range + expected.abs()) + 2. * ulps(mag.y_abs + slope.abs() * mag.x_abs)
}

fn linear_metrics_match_two_pass_reference(run: &RelationRun) -> Vec<String> {
    let pairs = &run.generated;
    let linear = &run.relation.linear;
    let mag = magnitudes(pairs);
    let n = pairs.len();
    let mut m = Mismatches::default();
    m.equal("pair count", &linear.count, &n);
    let reason = undefined_reason(pairs);
    let covariance_tolerance = 1e-9 * mag.x_range * mag.y_range;
    let reference =
        (n >= 2 && reason != Some(UndefinedReason::ConstantLeft)).then(|| reference_linear(pairs));
    let covariance = reference.map_or(0., |r| r.1);
    metric(
        &mut m,
        "covariance",
        linear.covariance,
        Expect::Defined(covariance, covariance_tolerance),
    );
    let sample_covariance = if n < 2 {
        Expect::Undefined(UndefinedReason::InsufficientPairs)
    } else {
        Expect::Defined(
            covariance * n as f64 / (n - 1) as f64,
            covariance_tolerance * 2.,
        )
    };
    metric(
        &mut m,
        "sample covariance",
        linear.sample_covariance,
        sample_covariance,
    );
    match reason {
        Some(UndefinedReason::InsufficientPairs) | Some(UndefinedReason::ConstantLeft) => {
            let reason = reason.unwrap();
            for (label, actual) in [
                ("pearson", linear.pearson),
                ("slope", linear.slope),
                ("intercept", linear.intercept),
                ("r squared", linear.r_squared),
            ] {
                metric(&mut m, label, actual, Expect::Undefined(reason));
            }
        }
        _ => {
            let (pearson, _, slope, intercept, r_squared) = reference.unwrap();
            let slope_tolerance = 1e-9 * slope.abs() + 1e-12 * mag.y_range / mag.x_range;
            metric(
                &mut m,
                "slope",
                linear.slope,
                Expect::Defined(slope, slope_tolerance),
            );
            metric(
                &mut m,
                "intercept",
                linear.intercept,
                Expect::Defined(intercept, intercept_tolerance(&mag, slope, intercept)),
            );
            if reason.is_none() {
                metric(
                    &mut m,
                    "pearson",
                    linear.pearson,
                    Expect::Defined(pearson, 1e-9),
                );
                metric(
                    &mut m,
                    "r squared",
                    linear.r_squared,
                    Expect::Defined(r_squared, 1e-9),
                );
            } else {
                let reason = Expect::Undefined(UndefinedReason::ConstantRight);
                metric(&mut m, "pearson", linear.pearson, reason);
                metric(
                    &mut m,
                    "r squared",
                    linear.r_squared,
                    Expect::Undefined(UndefinedReason::ConstantRight),
                );
            }
        }
    }
    m.finish()
}
fn spearman_matches_rank_reference_and_undefined_classification(run: &RelationRun) -> Vec<String> {
    let pairs = &run.generated;
    let expected = match undefined_reason(pairs) {
        Some(reason) => Expect::Undefined(reason),
        None => Expect::Defined(rank_correlation(pairs), 1e-9),
    };
    let mut m = Mismatches::default();
    metric(&mut m, "spearman", run.relation.spearman, expected);
    m.finish()
}
fn difference_summary_matches_sorted_reference(run: &RelationRun) -> Vec<String> {
    let differences: Vec<f64> = run.generated.iter().map(|(x, y)| y - x).collect();
    let n = differences.len() as f64;
    let max_abs = differences.iter().fold(0., |a: f64, d| a.max(d.abs()));
    let rounding = 2. * ulps(max_abs);
    let sorted = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v
    };
    let type7 = |v: &[f64], p: f64| {
        let h = (v.len() - 1) as f64 * p;
        let lo = h.floor() as usize;
        let hi = (lo + 1).min(v.len() - 1);
        v[lo] + (h - lo as f64) * (v[hi] - v[lo])
    };
    let signed = sorted(differences.clone());
    let absolute = sorted(differences.iter().map(|d| d.abs()).collect());
    let actual = &run.relation.difference;
    let mut m = Mismatches::default();
    m.equal("signed min", &actual.signed.min, &signed[0]);
    m.equal("signed max", &actual.signed.max, signed.last().unwrap());
    m.equal(
        "absolute max",
        &actual.absolute.max,
        absolute.last().unwrap(),
    );
    m.within(
        "signed mean",
        actual.signed.mean,
        differences.iter().sum::<f64>() / n,
        rounding * n,
    );
    m.within(
        "absolute mean",
        actual.absolute.mean,
        absolute.iter().sum::<f64>() / n,
        rounding * n,
    );
    m.within(
        "median signed",
        actual.median_signed,
        type7(&signed, 0.5),
        rounding,
    );
    m.within(
        "median absolute",
        actual.median_absolute,
        type7(&absolute, 0.5),
        rounding,
    );
    m.within(
        "p95 absolute",
        actual.p95_absolute,
        type7(&absolute, 0.95),
        rounding,
    );
    m.finish()
}
fn exact_generating_affine_law_is_recovered(
    run: &RelationRun,
    scenario: &RelationScenario,
) -> Vec<String> {
    let (slope, intercept, direction) = match scenario.association {
        Association::PositiveAffine => (UP_SLOPE, UP_INTERCEPT, 1.),
        Association::NegativeAffine => (DOWN_SLOPE, DOWN_INTERCEPT, -1.),
        _ => return Vec::new(),
    };
    let linear = &run.relation.linear;
    let mag = magnitudes(&run.generated);
    let expected_slope = slope as f64 * run.right_unit / run.left_unit;
    let expected_intercept = intercept as f64 * run.right_unit;
    let mut m = Mismatches::default();
    if undefined_reason(&run.generated).is_some() {
        return m.finish();
    }
    metric(
        &mut m,
        "slope",
        linear.slope,
        Expect::Defined(expected_slope, 1e-9 * expected_slope.abs()),
    );
    metric(
        &mut m,
        "intercept",
        linear.intercept,
        Expect::Defined(
            expected_intercept,
            intercept_tolerance(&mag, expected_slope, expected_intercept),
        ),
    );
    metric(
        &mut m,
        "pearson",
        linear.pearson,
        Expect::Defined(direction, 1e-9),
    );
    metric(
        &mut m,
        "r squared",
        linear.r_squared,
        Expect::Defined(1., 1e-9),
    );
    metric(
        &mut m,
        "spearman",
        run.relation.spearman,
        Expect::Defined(direction, 1e-9),
    );
    let residual = run.relation.residual.as_ref().map(|r| r.absolute.max);
    m.require(
        "exact affine fit leaves rounding-only residuals",
        residual.is_some_and(|r| {
            r <= 4. * ulps(mag.y_abs + expected_slope.abs() * mag.x_abs) + 1e-9 * mag.y_range
        }),
    );
    m.finish()
}
fn monotonic_relation_has_unit_rank_correlation(
    run: &RelationRun,
    scenario: &RelationScenario,
) -> Vec<String> {
    let mut m = Mismatches::default();
    if scenario.association == Association::MonotonicNonlinear && run.generated.len() >= 2 {
        match run.relation.spearman {
            Metric::Defined(rho) => m.within("spearman", rho, 1., 1e-12),
            other => m.equal("spearman", &other, &Metric::Defined(1.)),
        }
        if let Metric::Defined(r) = run.relation.linear.pearson {
            m.require("pearson is bounded", r.abs() <= 1.);
        }
    }
    m.finish()
}
fn pair_order_does_not_change_metrics(run: &RelationRun) -> Vec<String> {
    let (a, b) = (&run.ordered, &run.inserted);
    let mag = magnitudes(&run.generated);
    let mut m = Mismatches::default();
    m.equal("spearman", &a.spearman, &b.spearman);
    m.equal("count", &a.linear.count, &b.linear.count);
    for (label, x, y, tolerance) in [
        ("pearson", a.linear.pearson, b.linear.pearson, 1e-9),
        ("r squared", a.linear.r_squared, b.linear.r_squared, 1e-9),
        (
            "covariance",
            a.linear.covariance,
            b.linear.covariance,
            1e-9 * mag.x_range * mag.y_range,
        ),
        (
            "slope",
            a.linear.slope,
            b.linear.slope,
            1e-9 * mag.y_range / mag.x_range.max(f64::MIN_POSITIVE),
        ),
    ] {
        match (x, y) {
            (Metric::Defined(x), Metric::Defined(y)) => m.within(label, y, x, tolerance),
            (x, y) => m.equal(label, &y, &x),
        }
    }
    m.finish()
}
fn prepared_comparison_equals_direct_algorithms_with_provenance(run: &RelationRun) -> Vec<String> {
    let f = &run.facade;
    let pairs = run.generated.iter().copied();
    let flatten = |e: NumericError| e.to_string();
    let mut m = Mismatches::default();
    m.equal(
        "relation",
        &f.relation,
        &numeric_relation(pairs.clone()).map_err(flatten),
    );
    m.equal(
        "correlation",
        &f.correlation,
        &crate::certifications::correlation(pairs.clone()).map_err(flatten),
    );
    m.equal(
        "linear fit",
        &f.fit,
        &linear_fit(pairs.clone()).map_err(flatten),
    );
    m.equal(
        "difference",
        &f.difference,
        &difference_summary(pairs).map_err(flatten),
    );
    m.equal("left source", &f.left_source.as_deref(), &Some("left"));
    m.equal("right source", &f.right_source.as_deref(), &Some("right"));
    let keys: Vec<u32> = (0..run.generated.len() as u32).collect();
    m.equal("matched keys", &f.matched_keys, &keys);
    m.finish()
}
fn constant_or_insufficient_side_fails_expectations_closed(run: &RelationRun) -> Vec<String> {
    let f = &run.facade;
    let mut m = Mismatches::default();
    match (undefined_reason(&run.generated), run.relation.spearman) {
        (Some(_), _) => {
            m.equal("class", &f.expectation_class, &CheckClass::EvaluationError);
            m.equal("violations", &f.expectation_violations, &0);
        }
        (None, Metric::Defined(rho)) => {
            let passes = rho >= 0.5;
            m.equal("class", &(f.expectation_class == CheckClass::Pass), &passes);
            m.equal(
                "violations",
                &f.expectation_violations,
                &usize::from(!passes),
            );
        }
        (None, other) => m.equal("defined spearman", &other, &Metric::Defined(0.)),
    }
    m.finish()
}
fn metamorphic_transformations_preserve_mathematical_relationships(
    run: &RelationRun,
) -> Vec<String> {
    let base = &run.ordered;
    let mag = magnitudes(&run.generated);
    let mut m = Mismatches::default();
    let right_constant = run.generated.iter().all(|p| p.1 == run.generated[0].1);
    let flip = |reason| match reason {
        UndefinedReason::ConstantLeft if !right_constant => UndefinedReason::ConstantRight,
        UndefinedReason::ConstantRight => UndefinedReason::ConstantLeft,
        other => other,
    };
    let relate =
        |m: &mut Mismatches, label: &str, actual: Metric, expected: Metric, tolerance: f64| match (
            actual, expected,
        ) {
            (Metric::Defined(a), Metric::Defined(e)) => m.within(label, a, e, tolerance),
            (a, e) => m.equal(label, &a, &e),
        };
    let map = |metric: Metric, f: &dyn Fn(f64) -> f64| match metric {
        Metric::Defined(v) => Metric::Defined(f(v)),
        other => other,
    };
    let swap = |metric: Metric| match metric {
        Metric::Undefined(r) => Metric::Undefined(flip(r)),
        other => other,
    };
    relate(
        &mut m,
        "swapped pearson",
        run.swapped.linear.pearson,
        swap(base.linear.pearson),
        1e-9,
    );
    relate(
        &mut m,
        "swapped spearman",
        run.swapped.spearman,
        swap(base.spearman),
        1e-9,
    );
    relate(
        &mut m,
        "swapped covariance",
        run.swapped.linear.covariance,
        base.linear.covariance,
        1e-9 * mag.x_range * mag.y_range,
    );
    if let (Metric::Defined(forward), Metric::Defined(backward), Metric::Defined(r2)) = (
        base.linear.slope,
        run.swapped.linear.slope,
        base.linear.r_squared,
    ) {
        m.within(
            "slope product equals r squared",
            forward * backward,
            r2,
            1e-9,
        );
    }
    relate(
        &mut m,
        "negated pearson",
        run.negated.linear.pearson,
        map(base.linear.pearson, &|v| -v),
        1e-9,
    );
    relate(
        &mut m,
        "negated spearman",
        run.negated.spearman,
        map(base.spearman, &|v| -v),
        1e-9,
    );
    relate(
        &mut m,
        "negated r squared",
        run.negated.linear.r_squared,
        base.linear.r_squared,
        1e-9,
    );
    relate(
        &mut m,
        "scaled pearson",
        run.scaled.linear.pearson,
        base.linear.pearson,
        1e-9,
    );
    relate(
        &mut m,
        "scaled spearman",
        run.scaled.spearman,
        base.spearman,
        1e-9,
    );
    relate(
        &mut m,
        "scaled slope",
        run.scaled.linear.slope,
        map(base.linear.slope, &|v| v * SCALE_Y / SCALE_X),
        1e-9 * (base.linear.slope.value().unwrap_or(0.).abs() * SCALE_Y / SCALE_X)
            + 1e-12 * mag.y_range / mag.x_range.max(f64::MIN_POSITIVE),
    );
    relate(
        &mut m,
        "translated pearson",
        run.translated.linear.pearson,
        base.linear.pearson,
        1e-9,
    );
    let slope = base.linear.slope.value().unwrap_or(0.);
    relate(
        &mut m,
        "translated slope",
        run.translated.linear.slope,
        base.linear.slope,
        1e-9 * slope.abs() + 1e-12 * mag.y_range / mag.x_range.max(f64::MIN_POSITIVE),
    );
    let shifted_intercept = map(base.linear.intercept, &|v| v + run.translation);
    relate(
        &mut m,
        "translated intercept",
        run.translated.linear.intercept,
        shifted_intercept,
        2. * intercept_tolerance(
            &mag,
            slope,
            shifted_intercept.value().unwrap_or(0.) + run.translation,
        ) + 2. * ulps(run.translation),
    );
    m.finish()
}

#[test]
fn generated_fixture_replay_ignores_execution_ordinals_after_real_prefill() {
    fn collect(prefill: Option<RelationScenario>) -> Vec<(RelationScenario, Generated)> {
        let mut seen = vec![];
        let mut c =
            certification("fixture replay").executor_with_seed(|input: &RelationScenario, seed| {
                seen.push((input.clone(), generate(input, seed)));
                Ok::<_, ()>(())
            });
        c.data_source(prefill);
        c.domain(scenario_domain());
        c.seed(719).cases_budget(64);
        c.assert_certify();
        seen
    }
    let original = collect(None);
    assert_eq!(original, collect(None));
    let prefilled = collect(Some(original.last().unwrap().0.clone()));
    assert_eq!(prefilled.len(), 64);
    let mut moved = false;
    let mut compared = 0;
    for (ordinal, (scenario, fixture)) in prefilled.iter().enumerate().skip(1) {
        if let Some((previous_ordinal, (_, expected))) = original
            .iter()
            .enumerate()
            .find(|(_, (input, _))| input == scenario)
        {
            assert_eq!(fixture, expected);
            moved |= ordinal != previous_ordinal;
            compared += 1;
        }
    }
    assert!(compared >= 32);
    assert!(moved);
}

#[test]
fn generated_association_scenarios_match_references_facades_and_metamorphic_laws() {
    let mut c =
        certification("relation over generated association scenarios").executor_with_seed(run);
    c.domain(scenario_domain());
    c.seed(184729361).cases_budget(320);
    let case = c.case();
    c.check(
        fact!(linear_metrics_match_two_pass_reference(case.result()))
            .expect(Vec::<String>::new())
            .description("covariance, slope, intercept, Pearson, r squared"),
    );
    c.check(
        fact!(spearman_matches_rank_reference_and_undefined_classification(case.result()))
            .expect(Vec::<String>::new())
            .description("Spearman average ranks and undefined reasons"),
    );
    c.check(
        fact!(difference_summary_matches_sorted_reference(case.result()))
            .expect(Vec::<String>::new())
            .description("difference summaries"),
    );
    c.check(
        fact!(exact_generating_affine_law_is_recovered(
            case.result(),
            case.input()
        ))
        .expect(Vec::<String>::new())
        .description("known affine law and rounding-only residuals"),
    );
    c.check(
        fact!(monotonic_relation_has_unit_rank_correlation(
            case.result(),
            case.input()
        ))
        .expect(Vec::<String>::new())
        .description("monotonic nonlinear rank correlation"),
    );
    c.check(
        fact!(pair_order_does_not_change_metrics(case.result()))
            .expect(Vec::<String>::new())
            .description("pair order invariance"),
    );
    c.check(
        fact!(prepared_comparison_equals_direct_algorithms_with_provenance(case.result()))
            .expect(Vec::<String>::new())
            .description("prepared comparison facade parity"),
    );
    c.check(
        fact!(constant_or_insufficient_side_fails_expectations_closed(
            case.result()
        ))
        .expect(Vec::<String>::new())
        .description("undefined evidence fails expectations closed"),
    );
    c.check(
        fact!(metamorphic_transformations_preserve_mathematical_relationships(case.result()))
            .expect(Vec::<String>::new())
            .description("swap, negation, scaling and translation laws"),
    );
    c.require_pairwise_coverage();
    let report = c.report();
    report.assert_certify();
    assert_eq!(report.cases_executed(), 320);
    assert_eq!(report.checks.len(), 9);
}

mod deterministic {
    use super::*;
    use crate::certifications::test_support::{close, reference_linear, reference_rank, shuffle};
    use crate::certifications::{collection, fact, CheckClass};

    fn defined(metric: Metric, expected: f64) {
        close(metric.value().unwrap(), expected);
    }

    #[test]
    fn positive_negative_nonlinear_and_known_affine_transforms_are_certified() {
        let x = [1., 2., 3., 4., 5.];
        for (y, slope, intercept, direction) in [
            ([2., 4., 6., 8., 10.], 2., 0., 1.),
            ([10., 8., 6., 4., 2.], -2., 12., -1.),
            ([5., 7., 9., 11., 13.], 2., 3., 1.),
        ] {
            let report = numeric_relation(x.into_iter().zip(y)).unwrap();
            defined(report.linear.pearson, direction);
            defined(report.spearman, direction);
            defined(report.linear.slope, slope);
            defined(report.linear.intercept, intercept);
            defined(report.linear.r_squared, 1.);
            defined(report.linear.covariance, 2. * slope);
            defined(report.linear.sample_covariance, 2.5 * slope);
            close(report.residual.unwrap().absolute.max, 0.);
        }
        defined(
            spearman(x.into_iter().zip([1., 4., 9., 16., 25.])).unwrap(),
            1.,
        );
    }

    #[test]
    fn shifted_bivariate_moments_preserve_offset_relationship_and_detect_variance_underflow() {
        let mut values: Vec<_> = (0..128)
            .map(|i| {
                let x = 1e12 + i as f64 / 4.;
                (x, 2. * x + 3.)
            })
            .collect();
        shuffle(&mut values, 31);
        let actual = linear_metrics(values).unwrap();
        defined(actual.pearson, 1.);
        defined(actual.slope, 2.);
        defined(actual.intercept, 3.);
        assert!(matches!(
            pearson([(0., 0.), (1e-200, 1.)]),
            Err(NumericError::ArithmeticUnderflow { .. })
        ));
        assert!(matches!(
            pearson([(0., 0.), (1., 1e-200)]),
            Err(NumericError::ArithmeticUnderflow { .. })
        ));
    }

    #[test]
    fn ties_use_average_ranks_including_signed_zero() {
        defined(
            spearman([1., 2., 2., 3.].into_iter().zip([4., 1., 1., 0.])).unwrap(),
            -1.,
        );
        let x = [-0., 0., 1., 2., 2.];
        let y = [1., 2., 2., 4., 3.];
        let rx = reference_rank(&x);
        let ry = reference_rank(&y);
        let expected = reference_linear(&rx.into_iter().zip(ry).collect::<Vec<_>>()).0;
        defined(spearman(x.into_iter().zip(y)).unwrap(), expected);
    }

    #[test]
    fn empty_singleton_constant_and_nonfinite_metrics_never_succeed_as_nan() {
        assert_eq!(
            pearson([]).unwrap(),
            Metric::Undefined(UndefinedReason::InsufficientPairs)
        );
        assert_eq!(
            spearman([(1., 2.)]).unwrap(),
            Metric::Undefined(UndefinedReason::InsufficientPairs)
        );
        assert_eq!(
            pearson([(1., 2.), (1., 3.)]).unwrap(),
            Metric::Undefined(UndefinedReason::ConstantLeft)
        );
        assert_eq!(
            spearman([(1., 2.), (2., 2.)]).unwrap(),
            Metric::Undefined(UndefinedReason::ConstantRight)
        );
        let constant = linear_metrics([(1., 2.), (2., 2.)]).unwrap();
        assert_eq!(constant.slope, Metric::Defined(0.));
        assert_eq!(constant.intercept, Metric::Defined(2.));
        assert_eq!(
            constant.r_squared,
            Metric::Undefined(UndefinedReason::ConstantRight)
        );
        assert_eq!(numeric_relation([].into_iter()), Err(NumericError::Empty));
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(pearson([(1., 2.), (v, 3.)]).is_err());
            assert!(spearman([(1., v)]).is_err());
            assert!(difference_summary([(v, 1.)]).is_err());
        }
        assert!(linear_metrics([(f64::MAX, 0.), (-f64::MAX, 1.)]).is_err());
    }

    #[test]
    fn pair_differences_percentiles_and_top_keys_have_reference_values() {
        let left = [(3, 30.), (1, 10.), (2, 20.)];
        let right = [(2, 18.), (3, 35.), (1, 11.)];
        let a = collection("left", &left);
        let b = collection("right", &right);
        let joined = a.compare_with(&b).on(|v| v.0, |v| v.0);
        let numeric = joined.numeric(|v| v.1, |v| v.1);
        let summary = numeric.difference().observed.unwrap();
        let largest = numeric.largest(2).observed.unwrap();
        close(summary.signed.mean, 4. / 3.);
        close(summary.absolute.mean, 8. / 3.);
        assert_eq!(summary.median_signed, 1.);
        assert_eq!(summary.median_absolute, 2.);
        close(summary.p95_absolute, 4.7);
        assert_eq!(summary.absolute.max, 5.);
        assert_eq!(
            largest.iter().map(|v| v.key).collect::<Vec<_>>(),
            vec![3, 2]
        );
        assert_eq!(largest[0].signed_difference, 5.);
        assert!(numeric.largest(0).observed.unwrap().is_empty());
        assert!(serde_json::to_value(&summary).is_ok());
    }

    #[test]
    fn bounded_top_disagreements_match_full_sort_for_all_limits_and_ties() {
        let left: Vec<_> = (0..100).map(|key| (key, 0.)).collect();
        let right: Vec<_> = (0..100)
            .map(|key| (key, ((key * 37) % 11) as f64 - 5.))
            .collect();
        let a = collection("left", &left);
        let b = collection("right", &right);
        let joined = a.compare_with(&b).on(|v| v.0, |v| v.0);
        let mut expected = right.clone();
        expected.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()).then(a.0.cmp(&b.0)));
        for top in [0, 1, 2, 5, 99, 100, 200] {
            let report = joined
                .numeric(|v| v.1, |v| v.1)
                .largest(top)
                .observed
                .unwrap();
            assert_eq!(
                report.iter().map(|v| (v.key, v.right)).collect::<Vec<_>>(),
                expected.iter().take(top).copied().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn numeric_failures_retain_paired_source_coordinates() {
        let left = [(7, 1.), (8, 2.)];
        let right = [(8, f64::NAN), (7, 3.)];
        let a = collection("left", &left);
        let b = collection("right", &right);
        let paired = a.compare_with(&b).on(|v| v.0, |v| v.0);
        let result = paired
            .numeric(|v| v.1, |v| v.1)
            .relation()
            .expect(RelationExpectation::spearman_at_least(0.9).unwrap());
        assert_eq!(result.class(), CheckClass::EvaluationError);
        assert!(result.observed().is_err());
        assert_eq!(paired.comparison().matched()[1].key, 8);
        assert_eq!(paired.comparison().matched()[1].right_index, 0);
    }

    #[test]
    fn scenario_expectations_distinguish_direction_scale_and_residuals() {
        let x = [1., 2., 3., 4., 5.];
        for sign in [1., -1.] {
            let y = x.map(|v| sign * 2. * v + 3.);
            let a = collection("left", &x);
            let b = collection("right", &y);
            let paired = a
                .compare_with(&b)
                .on(|v| *v as i64, |v| ((*v - 3.) / (sign * 2.)) as i64);
            let numeric = paired.numeric(|v| *v, |v| *v);
            let expected = if sign > 0. {
                RelationExpectation::spearman_at_least(0.9)
            } else {
                RelationExpectation::spearman_at_most(-0.9)
            }
            .unwrap()
            .bound(
                RelationMeasure::Slope,
                Some(sign * 2. - 0.01),
                Some(sign * 2. + 0.01),
            )
            .unwrap()
            .bound(RelationMeasure::Intercept, Some(2.99), Some(3.01))
            .unwrap()
            .bound(RelationMeasure::RSquared, Some(0.99), None)
            .unwrap()
            .bound(RelationMeasure::ResidualMaxAbsolute, None, Some(1e-10))
            .unwrap();
            assert!(numeric.relation().expect(expected).is_pass());
            let opposite = if sign > 0. {
                RelationExpectation::spearman_at_most(-0.8)
            } else {
                RelationExpectation::spearman_at_least(0.8)
            }
            .unwrap();
            let result = numeric.relation().expect(opposite);
            assert_eq!(result.violations.len(), 1);
            assert_eq!(result.observed.unwrap().linear.count, 5);
        }
        let a = collection("left", &x);
        let numeric = a.compare_with(&a).on(|v| *v as i64, |v| *v as i64);
        let values = numeric.numeric(|v| *v, |v| 100. * *v);
        assert!(values
            .relation()
            .expect(RelationExpectation::spearman_at_least(0.99).unwrap())
            .is_pass());
        let result = values.relation().expect(
            RelationExpectation::spearman_at_least(0.99)
                .unwrap()
                .bound(RelationMeasure::Slope, Some(0.9), Some(1.1))
                .unwrap(),
        );
        assert_eq!(result.violations[0].measure, RelationMeasure::Slope);
        defined(result.observed.unwrap().linear.slope, 100.);
    }

    #[test]
    fn independent_constraint_measures_and_undefined_values_fail_closed() {
        let left = [1., 2., 3.];
        let right = [3., 2., 1.];
        let a = collection("left", &left);
        let b = collection("right", &right);
        let joined = a.compare_with(&b).on(|v| *v as i32, |v| (4. - *v) as i32);
        let report = joined.numeric(|v| *v, |v| *v).relation().observed.unwrap();
        for measure in [
            RelationMeasure::Pearson,
            RelationMeasure::Spearman,
            RelationMeasure::AbsolutePearson,
            RelationMeasure::AbsoluteSpearman,
            RelationMeasure::MeanAbsoluteDifference,
            RelationMeasure::P95AbsoluteDifference,
            RelationMeasure::MaxAbsoluteDifference,
            RelationMeasure::ResidualMeanAbsolute,
        ] {
            let value = super::super::measure(&report, measure).unwrap();
            assert!(fact("relation", report.clone())
                .expect(RelationExpectation::with_bound(measure, Some(value), Some(value)).unwrap())
                .is_pass());
            assert!(!fact("relation", report.clone())
                .expect(
                    if value <= 0.0 {
                        RelationExpectation::with_bound(measure, Some((value + 1.0) / 2.0), None)
                    } else {
                        RelationExpectation::with_bound(measure, None, Some(value / 2.0))
                    }
                    .unwrap()
                )
                .is_pass());
        }
        assert!(
            RelationExpectation::with_bound(RelationMeasure::Slope, Some(2.), Some(1.)).is_err()
        );
        for invalid in [f64::NAN, f64::INFINITY] {
            assert!(RelationExpectation::spearman_at_least(invalid).is_err());
        }
        for invalid in [f64::NAN, f64::NEG_INFINITY] {
            assert!(RelationExpectation::spearman_at_most(invalid).is_err());
        }
        let constant = numeric_relation(left.iter().map(|v| (1., *v))).unwrap();
        assert_eq!(
            fact("constant", constant)
                .expect(RelationExpectation::spearman_at_least(0.1).unwrap())
                .class,
            CheckClass::EvaluationError
        );
    }

    #[test]
    fn generated_centered_moments_ranking_and_fit_match_independent_references() {
        for seed in 1..80 {
            let mut values: Vec<_> = (0..60)
                .map(|i| {
                    (
                        ((i * 7 + seed) % 19) as f64,
                        ((i * 11 + seed * 3) % 23) as f64,
                    )
                })
                .collect();
            shuffle(&mut values, seed);
            let expected = reference_linear(&values);
            let actual = linear_metrics(values.iter().copied()).unwrap();
            for (metric, value) in [
                (actual.pearson, expected.0),
                (actual.covariance, expected.1),
                (actual.slope, expected.2),
                (actual.intercept, expected.3),
                (actual.r_squared, expected.4),
            ] {
                defined(metric, value);
            }
            let rx = reference_rank(&values.iter().map(|v| v.0).collect::<Vec<_>>());
            let ry = reference_rank(&values.iter().map(|v| v.1).collect::<Vec<_>>());
            defined(
                spearman(values.iter().copied()).unwrap(),
                reference_linear(&rx.into_iter().zip(ry).collect::<Vec<_>>()).0,
            );
        }
    }
}
