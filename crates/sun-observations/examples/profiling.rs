use std::time::Duration;
use sun_observations::{certifications::*, evidence::*, *};

/// Minimum delay in the supported 1..=3 millisecond input family.
#[derive(Clone, DomainInput)]
pub struct Request {
    pub delay_ms: u64,
}
/// Production span payload attributing a wait to its requested minimum delay.
#[derive(Debug, serde::Serialize, serde::Deserialize, Observation)]
#[observation(namespace = "paced::wait")]
pub struct Wait {
    pub delay_ms: u64,
}

/// A minimum delay, not a scheduler deadline or an upper latency guarantee.
pub fn wait(request: &Request) -> Result<(), &'static str> {
    if !(1..=3).contains(&request.delay_ms) {
        return Err("delay outside contract");
    }
    let span = observe_span!(Wait {
        delay_ms: request.delay_ms
    })
    .map_err(|_| "start delivery")?;
    std::thread::sleep(Duration::from_millis(request.delay_ms));
    span.finish().map_err(|_| "end delivery")
}

pub fn certify() {
    let mut runner =
        certification("minimum delay across generated inputs").executor(|request: &Request| {
            capture_observations(CaptureConfig::new().activate_all(), || wait(request))
        });
    runner.data_source([
        Request { delay_ms: 1 },
        Request { delay_ms: 1 },
        Request { delay_ms: 3 },
    ]);
    runner.domain(domain!(Request { delay_ms.in_range(1..=3) }));
    runner.cases_budget(16);
    let case = runner.case();
    runner.check(
        fact!({
            let spans = case.spans::<Wait>().unwrap();
            let measurement = spans.values().first();
            (
                spans.values().len(),
                measurement.map(|m| m.value().delay_ms),
                measurement
                    .is_some_and(|m| m.duration() >= Duration::from_millis(case.input().delay_ms)),
            )
        })
        .expect(fact!((1usize, Some(case.input().delay_ms), true)))
        .description("one attributable completed wait satisfying the minimum delay"),
    );
    runner.assert_certify();
}

fn main() {
    certify();
}
