/// Lazily construct a typed observation only after selection and policy admission.
/// Returns Result<bool, ObservationError>; false means rejected, true means admitted.
#[macro_export]
macro_rules! observe {
    ($payload:expr $(,)?) => {
        $crate::observe_if_active(|| $payload)
    };
}

/// Typed RAII span. Returns Result<SpanGuard, ObservationError>, including an inert guard
/// when rejected. Keep the guard alive for the measured interval.
#[macro_export]
macro_rules! observe_span {
    ($payload:expr $(,)?) => {
        $crate::span_if_active(|| $payload)
    };
}
