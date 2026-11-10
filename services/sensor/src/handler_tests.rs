//! Unit tests for the helpers in `handler.rs`.

use connectrpc::ErrorCode;

use super::*;
use crate::drivers::device;

fn frame(rgba: Vec<u8>) -> RawFrame {
    RawFrame { rgba, width: 2, height: 1, device: device(0, "A".into(), true) }
}

#[test]
fn encodes_png_and_jpeg() {
    let pixels = vec![255; 2 * 4];
    assert!(encode(&frame(pixels.clone()), ImageFormat::Png).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    assert!(encode(&frame(pixels), ImageFormat::Jpeg).unwrap().starts_with(&[0xFF, 0xD8]));
}

#[test]
fn wrong_pixel_count_is_internal() {
    let error = encode(&frame(vec![0; 3]), ImageFormat::Png).unwrap_err();
    assert_eq!(error.code, ErrorCode::Internal);
}
