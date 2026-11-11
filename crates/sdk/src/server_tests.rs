use super::*;

#[test]
fn endpoint_name_is_short_service_and_method() {
    assert_eq!(endpoint_name("sensor.v1.SensorService", "Capture"), "SensorService-Capture");
}

#[test]
fn timeout_header_is_capped_and_defaults_to_max_call() {
    let mut headers = HeaderMap::new();
    headers.insert(TIMEOUT_HEADER, "1500");
    assert_eq!(timeout_of(Some(&headers)), Duration::from_millis(1500));
    headers.insert(TIMEOUT_HEADER, "99999999");
    assert_eq!(timeout_of(Some(&headers)), MAX_CALL);
    assert_eq!(timeout_of(None), MAX_CALL);
}
