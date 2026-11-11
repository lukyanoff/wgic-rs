//! Document ids: `<stream>/<yyyy-mm-dd>/<HHMMSS.micros>` in UTC, e.g. `screen-1.frames/2026-11-11/103045.123456`.
//! Every part has a fixed width, so sorting ids of one stream as text sorts them by time.

use chrono::{DateTime, NaiveDateTime};
use sdk::RpcError;

const TIME_FORMAT: &str = "%Y-%m-%d/%H%M%S%.6f";
const MAX_STREAM_LEN: usize = 100;

/// 9999-12-31T23:59:59.999999Z: the largest time an id can hold. Used as "no upper bound".
pub const MAX_TIME_MICROS: i64 = 253_402_300_799_999_999;

/// Builds the id of a document appended to `stream` at `time_micros` (microseconds since the Unix epoch).
pub fn format_id(stream: &str, time_micros: i64) -> Result<String, RpcError> {
    Ok(format!("{stream}/{}", format_time(time_micros)?))
}

/// `<yyyy-mm-dd>/<HHMMSS.micros>`: the part of an id after the stream.
pub fn format_time(time_micros: i64) -> Result<String, RpcError> {
    if !(0..=MAX_TIME_MICROS).contains(&time_micros) {
        return Err(RpcError::invalid_arg(format!("time {time_micros} is out of range")));
    }
    let time = DateTime::from_timestamp_micros(time_micros)
        .ok_or_else(|| RpcError::invalid_arg(format!("time {time_micros} is out of range")))?;
    Ok(time.format(TIME_FORMAT).to_string())
}

/// The part of an id after the stream. Ids of different streams are ordered by this.
pub fn time_part(id: &str) -> &str {
    match id.split_once('/') {
        Some((_stream, time)) => time,
        None => id,
    }
}

/// Checks that `id` is exactly what `format_id` produces. Ids become file paths, so anything else is refused.
pub fn check_id(id: &str) -> Result<(), RpcError> {
    let invalid = || RpcError::invalid_arg(format!("invalid document id '{id}'"));
    let (stream, time) = id.split_once('/').ok_or_else(invalid)?;
    check_stream(stream)?;
    let parsed = NaiveDateTime::parse_from_str(time, TIME_FORMAT).map_err(|_| invalid())?;
    let canonical = format_id(stream, parsed.and_utc().timestamp_micros())?;
    if canonical != id {
        return Err(invalid());
    }
    Ok(())
}

/// A stream name is one folder name: letters, digits, '.', '-' and '_', starting with a letter or digit.
pub fn check_stream(stream: &str) -> Result<(), RpcError> {
    let starts_ok = stream.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    let chars_ok = stream.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !starts_ok || !chars_ok || stream.len() > MAX_STREAM_LEN {
        return Err(RpcError::invalid_arg(format!("invalid stream name '{stream}'")));
    }
    Ok(())
}

#[cfg(test)]
#[path = "doc_id_tests.rs"]
mod tests;
