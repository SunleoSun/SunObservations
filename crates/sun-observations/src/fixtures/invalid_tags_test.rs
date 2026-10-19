use serde::Serialize;
use sun_observations::Observation;

#[derive(Serialize, Observation)]
#[observation(namespace = "bad::tag", tags("Upper"))]
struct Upper;

#[derive(Serialize, Observation)]
#[observation(namespace = "bad::empty_tag", tags(""))]
struct Empty;

#[derive(Serialize, Observation)]
#[observation(namespace = "bad::tree_tag", tags("a::b"))]
struct Tree;

fn main() {}
