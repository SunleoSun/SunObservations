use crate::{
    namespace::{matches_subtree, validate_namespace, InvalidNamespace},
    Channel,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(crate) struct Routing {
    channels: HashMap<String, Arc<Channel>>,
    routes: Vec<(String, Arc<Channel>)>,
}
impl Routing {
    pub fn channel(&mut self, name: &str) -> Arc<Channel> {
        self.channels
            .entry(name.to_owned())
            .or_insert_with(|| Arc::new(Channel::new(name)))
            .clone()
    }
    pub fn register(&mut self, namespace: &str, channel: &str) -> Result<(), InvalidNamespace> {
        validate_namespace(namespace)?;
        let channel = self.channel(channel);
        if !self
            .routes
            .iter()
            .any(|(root, c)| root == namespace && Arc::ptr_eq(c, &channel))
        {
            self.routes.push((namespace.to_owned(), channel));
        }
        Ok(())
    }
    pub fn matching(&self, namespace: &str) -> Vec<Arc<Channel>> {
        let mut channels = Vec::new();
        for (root, channel) in &self.routes {
            if matches_subtree(root, namespace) && !channels.iter().any(|c| Arc::ptr_eq(c, channel))
            {
                channels.push(channel.clone());
            }
        }
        channels
    }
    pub fn all(&self) -> Vec<Arc<Channel>> {
        self.channels.values().cloned().collect()
    }
}

#[cfg(test)]
#[path = "routing_test.rs"]
mod tests;
