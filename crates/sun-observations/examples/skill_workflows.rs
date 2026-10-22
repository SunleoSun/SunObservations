use serde::{Deserialize, Serialize};
use std::path::Path;
use sun_observations::certifications::*;
use sun_observations::evidence::*;
use sun_observations::*;

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::prediction", tags("evidence"))]
struct Prediction {
    fold: u8,
    target: u8,
    row: i64,
    signal: f64,
    strength: f64,
}
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::rejected", level = WARN)]
struct Rejected {
    reason: String,
}
#[derive(Debug)]
struct Request {
    count: usize,
}
#[derive(Debug)]
struct Output {
    fold: u8,
    target: u8,
    row: i64,
    signal: f64,
    strength: f64,
}
#[derive(Debug, PartialEq)]
enum RunError {
    EmptyRequest,
    Evidence(String),
}
impl From<ObservationError> for RunError {
    fn from(error: ObservationError) -> Self {
        Self::Evidence(error.to_string())
    }
}
fn execute(request: &Request) -> Result<Vec<Output>, RunError> {
    if request.count == 0 {
        observe!(Rejected {
            reason: "empty request".into()
        })?;
        return Err(RunError::EmptyRequest);
    }
    let mut result = Vec::new();
    for row in 0..request.count as i64 {
        let signal = ((row * 17 + row * row * 13) % 101) as f64;
        let strength = signal.abs();
        observe!(Prediction {
            fold: (row % 4) as u8,
            target: 0,
            row,
            signal,
            strength
        })?;
        result.push(Output {
            fold: (row % 4) as u8,
            target: 0,
            row,
            signal: 2. * signal + 3.,
            strength,
        });
    }
    result.reverse();
    Ok(result)
}

pub fn workflows(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let request = Request { count: 64 };
    let run = capture_observations(CaptureConfig::new().activate_tag("evidence"), || {
        execute(&request)
    });
    let output = run.result().as_ref().unwrap();
    let predictions = run.typed::<Prediction>()?;
    let mut c = run.certify("Harness evaluation");
    let input = c.source("request", &request);
    let result = c.collection("result", output);
    let internal = predictions.source("encoder predictions");
    c.section("inputs", |c| {
        c.check(
            "nonempty request",
            input.select(|v| v.count).expect(CountRule::NonEmpty),
        );
    });
    let pair = internal
        .compare_with(&result)
        .on(|v| (v.fold, v.target, v.row), |v| (v.fold, v.target, v.row));
    let signals = pair.numeric(|v| v.signal, |v| v.signal);
    c.section("prediction -> decision", |c| {
        c.check("comparison_fact", pair.comparison_fact().expect_valid());
        c.check(
            "strength preserved",
            pair.equal(|v| v.strength, |v| v.strength, 3).expect_valid(),
        );
        c.check(
            "transformation",
            pair.check_pairs(|a, b| b.signal == 2. * a.signal + 3., 3)
                .expect_valid(),
        );
        c.check(
            "signal correlation",
            signals
                .correlation()
                .expect(CorrelationExpectation::positive(0.99).unwrap()),
        );
        c.check(
            "difference fact",
            signals.difference().expect(criterion(
                "positive mean difference",
                CheckClass::StatisticalViolation,
                |v: &DifferenceSummary| v.signed.mean > 0.,
            )),
        );
        c.section("timing", |c| {
            c.check(
                "causal lag",
                pair.lag(
                    |v| (v.row, v.signal),
                    |v| (v.row, v.signal),
                    LagConfig {
                        range: LagRange { start: -2, end: 2 },
                        measure: LagMeasure::Pearson,
                    },
                )
                .expect(LagExpectation { best_lag: 0 }),
            );
        });
    });
    let groups = internal.group_by(|v| v.fold);
    c.check(
        "fold evidence",
        fact(
            "folds",
            groups.numeric(|v| v.signal).summaries_with_robust(),
        )
        .expect(GroupExpectation {
            minimum_count: 16,
            mean_min: 0.,
            mean_max: 100.,
        }),
    );
    c.section("final result", |c| {
        c.check(
            "count",
            result.count().expect(CountRule::Exact(request.count)),
        );
        c.check(
            "unique domain rows",
            result.unique(|v| (v.fold, v.target, v.row)).expect_valid(),
        );
        c.check(
            "reference formula",
            result
                .matching_count(|v| {
                    v.signal != 2. * ((v.row * 17 + v.row * v.row * 13) % 101) as f64 + 3.
                })
                .expect(0),
        );
    });
    c.check("slope", signals.linear_fit().slope().expect(close_to(2.)));
    c.assert_certify();
    let artifact = JsonlSink::append(path)?;
    for record in run.dataset().records() {
        artifact.write_shared(record.clone())?;
    }
    artifact.flush()?;
    let direct = signals.pearson();
    let accepted = direct.expect(criterion(
        "Pearson >= 0.99",
        CheckClass::StatisticalViolation,
        |m: &Metric| m.value().is_some_and(|v| v >= 0.99),
    ));
    certify("direct API")
        .check(
            "result inspection",
            fact("comparison", accepted.is_pass()).expect(true),
        )
        .assert_certify();
    Ok(())
}

pub fn negative_workflow() {
    let request = Request { count: 0 };
    let run = capture_observations(CaptureConfig::new().activate_namespace("pipeline"), || {
        execute(&request)
    });
    let predictions = run.typed::<Prediction>().unwrap();
    let rejected = run.typed::<Rejected>().unwrap();
    run.certify("rejection at owning input boundary")
        .check_fact_expect(
            "correct failure category",
            run.result().as_ref().err(),
            Some(&RunError::EmptyRequest),
        )
        .check(
            "no forbidden downstream evidence",
            predictions.source("predictions").count().expect(0usize),
        )
        .check(
            "rejection evidence",
            rejected.source("rejection").count().expect(1usize),
        )
        .check(
            "diagnostic category",
            rejected
                .source("rejection")
                .select(|v| v.reason.clone())
                .expect("empty request"),
        )
        .assert_certify();
}

#[allow(dead_code)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("sun-skills-{}.jsonl", std::process::id()));
    let result = workflows(&path);
    let _ = std::fs::remove_file(path);
    result
}
