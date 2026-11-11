use super::*;

// 2026-11-11T10:30:45.123456Z
const TIME: i64 = 1_794_393_045_123_456;

#[test]
fn id_is_stream_date_and_time_with_micros() {
    assert_eq!(format_id("screen-1.frames", TIME).unwrap(), "screen-1.frames/2026-11-11/103045.123456");
}

#[test]
fn text_order_of_ids_is_time_order() {
    let earlier = format_id("s", TIME).unwrap();
    let later = format_id("s", TIME + 1).unwrap();
    let next_day = format_id("s", TIME + 86_400_000_000).unwrap();
    assert!(earlier < later);
    assert!(later < next_day);
}

#[test]
fn formatted_id_passes_the_check() {
    assert!(check_id(&format_id("screen-1.frames", TIME).unwrap()).is_ok());
    assert!(check_id(&format_id("s", MAX_TIME_MICROS).unwrap()).is_ok());
}

#[test]
fn id_that_escapes_the_folder_is_invalid_argument() {
    for id in ["../etc/2026-11-11/103045.123456", "s/../../103045.123456", "s/2026-11-11/../x", "/etc/passwd", "s"] {
        assert_eq!(check_id(id).unwrap_err().code, "invalid_argument", "{id}");
    }
}

#[test]
fn id_with_a_short_time_is_invalid_argument() {
    assert_eq!(check_id("s/2026-11-11/103045.1").unwrap_err().code, "invalid_argument");
}

#[test]
fn stream_name_must_be_one_safe_folder_name() {
    assert!(check_stream("screen-1.frames").is_ok());
    for stream in ["", ".hidden", "a/b", "a b", ".."] {
        assert_eq!(check_stream(stream).unwrap_err().code, "invalid_argument", "{stream}");
    }
}

#[test]
fn time_part_is_everything_after_the_stream() {
    assert_eq!(time_part("s/2026-11-11/103045.123456"), "2026-11-11/103045.123456");
}
