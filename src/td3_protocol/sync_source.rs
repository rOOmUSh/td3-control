use std::time::Duration;

use crate::error::Td3Error;
use crate::midi_io::{exchange_sysex, SysexSender};

use super::{cmd, validate_upload_ack};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncSourceFailurePolicy {
    #[allow(dead_code)]
    ReturnError,
    DefaultToUsb,
}

/// MIDI sync source selector for the TD-3 sequencer clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncSource {
    Internal = 0x00,
    MidiDin = 0x01,
    MidiUsb = 0x02,
    Trigger = 0x03,
}

impl SyncSource {
    /// Decode a single byte from a TD-3 configuration response.
    pub fn from_byte(byte: u8) -> Result<Self, Td3Error> {
        match byte {
            0x00 => Ok(SyncSource::Internal),
            0x01 => Ok(SyncSource::MidiDin),
            0x02 => Ok(SyncSource::MidiUsb),
            0x03 => Ok(SyncSource::Trigger),
            other => Err(Td3Error::SysexResponse(format!(
                "invalid sync source byte: 0x{:02x}",
                other
            ))),
        }
    }

    /// Encode the value as the byte the TD-3 expects on the wire.
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    /// Wire/UI string identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            SyncSource::Internal => "int",
            SyncSource::MidiDin => "din",
            SyncSource::MidiUsb => "usb",
            SyncSource::Trigger => "trig",
        }
    }

    /// Parse the wire/UI string identifier.
    pub fn from_str(value: &str) -> Result<Self, Td3Error> {
        match value {
            "int" => Ok(SyncSource::Internal),
            "din" => Ok(SyncSource::MidiDin),
            "usb" => Ok(SyncSource::MidiUsb),
            "trig" => Ok(SyncSource::Trigger),
            other => Err(Td3Error::CliError(format!(
                "unknown sync source '{}'",
                other
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol constants
// ---------------------------------------------------------------------------

pub fn read_sync_source<S: SysexSender + ?Sized>(
    sender: &mut S,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    timeout: Duration,
) -> Result<SyncSource, Td3Error> {
    let payload = exchange_sysex(
        sender,
        rx,
        "configuration",
        cmd::CONFIG_REQ,
        Some(cmd::CONFIG_RESP),
        timeout,
    )?;

    if payload.len() < cmd::CONFIG_MIN_LEN {
        return Err(Td3Error::PayloadTooShort {
            expected: cmd::CONFIG_MIN_LEN,
            actual: payload.len(),
        });
    }

    SyncSource::from_byte(payload[cmd::CONFIG_SYNC_SOURCE_OFFSET])
}

/// Set the TD-3 sequencer clock source. Waits for the standard config-set ACK.
pub fn set_sync_source<S: SysexSender + ?Sized>(
    sender: &mut S,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    source: SyncSource,
    timeout: Duration,
) -> Result<(), Td3Error> {
    let request = [cmd::SET_CLOCK_SOURCE_REQ, source.as_byte()];

    let ack = exchange_sysex(
        sender,
        rx,
        "set sync source",
        &request,
        Some(cmd::UPLOAD_ACK_RESP),
        timeout,
    )?;

    validate_upload_ack("set sync source", &ack)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Retry wrapper
// ---------------------------------------------------------------------------
