use crate::{DecodeError, ObservationDataset, TypedView};
use serde::de::DeserializeOwned;
use sun_certifications::{certify, try_fact, Certification};
pub use sun_observations::capture::CaptureStatus;
use sun_observations::{capture::ScopedCapture, MemorySinkConfig, Observation, Selection};

/// Local canonical selection and bounded storage for one production execution.
#[derive(Clone, Debug, Default)]
pub struct CaptureConfig {
    selection: Selection,
    memory: MemorySinkConfig,
}
impl CaptureConfig {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn selection(mut self, selection: Selection) -> Self {
        self.selection = selection;
        self
    }
    pub fn retention(mut self, config: MemorySinkConfig) -> Self {
        self.memory = config;
        self
    }
    pub fn activate_all(mut self) -> Self {
        self.selection = self.selection.activate_all();
        self
    }
    pub fn activate_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.selection = self.selection.activate_namespace(namespace);
        self
    }
    pub fn activate_namespaces(
        mut self,
        namespaces: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        for namespace in namespaces {
            self = self.activate_namespace(namespace);
        }
        self
    }
    pub fn exclude_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.selection = self.selection.exclude_namespace(namespace);
        self
    }
    pub fn exclude_namespaces(
        mut self,
        namespaces: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        for namespace in namespaces {
            self = self.exclude_namespace(namespace);
        }
        self
    }
    pub fn activate_tag(self, tag: impl Into<String>) -> Self {
        self.activate_tags([tag])
    }
    pub fn activate_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.selection = self.selection.activate_tags(tags);
        self
    }
    pub fn exclude_tag(self, tag: impl Into<String>) -> Self {
        self.exclude_tags([tag])
    }
    pub fn exclude_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.selection = self.selection.exclude_tags(tags);
        self
    }
}
/// Application result and immutable capture evidence; application errors remain ordinary data.
pub struct CapturedRun<T> {
    result: T,
    dataset: ObservationDataset,
    status: CaptureStatus,
}
impl<T> CapturedRun<T> {
    pub fn result(&self) -> &T {
        &self.result
    }
    pub fn dataset(&self) -> &ObservationDataset {
        &self.dataset
    }
    pub fn status(&self) -> CaptureStatus {
        self.status
    }
    pub fn typed<O: Observation + DeserializeOwned>(&self) -> Result<TypedView<O>, DecodeError> {
        self.dataset.typed::<O>()
    }
    pub fn certify(&self, name: impl Into<String>) -> Certification {
        let mut c = certify(name);
        if !self.status.is_complete() {
            c.check(
                "capture completeness",
                try_fact::<bool, _>(
                    "scoped Observation capture",
                    Err(format!("incomplete capture: {:?}", self.status)),
                )
                .expect(true),
            );
        }
        c
    }
}
/// Captures only the current propagated context. The guard restores its predecessor even on unwind.
/// Join propagated workers and finish spans inside the closure; escaped contexts make proof incomplete.
pub fn capture_observations<T>(
    config: CaptureConfig,
    operation: impl FnOnce() -> T,
) -> CapturedRun<T> {
    let guard = ScopedCapture::new(config.selection, config.memory);
    let result = operation();
    let (records, status) = guard.finish();
    CapturedRun {
        result,
        dataset: ObservationDataset::from_shared(records),
        status,
    }
}
#[cfg(test)]
#[path = "capture_certification.rs"]
mod certification;
