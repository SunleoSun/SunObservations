use serde::Serialize;
use std::sync::Arc;
use sun_observations::{ConsoleSink, Observation, ObservationEngine, Selection, Sink};

#[derive(Serialize)]
struct Line {
    index: usize,
}
impl Observation for Line {
    const NAMESPACE: &'static str = "console::line";
    fn render_text(&self) -> Option<String> {
        Some(format!("{}\ncontinued", self.index))
    }
}
fn main() {
    let mode = std::env::args().nth(1).unwrap();
    let sink = Arc::new(if mode.starts_with("stdout") {
        ConsoleSink::stdout()
    } else {
        ConsoleSink::stderr()
    });
    if mode.ends_with("flush") {
        sun_observations::global().set_selection(Selection::new().activate_all());
        sun_observations::global()
            .register_namespace_channel("console", "human")
            .unwrap();
        sun_observations::global().add_channel_sink("human", sink.clone());
        for index in 0..4000 {
            sun_observations::global()
                .observe_if_active(|| Line { index })
                .unwrap();
        }
        sink.flush().unwrap();
    } else {
        let engine = ObservationEngine::new();
        engine.set_selection(Selection::new().activate_all());
        engine
            .register_namespace_channel("console", "human")
            .unwrap();
        engine.add_channel_sink("human", sink.clone());
        for index in 0..4000 {
            engine.observe_if_active(|| Line { index }).unwrap();
        }
    }
}
