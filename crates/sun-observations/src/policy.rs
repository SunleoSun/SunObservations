/// Applied after selection. Occurrences are counted per observation type and scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    #[default]
    Always,
    Never,
    FirstN(u64),
    /// Admit occurrences N, 2N, ...; zero rejects every occurrence.
    EveryN(u64),
    /// A scope-local cap, equivalent to FirstN for this synchronous admission model.
    MaxPerScope(u64),
}

impl Policy {
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
            Self::FirstN(n) | Self::MaxPerScope(n) => occurrence <= n,
            Self::EveryN(n) => n != 0 && occurrence.is_multiple_of(n),
            Self::Always | Self::Never => unreachable!("handled without a counter"),
        }
    }
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
