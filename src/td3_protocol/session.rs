use std::time::Duration;

use crate::config::DEFAULT_DEVICE_NAME;
use crate::error::Td3Error;
use crate::midi_io::{exchange_sysex, SysexSender};

use super::cmd;
use super::sync_source::{read_sync_source, SyncSource, SyncSourceFailurePolicy};

// Response types
// ---------------------------------------------------------------------------

/// Result of probing a TD-3 device (product name + firmware version).
pub struct DeviceInfo {
    pub product_name: String,
    pub firmware_version: String,
}

/// Result of establishing a typed TD-3 protocol session.
pub struct SessionInfo {
    pub product_name: String,
    pub firmware_version: String,
    pub sync_source: SyncSource,
    pub sync_source_error: Option<Td3Error>,
}

pub fn probe_device<S: SysexSender + ?Sized>(
    sender: &mut S,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    timeout: Duration,
) -> Result<DeviceInfo, Td3Error> {
    // Product name query
    let name_payload = exchange_sysex(
        sender,
        rx,
        "product name",
        cmd::PRODUCT_NAME_REQ,
        Some(cmd::PRODUCT_NAME_RESP),
        timeout,
    )?;

    // Shape validation: cmd byte + at least 1 char + null terminator
    if name_payload.len() < 3 {
        return Err(Td3Error::PayloadTooShort {
            expected: 3,
            actual: name_payload.len(),
        });
    }

    // Verify null terminator
    let last_name_byte = name_payload[name_payload.len() - 1];
    if last_name_byte != 0x00 {
        return Err(Td3Error::SysexResponse(format!(
            "product name response missing null terminator (last byte: 0x{:02x})",
            last_name_byte
        )));
    }

    // Product name is between the command byte and the trailing null
    let product_name = std::str::from_utf8(&name_payload[1..name_payload.len() - 1])?.to_string();

    if product_name.is_empty() {
        return Err(Td3Error::SysexResponse(
            "product name response contains empty name".to_string(),
        ));
    }

    if product_name != DEFAULT_DEVICE_NAME {
        return Err(Td3Error::DeviceMismatch {
            expected: DEFAULT_DEVICE_NAME.to_owned(),
            actual: product_name.clone(),
        });
    }

    // Firmware version query
    let fw_payload = exchange_sysex(
        sender,
        rx,
        "firmware version",
        cmd::FIRMWARE_REQ,
        Some(cmd::FIRMWARE_RESP),
        timeout,
    )?;

    // Shape validation: cmd(1) + sub-cmd(1) + at least 1 version byte
    if fw_payload.len() < cmd::FIRMWARE_MIN_LEN {
        return Err(Td3Error::PayloadTooShort {
            expected: cmd::FIRMWARE_MIN_LEN,
            actual: fw_payload.len(),
        });
    }

    // Version bytes start at offset 2 (after cmd + sub-cmd)
    let firmware_version = fw_payload[2..]
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<String>>()
        .join(".");

    Ok(DeviceInfo {
        product_name,
        firmware_version,
    })
}

/// Establish a typed protocol session by validating identity, firmware, and
/// sync-source state.
pub fn establish_session<S: SysexSender + ?Sized>(
    sender: &mut S,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    timeout: Duration,
    sync_source_policy: SyncSourceFailurePolicy,
) -> Result<SessionInfo, Td3Error> {
    let device_info = probe_device(sender, rx, timeout)?;
    let (sync_source, sync_source_error) = match read_sync_source(sender, rx, timeout) {
        Ok(value) => (value, None),
        Err(err) => match sync_source_policy {
            SyncSourceFailurePolicy::ReturnError => return Err(err),
            SyncSourceFailurePolicy::DefaultToUsb => (SyncSource::MidiUsb, Some(err)),
        },
    };

    Ok(SessionInfo {
        product_name: device_info.product_name,
        firmware_version: device_info.firmware_version,
        sync_source,
        sync_source_error,
    })
}

/// Retry a fallible operation up to `max_retries` times on timeout.
/// Non-timeout errors propagate immediately.
pub fn with_retry<F, T>(max_retries: u32, operation: &str, mut f: F) -> Result<T, Td3Error>
where
    F: FnMut() -> Result<T, Td3Error>,
{
    let mut last_err = None;
    for attempt in 0..=max_retries {
        match f() {
            Ok(val) => return Ok(val),
            Err(err @ Td3Error::Timeout { .. }) => {
                last_err = Some(err);
                if attempt < max_retries {
                    log::warn!(
                        "{}: timeout (attempt {}/{}), retrying...",
                        operation,
                        attempt + 1,
                        max_retries + 1
                    );
                    continue;
                }
                break;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| Td3Error::Timeout {
        operation: operation.to_string(),
    }))
}
