//! Unit tests for `logger.rs`.

use super::*;
use crate::kv;
use crate::testing::MemoryLogs;

fn logger(logs: &MemoryLogs) -> Logger {
    Logger::new("greet", Arc::new(logs.clone()))
}

#[test]
fn passes_level_category_message_and_fields() {
    let logs = MemoryLogs::new();
    logger(&logs).warn("slow call", [kv("ms", 900)]);

    let entry = logs.find("slow call").unwrap();
    assert_eq!(entry.level, Level::Warn);
    assert_eq!(entry.category, "greet");
    assert_eq!(entry.field("ms"), Some(&crate::FieldValue::I64(900)));
}

#[test]
fn scope_fields_come_first_and_are_kept_by_child_only() {
    let logs = MemoryLogs::new();
    let parent = logger(&logs);
    let child = parent.with([kv("order_id", 42)]);

    child.info("paid", [kv("amount", 10)]);
    parent.info("idle", []);

    let paid = logs.find("paid").unwrap();
    let keys: Vec<&str> = paid.fields.iter().map(|f| f.key).collect();
    assert_eq!(keys, ["order_id", "amount"]);
    assert!(logs.find("idle").unwrap().fields.is_empty());
}
