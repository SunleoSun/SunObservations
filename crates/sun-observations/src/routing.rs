use crate::{
    namespace::{prefixes, validate_namespace, InvalidNamespace},
    Channel, Sink,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Default)]
pub(crate) struct Routing {
    channels: HashMap<String, Channel>,
    routes: HashMap<String, Vec<String>>,
}
impl Routing {
    pub fn add_sink(&mut self, channel: &str, sink: Arc<dyn Sink>, needs_render_text: bool) {
        let channel = self
            .channels
            .entry(channel.to_owned())
            .or_insert_with(Channel::new);
        channel.needs_render_text |= needs_render_text;
        channel.sinks.push(sink);
    }
    pub fn clear_sinks(&mut self, channel: &str) -> Vec<Arc<dyn Sink>> {
        self.channels
            .get_mut(channel)
            .map(|channel| {
                channel.needs_render_text = false;
                std::mem::take(&mut channel.sinks)
            })
            .unwrap_or_default()
    }
    pub fn register(&mut self, namespace: &str, channel: &str) -> Result<(), InvalidNamespace> {
        validate_namespace(namespace)?;
        self.channels
            .entry(channel.to_owned())
            .or_insert_with(Channel::new);
        let channels = self.routes.entry(namespace.to_owned()).or_default();
        if !channels.iter().any(|c| c == channel) {
            channels.push(channel.to_owned());
        }
        Ok(())
    }
    pub fn matching(&self, namespace: &str) -> Vec<&Channel> {
        let mut seen = HashSet::new();
        let mut channels = Vec::new();
        for prefix in prefixes(namespace) {
            if let Some(names) = self.routes.get(prefix) {
                for name in names {
                    if seen.insert(name) {
                        channels.push(&self.channels[name]);
                    }
                }
            }
        }
        channels
    }
    pub fn sinks(&self) -> Vec<Arc<dyn Sink>> {
        let mut sinks = Vec::new();
        for channel in self.channels.values() {
            for sink in &channel.sinks {
                if !sinks.iter().any(|s| Arc::ptr_eq(s, sink)) {
                    sinks.push(sink.clone());
                }
            }
        }
        sinks
    }
}

#[cfg(test)]
#[path = "routing_certification.rs"]
mod certification;
