use super::*;
use crate::certifications::test_support::{
    assert_pairwise_certified, reference_linear, reference_rank, shuffle, ulps, Mismatches, Rng,
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

struct Generated {
    pairs: Vec<(f64, f64)>,
    order: Vec<usize>,
    left_unit: f64,
    right_unit: f64,
}
fn generate(scenario: &RelationScenario, salt: u64) -> Generated {
    let mut rng = Rng::new(salt);
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
        Order::Shuffled => shuffle(&mut order, salt ^ 0x5eed),
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
fn run(scenario: &RelationScenario, salt: u64) -> Result<RelationRun, NumericError> {
    let generated = generate(scenario, salt);
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
fn generated_association_scenarios_match_references_facades_and_metamorphic_laws() {
    let mut salt = 0;
    let mut c = certification("relation over generated association scenarios").executor(
        move |scenario: &RelationScenario| {
            salt += 1;
            run(scenario, salt)
        },
    );
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
    let report = c.report();
    assert_pairwise_certified(&report);
    assert_eq!(report.cases_executed(), 320);
    assert_eq!(report.checks.len(), 9);
}
