//! Optional device features derived from the probed firmware version.
//! Every function is pure so the detection rule is testable without a
//! MIDI port.

/// Firmware version string, as produced by `probe_device`, whose device
/// accepts Filter Cutoff (CC 74) and Pitch Bend over USB MIDI.
pub(crate) const DEVICE_CONTROL_FIRMWARE: &str = "2.0.1";

/// True when the probed firmware is the version that accepts Filter
/// Cutoff (CC 74) and Pitch Bend over USB MIDI. The product name plays
/// no part: firmware 2.0.1 alone enables the controls.
pub(crate) fn supports_device_controls(firmware: &str) -> bool {
    firmware == DEVICE_CONTROL_FIRMWARE
}

/// Maximum CC 74 value.
pub(crate) const MAX_FILTER_CUTOFF: u16 = 127;
/// Maximum 14-bit pitch bend value.
pub(crate) const MAX_PITCH_BEND: u16 = 16383;

/// Filter Cutoff as a Control Change message: `[status, 0x4A, value]`.
/// `status` must already carry the channel nibble (`0xB0 | channel`).
/// Returns `None` when `value` exceeds 127.
pub(crate) fn filter_cutoff_bytes(status: u8, value: u16) -> Option<[u8; 3]> {
    if value > MAX_FILTER_CUTOFF {
        return None;
    }
    Some([status, 0x4A, value as u8])
}

/// Pitch Bend as `[status, low 7 bits, high 7 bits]`. `status` must
/// already carry the channel nibble (`0xE0 | channel`). Returns `None`
/// when `value` exceeds 16383.
pub(crate) fn pitch_bend_bytes(status: u8, value: u16) -> Option<[u8; 3]> {
    if value > MAX_PITCH_BEND {
        return None;
    }
    Some([status, (value & 0x7F) as u8, (value >> 7) as u8])
}
