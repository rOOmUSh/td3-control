//! Channel-voice status byte construction for device-addressed MIDI.
//!
//! A TD-3 accepts channel-voice messages only on the channel it is
//! configured for and silently discards every other channel. Host
//! audition and the keyboard note preview both address the device
//! directly, so both must encode the configured channel into the status
//! nibble; SysEx transfers and MIDI realtime transport carry no channel
//! and do not use this.

/// Combine a channel-voice status base (`0x80` Note Off, `0x90` Note On,
/// `0xB0` Control Change) with a 1-based MIDI `channel`.
///
/// MIDI encodes the channel as `status_base | (channel - 1)`. Channels
/// outside 1 through 16 are clamped into range: the alternative is a
/// carry into the status nibble, which turns a Note Off into an entirely
/// different message type and would leave a note sounding.
pub fn channel_status(status_base: u8, channel: u8) -> u8 {
    status_base | (channel.clamp(1, 16) - 1)
}
