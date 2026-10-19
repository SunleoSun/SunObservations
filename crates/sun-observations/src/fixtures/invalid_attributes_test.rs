use serde::Serialize;
use sun_observations::Observation;

#[derive(Serialize, Observation)]
#[observation(namespace = "a", namespace = "b")]
struct DuplicateNamespace;

#[derive(Serialize, Observation)]
#[observation(namespace = "a", level = INFO, level = WARN)]
struct DuplicateLevel;

#[derive(Serialize, Observation)]
#[observation(namespace = "a", tags("one"), tags("two"))]
struct DuplicateTags;

#[derive(Serialize, Observation)]
#[observation(namespace = "a", other = true)]
struct Unknown;

#[derive(Serialize, Observation)]
#[observation(namespace = 42)]
struct NotAString;

fn main() {}
