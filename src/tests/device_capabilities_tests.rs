use crate::web::device_capabilities::{
    filter_cutoff_bytes, pitch_bend_bytes, supports_device_controls,
};

#[test]
fn firmware_2_0_1_supports_device_controls() {
    assert!(supports_device_controls("2.0.1"));
}

#[test]
fn other_firmware_does_not_support_device_controls() {
    assert!(!supports_device_controls("1.1.7"));
    assert!(!supports_device_controls("2.0.0"));
    assert!(!supports_device_controls("2.0.1.0"));
    assert!(!supports_device_controls(""));
}

#[test]
fn filter_cutoff_bytes_encode_cc74_and_reject_over_range() {
    assert_eq!(filter_cutoff_bytes(0xB0, 0), Some([0xB0, 0x4A, 0x00]));
    assert_eq!(filter_cutoff_bytes(0xB2, 127), Some([0xB2, 0x4A, 0x7F]));
    assert_eq!(filter_cutoff_bytes(0xB0, 128), None);
}

#[test]
fn pitch_bend_bytes_split_14_bits_low_then_high() {
    assert_eq!(pitch_bend_bytes(0xE0, 0), Some([0xE0, 0x00, 0x00]));
    assert_eq!(pitch_bend_bytes(0xE0, 8192), Some([0xE0, 0x00, 0x40]));
    assert_eq!(pitch_bend_bytes(0xE0, 16383), Some([0xE0, 0x7F, 0x7F]));
    assert_eq!(pitch_bend_bytes(0xE5, 1), Some([0xE5, 0x01, 0x00]));
}

#[test]
fn pitch_bend_bytes_reject_over_range() {
    assert_eq!(pitch_bend_bytes(0xE0, 16384), None);
    assert_eq!(pitch_bend_bytes(0xE0, u16::MAX), None);
}
