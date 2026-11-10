//! Asking for JSON and finding it in a model's answer. Pure functions, unit tested.

/// The system instruction that asks for a JSON answer, with the schema when one is given.
pub fn json_instruction(schema: Option<&str>) -> String {
    let base = "Answer with JSON only: no prose, no markdown fences. Answer directly from your own \
                knowledge; do not use tools.";
    match schema {
        Some(schema) => format!("{base}\nThe JSON must match this JSON Schema:\n{schema}"),
        None => format!("{base}\nUse a JSON object or array."),
    }
}

/// The JSON document in `text`, compact. Models sometimes wrap it in ```json fences or add a
/// sentence before it, so this takes the text from the first `{`/`[` to the last `}`/`]`.
/// `None` when no valid JSON is found.
pub fn extract_json(text: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim()) {
        return Some(value.to_string());
    }
    let start = text.find(['{', '['])?;
    let end = text.rfind(['}', ']'])?;
    if end < start {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(&text[start..=end]).ok()?;
    Some(value.to_string())
}

/// Up to `max` chars of `text`, for error messages.
pub fn excerpt(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
