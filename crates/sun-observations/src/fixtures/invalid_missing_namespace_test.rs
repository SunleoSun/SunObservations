use serde::Serialize;
use sun_observations::Observation;

#[derive(Serialize, Observation)]
struct Missing;

fn main() {}
