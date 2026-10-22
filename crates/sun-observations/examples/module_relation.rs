use serde::{Deserialize, Serialize};
use sun_observations::certifications::*;
use sun_observations::evidence::{capture_observations, CaptureConfig};
use sun_observations::{observe, Observation, ObservationError};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "module::prediction", tags("evidence"))]
struct Prediction {
    row: u64,
    value: f64,
}
struct Request {
    count: usize,
}
struct Output {
    row: u64,
    value: f64,
}
fn execute(request: &Request) -> Result<Vec<Output>, ObservationError> {
    let mut output = Vec::new();
    for row in 0..request.count as u64 {
        let value = row as f64;
        observe!(Prediction { row, value })?;
        output.push(Output {
            row,
            value: 2. * value + 3.,
        });
    }
    output.reverse();
    Ok(output)
}
pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = Request { count: 8 };
    let run = capture_observations(
        CaptureConfig::new().activate_tags(["evidence"]).unwrap(),
        || execute(&request),
    );
    let output = run.result().as_ref().unwrap();
    let predictions = run.to_typed::<Prediction>()?;
    let mut c = run.certification("module semantic transfer");
    let a = predictions.source("producer boundary");
    let b = c.collection("canonical output", output);
    let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
    let values = pair.numeric(|v| v.value, |v| v.value);
    c.check_fact_expect("result count", output.len(), request.count);
    c.section("prediction -> result", |c| {
        c.check("comparison", pair.paired_comparison_fact().expect_valid());
        c.check(
            "correlation",
            values
                .correlation()
                .expect(CorrelationExpectation::spearman_at_least(0.99).unwrap()),
        );
        c.check(
            "slope",
            values.linear_fit().slope().expect(close_to(2.).unwrap()),
        );
        c.check(
            "affine output",
            values.linear_fit().expect(
                LinearFitExpectation::intercept_between(2.99, 3.01)
                    .unwrap()
                    .and_residual_max_absolute_at_most(1e-10)
                    .unwrap(),
            ),
        );
    });
    c.assert_certify();
    Ok(())
}
