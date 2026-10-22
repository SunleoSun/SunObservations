use super::*;

#[test]
fn severity_has_only_three_wire_values() {
    let mut certification = sun_certifications::certify("severity has only three wire values");
    for level in [
        ObservationLevel::INFO,
        ObservationLevel::WARN,
        ObservationLevel::ERROR,
    ] {
        let encoded = serde_json::to_string(&level).unwrap();
        certification.check_fact_expect("encoded", &(encoded), &(format!("\"{level}\"")));
        certification.check_fact_expect(
            "serde_json::from_str::<ObservationLevel>(&encoded).unwrap()",
            &(serde_json::from_str::<ObservationLevel>(&encoded).unwrap()),
            &(level),
        );
    }
    for invalid in ["DEBUG", "TRACE", "info", "OTHER"] {
        certification.check_fact_expect(
            "serde_json::from_value::<ObservationLevel>(serde_json::json!(invalid)).is_err()",
            serde_json::from_value::<ObservationLevel>(serde_json::json!(invalid)).is_err(),
            true,
        );
    }
    certification.assert_certify();
}
