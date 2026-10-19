use serde::Serialize;
use sun_observations::Observation;

#[derive(Serialize, Observation)]
#[observation(namespace = "bad::debug", level = DEBUG)]
struct Debug;

#[derive(Serialize, Observation)]
#[observation(namespace = "bad::trace", level = TRACE)]
struct Trace;

#[derive(Serialize, Observation)]
#[observation(namespace = "bad::unknown", level = FATAL)]
struct Unknown;

#[derive(Serialize, Observation)]
#[observation(namespace = "bad::string", level = "INFO")]
struct StringLevel;

fn main() {}
