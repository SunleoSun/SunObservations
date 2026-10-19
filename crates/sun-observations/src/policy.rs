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
    pub(crate) fn admit(self, counter: &mut u64) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::FirstN(n) | Self::MaxPerScope(n) => {
                *counter = counter.saturating_add(1);
                *counter <= n
            }
            Self::EveryN(n) => {
                *counter = counter.saturating_add(1);
                n != 0 && counter.is_multiple_of(n)
            }
        }
    }
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
