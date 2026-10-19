use super::*;

#[test]
fn severity_has_only_three_wire_values() {
    for level in [
        ObservationLevel::INFO,
        ObservationLevel::WARN,
        ObservationLevel::ERROR,
    ] {
        let encoded = serde_json::to_string(&level).unwrap();
        assert_eq!(encoded, format!("\"{level}\""));
        assert_eq!(
            serde_json::from_str::<ObservationLevel>(&encoded).unwrap(),
            level
        );
    }
    for invalid in ["DEBUG", "TRACE", "info", "OTHER"] {
        assert!(serde_json::from_value::<ObservationLevel>(serde_json::json!(invalid)).is_err());
    }
}
