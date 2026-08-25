//! Tests for the build-time decoded launcher window icon.
//!
//! The icon bytes come from `build.rs`, which decodes
//! `ui/img/td3-control.png`. These assert the decode produced a square
//! RGBA buffer of the declared size, so a re-exported artwork that is
//! the wrong shape or colour type fails here instead of producing a
//! garbled window icon.

use crate::launcher::icon;

#[test]
fn icon_is_square_and_non_empty() {
    let data = icon::icon_data();
    assert!(data.width > 0, "icon width must be positive");
    assert_eq!(
        data.width, data.height,
        "icon must be square, got {}x{}",
        data.width, data.height
    );
}

#[test]
fn icon_buffer_length_matches_dimensions() {
    let data = icon::icon_data();
    let expected = data.width as usize * data.height as usize * 4;
    assert_eq!(
        data.rgba.len(),
        expected,
        "expected {} RGBA bytes for {}x{}, got {}",
        expected,
        data.width,
        data.height,
        data.rgba.len()
    );
}

#[test]
fn icon_is_not_fully_transparent() {
    let data = icon::icon_data();
    let opaque = data
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[3] > 0)
        .count();
    assert!(
        opaque > 0,
        "every pixel is fully transparent, the artwork lost its alpha"
    );
}
