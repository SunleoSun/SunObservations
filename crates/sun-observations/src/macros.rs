/// Best-effort emission returning `()`; rejection skips payload construction.
/// Failures remain visible to capture/runtime health accounting. Use `try_observe!`
/// when admission or emission errors must affect caller control flow.
#[macro_export]
macro_rules! observe {
    ($payload:expr $(,)?) => {{
        let _ = $crate::try_observe!($payload);
    }};
}

/// Lazily emit through the canonical runtime path.
/// Returns `Result<bool, ObservationError>`: `Ok(false)` rejects before construction,
/// `Ok(true)` admits/dispatches (not durable persistence), and `Err` reports metadata,
/// serialization, or sink failure.
#[macro_export]
macro_rules! try_observe {
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
