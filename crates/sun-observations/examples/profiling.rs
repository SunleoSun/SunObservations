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
    let cases = runner.case_aggregate();
    runner.check_aggregate(
        fact!({
            let spans = cases.spans::<Wait>().unwrap();
            spans
                .source("elapsed across requested delays")
                .group_by(|row| row.case().input().delay_ms)
                .numeric(|row| row.measurement().duration_ns() as f64)
                .summaries_with_robust()
        })
        .expect(criterion(
            "complete delay groups with valid distributions",
            ViolationClass::Hard,
            |report: &GroupedSummaryReport<u64>| {
                report.groups.iter().map(|group| group.key).eq([1, 2, 3])
                    && report.groups.iter().all(|group| {
                        group.member_count > 0
                            && group.summary.is_ok()
                            && group.robust.as_ref().is_some_and(Result::is_ok)
                    })
            },
        ))
        .description("cross-execution grouped distributions retain every requested delay"),
    );
    runner.assert_certify();
}

fn main() {
    certify();
}
