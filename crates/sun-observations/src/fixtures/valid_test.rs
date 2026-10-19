use serde::Serialize;
use sun_observations::{observe, observe_span, Observation, ObservationLevel};

#[derive(Serialize, Observation)]
#[observation(namespace = "downstream::renamed", tags("detail", "costly"))]
struct Renamed<T: Clone> where T: Serialize { value: T }

#[derive(Serialize, Observation)]
#[observation(namespace = "downstream::warn", level = WARN)]
struct Warning;

#[derive(Serialize, Observation)]
#[observation(namespace = "downstream::error", level = ERROR, tags())]
struct Error;

#[derive(Serialize, Observation)]
#[observation(namespace = "downstream::info", level = INFO)]
struct Info;

fn main() {
    assert_eq!(Renamed::<u32>::NAMESPACE, "downstream::renamed");
    assert_eq!(Renamed::<u32>::LEVEL, ObservationLevel::INFO);
    assert_eq!(Warning::LEVEL, ObservationLevel::WARN);
    assert_eq!(Error::LEVEL, ObservationLevel::ERROR);
    assert_eq!(Info::LEVEL, ObservationLevel::INFO);
    assert_eq!(Renamed::<u32>::TAGS, ["detail", "costly"]);
    assert!(!observe!(Renamed { value: 7u32 }).unwrap());
    assert!(!observe_span!(Warning).unwrap().is_active());
}
