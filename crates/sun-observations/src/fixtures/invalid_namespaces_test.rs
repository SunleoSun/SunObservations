use serde::Serialize;
use sun_observations::Observation;

#[derive(Serialize, Observation)]
#[observation(namespace = "Encoder::Train")]
struct Uppercase;

#[derive(Serialize, Observation)]
#[observation(namespace = "encoder/Train")]
struct Slash;

#[derive(Serialize, Observation)]
#[observation(namespace = "encoder::::train")]
struct EmptySegment;

#[derive(Serialize, Observation)]
#[observation(namespace = "::encoder")]
struct Leading;

#[derive(Serialize, Observation)]
#[observation(namespace = "encoder::")]
struct Trailing;

#[derive(Serialize, Observation)]
#[observation(namespace = "")]
struct Empty;

#[derive(Serialize, Observation)]
#[observation(namespace = "2digits")]
struct Digit;

fn main() {}
