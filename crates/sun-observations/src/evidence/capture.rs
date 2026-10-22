use crate::evidence::{DecodeError, ObservationDataset, TypedView};
use serde::de::DeserializeOwned;
pub use sun_observations::capture::CaptureStatus;
use sun_observations::certifications::{certification, try_fact, Certification};
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
    pub fn activate_namespace(
        mut self,
        namespace: impl Into<String>,
    ) -> Result<Self, crate::namespace::InvalidNamespace> {
        self.selection = self.selection.activate_namespace(namespace)?;
        Ok(self)
    }
    pub fn activate_namespaces(
        mut self,
        namespaces: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, crate::namespace::InvalidNamespace> {
        for namespace in namespaces {
            self = self.activate_namespace(namespace)?;
        }
        Ok(self)
    }
    pub fn exclude_namespace(
        mut self,
        namespace: impl Into<String>,
    ) -> Result<Self, crate::namespace::InvalidNamespace> {
        self.selection = self.selection.exclude_namespace(namespace)?;
        Ok(self)
    }
    pub fn exclude_namespaces(
        mut self,
        namespaces: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, crate::namespace::InvalidNamespace> {
        for namespace in namespaces {
            self = self.exclude_namespace(namespace)?;
        }
        Ok(self)
    }
    pub fn activate_tag(
        self,
        tag: impl Into<String>,
    ) -> Result<Self, crate::namespace::InvalidTag> {
        self.activate_tags([tag])
    }
    pub fn activate_tags(
        mut self,
        tags: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, crate::namespace::InvalidTag> {
        self.selection = self.selection.activate_tags(tags)?;
        Ok(self)
    }
    pub fn exclude_tag(self, tag: impl Into<String>) -> Result<Self, crate::namespace::InvalidTag> {
        self.exclude_tags([tag])
    }
    pub fn exclude_tags(
        mut self,
        tags: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, crate::namespace::InvalidTag> {
        self.selection = self.selection.exclude_tags(tags)?;
        Ok(self)
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
    pub fn to_typed<O: Observation + DeserializeOwned>(&self) -> Result<TypedView<O>, DecodeError> {
        self.dataset.to_typed::<O>()
    }
    pub fn certification(&self, name: impl Into<String>) -> Certification {
        let mut c = certification(name);
        let completeness = if self.status.is_complete() {
            Ok(true)
        } else {
            Err(format!("incomplete capture: {:?}", self.status))
        };
        c.check(
            "capture completeness",
            try_fact("scoped Observation capture", completeness).expect(true),
        );
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
