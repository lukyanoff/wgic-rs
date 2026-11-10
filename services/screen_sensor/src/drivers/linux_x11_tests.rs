//! Unit tests for `linux_x11.rs` (pixel conversion only; capturing needs a real X server).

use super::*;

#[test]
fn bgrx_becomes_opaque_rgba() {
    let bgrx = [10, 20, 30, 0, 1, 2, 3, 99];
    assert_eq!(bgrx_to_rgba(&bgrx), vec![30, 20, 10, 255, 3, 2, 1, 255]);
}

#[test]
fn trailing_partial_pixel_is_ignored() {
    assert_eq!(bgrx_to_rgba(&[1, 2, 3, 4, 5]), vec![3, 2, 1, 255]);
}

#[test]
fn driver_name_is_linux_x11() {
    assert_eq!(LinuxX11Driver.name(), "linux-x11");
}
