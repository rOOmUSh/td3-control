use std::time::Duration;

use crate::error::Td3Error;
use crate::midi_io::{exchange_sysex, exchange_sysex_matching, SysexSender};
use crate::pattern::{self, Pattern};

use super::{cmd, validate_upload_ack};

pub fn download_pattern<S: SysexSender + ?Sized>(
    sender: &mut S,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    patgroup: u8,
    slot: u8,
    side: u8,
    timeout: Duration,
) -> Result<(Vec<u8>, Pattern), Td3Error> {
    let slot_addr = pattern::validate_pattern_address(patgroup, slot, side)?;
    let desc = format!(
        "download pattern G{}-P{}{}",
        patgroup + 1,
        slot + 1,
        if side == 0 { "A" } else { "B" }
    );

    let raw_payload = exchange_sysex_matching(
        sender,
        rx,
        &desc,
        &[cmd::PATTERN_DOWNLOAD_REQ, patgroup, slot_addr],
        Some(cmd::PATTERN_DUMP_RESP),
        timeout,
        |payload| pattern_dump_address_matches(payload, patgroup, slot_addr),
    )?;

    let pattern = pattern::sysex_to_pattern(&raw_payload)?;
    Ok((raw_payload, pattern))
}

fn pattern_dump_address_matches(
    payload: &[u8],
    patgroup: u8,
    slot_addr: u8,
) -> Result<bool, Td3Error> {
    if payload.len() != pattern::PATTERN_SYSEX_PAYLOAD_LEN {
        log::debug!(
            "Skipping pattern dump response with length {}, expected {}",
            payload.len(),
            pattern::PATTERN_SYSEX_PAYLOAD_LEN
        );
        return Ok(false);
    }
    Ok(payload[1] == patgroup && payload[2] == slot_addr)
}

pub fn upload_pattern<S: SysexSender + ?Sized>(
    sender: &mut S,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    pattern: &Pattern,
    patgroup: u8,
    slot: u8,
    side: u8,
    timeout: Duration,
) -> Result<(), Td3Error> {
    let sysex_payload = pattern::pattern_to_sysex(pattern, patgroup, slot, side)?;

    let ack = exchange_sysex(
        sender,
        rx,
        "upload pattern",
        sysex_payload.as_slice(),
        Some(cmd::UPLOAD_ACK_RESP),
        timeout,
    )?;

    validate_upload_ack("upload pattern", &ack)?;
    log::debug!("Upload ACK validated");
    Ok(())
}

/// Upload a raw 112-byte bank-record payload to the TD-3 without going through
/// the `Pattern` decode/re-encode path.
///
/// Used by `import-bank` where byte-preserving upload is important (CLEAR'd
/// slots with pitch/accent/slide residue survive the round-trip unchanged).
pub fn upload_raw_payload<S: SysexSender + ?Sized>(
    sender: &mut S,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    group: u8,
    slot_addr: u8,
    payload: &[u8],
    timeout: Duration,
) -> Result<(), Td3Error> {
    if payload.len() != 112 {
        return Err(Td3Error::FormatError(format!(
            "upload_raw_payload: expected 112-byte payload, got {}",
            payload.len()
        )));
    }
    if group > 3 || slot_addr > 15 {
        return Err(Td3Error::FormatError(format!(
            "upload_raw_payload: invalid address group={} slot_addr={}",
            group, slot_addr
        )));
    }

    let mut sysex_body = Vec::with_capacity(3 + payload.len());
    sysex_body.push(0x78);
    sysex_body.push(group);
    sysex_body.push(slot_addr);
    sysex_body.extend_from_slice(payload);

    let ack = exchange_sysex(
        sender,
        rx,
        "upload raw pattern",
        sysex_body.as_slice(),
        Some(cmd::UPLOAD_ACK_RESP),
        timeout,
    )?;

    validate_upload_ack("upload raw pattern", &ack)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sync source query / set
// ---------------------------------------------------------------------------
