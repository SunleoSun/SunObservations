use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use sun_certifications::*;
use sun_observations::*;
use sun_observations_data_source::*;

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
}
fn execute(engine: &ObservationEngine, request: &Request) -> Result<Vec<Output>, RunError> {
    if request.count == 0 {
        engine
            .observe_if_active(|| Rejected {
                reason: "empty request".into(),
            })
            .unwrap();
        return Err(RunError::EmptyRequest);
    }
    let mut result = Vec::new();
    for row in 0..request.count as i64 {
        let signal = ((row * 17 + row * row * 13) % 101) as f64;
        let strength = signal.abs();
        engine
            .observe_if_active(|| Prediction {
                fold: (row % 4) as u8,
                target: 0,
                row,
                signal,
                strength,
            })
            .unwrap();
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
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("capture", memory.clone());
    engine.add_channel_sink("capture", Arc::new(JsonlSink::append(path)?));
    engine.register_namespace_channel("pipeline", "capture")?;
    engine.set_selection(Selection::new().activate_namespace("pipeline"));
    let _scope = start_scope(ScopeConfig::new("semantic-run"));
    let request = Request { count: 64 };
    let output = execute(&engine, &request).unwrap();
    engine.flush_channels()?;
    let dataset = ObservationDataset::from_memory(&memory);
    let selected = dataset.query(EnvelopeQuery {
        proc: Some(engine.process_id()),
        scope: Some(Some("semantic-run")),
        phase: Some(None),
        ..Default::default()
    });
    let predictions = selected.typed::<Prediction>()?;
    let mut c = certify("Harness evaluation");
    let input = c.source("request", &request);
    let result = c.collection("result", &output);
    let internal = predictions.source("encoder predictions");
    c.section("inputs", |c| {
        c.check(
            "nonempty request",
            input.select(|v| v.count).compare(CountRule::NonEmpty),
        );
    });
    let pair = internal
        .compare_with(&result)
        .on(|v| (v.fold, v.target, v.row), |v| (v.fold, v.target, v.row));
    let signals = pair.numeric(|v| v.signal, |v| v.signal);
    c.section("prediction -> decision", |c| {
        c.check("coverage", pair.coverage().compare(AllValid));
        c.check(
            "strength preserved",
            pair.equal(|v| v.strength, |v| v.strength, 3)
                .compare(AllValid),
        );
        c.check(
            "transformation",
            pair.check_pairs(|a, b| b.signal == 2. * a.signal + 3., 3)
                .compare(AllValid),
        );
        c.check(
            "signal relation",
            signals.relation().compare(
                RelationExpectation::positive(0.99)
                    .unwrap()
                    .bound(RelationMeasure::Slope, Some(1.99), Some(2.01))
                    .unwrap(),
            ),
        );
        c.check(
            "difference measured",
            signals.difference().compare(criterion(
                "positive mean difference",
                CheckClass::Statistical,
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
                .compare(LagExpectation { best_lag: 0 }),
            );
        });
    });
    let groups = internal.group_by(|v| v.fold);
    c.check(
        "fold evidence",
        measured(
            "folds",
            groups.numeric(|v| v.signal).summaries_with_robust(),
        )
        .compare(GroupExpectation {
            minimum_count: 16,
            mean_min: 0.,
            mean_max: 100.,
        }),
    );
    c.section("final result", |c| {
        c.check(
            "count",
            result.count().compare(CountRule::Exact(request.count)),
        );
        c.check(
            "unique domain rows",
            result
                .unique(|v| (v.fold, v.target, v.row))
                .compare(AllValid),
        );
        c.check(
            "reference formula",
            result
                .matching_count(|v| {
                    v.signal != 2. * ((v.row * 17 + v.row * v.row * 13) % 101) as f64 + 3.
                })
                .compare(Equal(0)),
        );
    });
    c.assert_certify();
    let direct = signals.pearson();
    let accepted = direct.compare(criterion(
        "Pearson >= 0.99",
        CheckClass::Statistical,
        |m: &Metric| m.value().is_some_and(|v| v >= 0.99),
    ));
    certify("direct API")
        .check(
            "result inspection",
            measured("comparison", accepted.is_pass()).compare(Equal(true)),
        )
        .assert_certify();
    Ok(())
}

pub fn negative_workflow() {
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("capture", memory.clone());
    engine
        .register_namespace_channel("pipeline", "capture")
        .unwrap();
    engine.set_selection(Selection::new().activate_namespace("pipeline"));
    let request = Request { count: 0 };
    let outcome = execute(&engine, &request);
    let dataset = ObservationDataset::from_memory(&memory);
    let predictions = dataset.typed::<Prediction>().unwrap();
    let rejected = dataset.typed::<Rejected>().unwrap();
    certify("rejection at owning input boundary")
        .check(
            "correct failure category",
            measured("result", outcome.err()).compare(Equal(Some(RunError::EmptyRequest))),
        )
        .check(
            "no forbidden downstream evidence",
            predictions.source("predictions").count().compare(Equal(0)),
        )
        .check(
            "rejection evidence",
            rejected
                .source("rejection")
                .count()
                .compare(CountRule::Exact(1)),
        )
        .check(
            "diagnostic category",
            rejected
                .source("rejection")
                .select(|v| v.reason.clone())
                .compare(Equal("empty request".into())),
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
