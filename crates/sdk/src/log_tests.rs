use super::*;

#[test]
fn memory_logger_keeps_level_message_and_fields() {
    let log = MemoryLogger::default();
    log.info("frame captured", json!({ "bytes": 12 }));
    log.warn("slow", json!({}));

    let entries = log.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, Level::Info);
    assert_eq!(entries[0].message, "frame captured");
    assert_eq!(entries[0].fields["bytes"], json!(12));
    assert_eq!(entries[1].level, Level::Warn);
}

#[test]
fn otel_logger_works_without_a_subscriber() {
    let log = OtelLogger::new("test");
    for level in [Level::Debug, Level::Info, Level::Warn, Level::Error] {
        log.write(level, "hello", json!({ "a": 1 }));
    }
}
