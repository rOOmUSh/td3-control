//! Typed TD-3 protocol layer.
//!
//! Owns all device-level request/response definitions.
//! No raw byte slicing or protocol constants leak outside this module.
//!
//! Every response decoder validates:
//! - command byte (via transport layer)
//! - exact or minimum payload length required by the response
//! - field boundaries and structure
//! - text field encoding safety

use crate::error::Td3Error;

mod cmd {
    /// Request product name.
    pub const PRODUCT_NAME_REQ: &[u8] = &[0x06];
    /// Expected response command byte for product name.
    pub const PRODUCT_NAME_RESP: u8 = 0x07;

    /// Request firmware version.
    pub const FIRMWARE_REQ: &[u8] = &[0x08, 0x00];
    /// Expected response command byte for firmware version.
    pub const FIRMWARE_RESP: u8 = 0x09;
    /// Minimum firmware response: cmd(1) + sub-cmd(1) + at least 1 version byte.
    pub const FIRMWARE_MIN_LEN: usize = 3;

    /// Request pattern download.
    pub const PATTERN_DOWNLOAD_REQ: u8 = 0x77;
    /// Expected response command byte for pattern dump.
    pub const PATTERN_DUMP_RESP: u8 = 0x78;

    /// Expected response command byte for upload ACK.
    pub const UPLOAD_ACK_RESP: u8 = 0x01;
    /// Expected ACK payload length (3 bytes: cmd + 2 status bytes).
    pub const UPLOAD_ACK_LEN: usize = 3;

    /// Request full configuration dump.
    pub const CONFIG_REQ: &[u8] = &[0x75];
    /// Expected response command byte for configuration dump.
    pub const CONFIG_RESP: u8 = 0x76;
    /// Payload offset of the clock-source byte inside a configuration dump
    /// (cmd byte at index 0, sync source at index 9).
    pub const CONFIG_SYNC_SOURCE_OFFSET: usize = 9;
    /// Minimum configuration response length to safely read the sync-source byte.
    pub const CONFIG_MIN_LEN: usize = CONFIG_SYNC_SOURCE_OFFSET + 1;

    /// Set Sequencer Clock Source command byte.
    pub const SET_CLOCK_SOURCE_REQ: u8 = 0x1B;
}

fn validate_upload_ack(operation: &str, ack: &[u8]) -> Result<(), Td3Error> {
    if ack.len() != cmd::UPLOAD_ACK_LEN {
        return Err(Td3Error::UploadFailed(format!(
            "unexpected {} ACK length: expected {} bytes, got {}",
            operation,
            cmd::UPLOAD_ACK_LEN,
            ack.len()
        )));
    }
    if ack[1] != 0x00 || ack[2] != 0x00 {
        return Err(Td3Error::UploadFailed(format!(
            "{} ACK status bytes indicate failure: 0x{:02x} 0x{:02x}",
            operation, ack[1], ack[2]
        )));
    }
    Ok(())
}

mod session;
mod sync_source;
mod transfer;

pub use session::{establish_session, probe_device, with_retry, DeviceInfo, SessionInfo};
#[allow(unused_imports)]
pub use sync_source::read_sync_source;
pub use sync_source::{set_sync_source, SyncSource, SyncSourceFailurePolicy};
pub use transfer::{download_pattern, upload_pattern, upload_raw_payload};
