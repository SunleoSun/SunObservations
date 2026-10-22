/// Applied after selection. Occurrences are counted per observation type and scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    #[default]
    Always,
    Never,
    FirstN(std::num::NonZeroU64),
    /// Admit occurrences N, 2N, ... in the current scope.
    EveryN(std::num::NonZeroU64),
}

impl Policy {
    pub fn first_n(n: u64) -> Option<Self> {
        std::num::NonZeroU64::new(n).map(Self::FirstN)
    }
    pub fn every_n(n: u64) -> Option<Self> {
        std::num::NonZeroU64::new(n).map(Self::EveryN)
    }
    #[inline]
    pub(crate) fn admit_atomic(self, counter: &std::sync::atomic::AtomicU64) -> bool {
        use std::sync::atomic::Ordering;
        match self {
            Self::Always => return true,
            Self::Never => return false,
            _ => {}
        }
        let previous = counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            })
            .expect("update always succeeds");
        let occurrence = previous.saturating_add(1);
        match self {
            Self::FirstN(n) => occurrence <= n.get(),
            Self::EveryN(n) => occurrence.is_multiple_of(n.get()),
            Self::Always | Self::Never => unreachable!("handled without a counter"),
        }
    }
}

#[cfg(test)]
#[path = "policy_certification.rs"]
mod certification;
