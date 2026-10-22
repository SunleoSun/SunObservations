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
    let run = capture_observations(CaptureConfig::new().activate_tags(["evidence"]), || {
        execute(&request)
    });
    let output = run.result().as_ref().unwrap();
    let predictions = run.typed::<Prediction>()?;
    let mut c = run.certify("module semantic transfer");
    let a = predictions.source("producer boundary");
    let b = c.collection("canonical output", output);
    let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
    let values = pair.numeric(|v| v.value, |v| v.value);
    c.check_fact_expect("result count", output.len(), request.count);
    c.section("prediction -> result", |c| {
        c.check("comparison", pair.comparison_fact().expect_valid());
        c.check(
            "correlation",
            values
                .correlation()
                .expect(CorrelationExpectation::positive(0.99).unwrap()),
        );
        c.check("slope", values.linear_fit().slope().expect(close_to(2.)));
        c.check(
            "affine output",
            values.linear_fit().expect(
                LinearFitExpectation::default()
                    .intercept(2.99, 3.01)
                    .unwrap()
                    .residual_max_absolute(1e-10)
                    .unwrap(),
            ),
        );
    });
    c.assert_certify();
    Ok(())
}
