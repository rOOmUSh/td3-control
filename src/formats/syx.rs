use crate::error::Td3Error;
use crate::pattern::{pattern_to_sysex, sysex_to_pattern, Pattern, PATTERN_SYSEX_PAYLOAD_LEN};

/// TD-3 sysex framing bytes.
const SYX_PRE: &[u8] = &[0xF0, 0x00, 0x20, 0x32, 0x00, 0x01, 0x0A];
const SYX_POST: &[u8] = &[0xF7];
const SYX_FILE_LEN: usize = SYX_PRE.len() + PATTERN_SYSEX_PAYLOAD_LEN + SYX_POST.len();

/// Export pattern to a complete .syx file (F0 ... F7).
/// Uses the sysex payload reconstructed from the Pattern model.
#[allow(dead_code)]
pub fn export(pattern: &Pattern, patgroup: u8, slot: u8, side: u8) -> Result<Vec<u8>, Td3Error> {
    let payload = pattern_to_sysex(pattern, patgroup, slot, side)?;
    let mut out = Vec::with_capacity(SYX_PRE.len() + payload.len() + SYX_POST.len());
    out.extend_from_slice(SYX_PRE);
    out.extend_from_slice(&payload);
    out.extend_from_slice(SYX_POST);
    Ok(out)
}

/// Export pattern to .syx using raw device payload bytes (preserves exact device dump).
pub fn export_raw(raw_payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(SYX_PRE.len() + raw_payload.len() + SYX_POST.len());
    out.extend_from_slice(SYX_PRE);
    out.extend_from_slice(raw_payload);
    out.extend_from_slice(SYX_POST);
    out
}

/// Import a .syx file into a Pattern.
/// Validates framing (F0 header, manufacturer ID, F7 terminator) then decodes the payload.
pub fn import(data: &[u8]) -> Result<Pattern, Td3Error> {
    if data.len() < SYX_FILE_LEN {
        return Err(Td3Error::FormatError(format!(
            ".syx file too short: {} bytes (minimum {})",
            data.len(),
            SYX_FILE_LEN
        )));
    }
    if data.len() > SYX_FILE_LEN {
        return Err(Td3Error::FormatError(format!(
            ".syx file has unexpected length: expected {} bytes, got {}",
            SYX_FILE_LEN,
            data.len()
        )));
    }
    if &data[..SYX_PRE.len()] != SYX_PRE {
        return Err(Td3Error::FormatError(
            ".syx file has wrong header (expected Behringer TD-3 sysex)".to_string(),
        ));
    }
    if data.last() != Some(&SYX_POST[0]) {
        return Err(Td3Error::FormatError(
            ".syx file missing F7 terminator".to_string(),
        ));
    }
    let payload = &data[SYX_PRE.len()..data.len() - SYX_POST.len()];
    sysex_to_pattern(payload)
}
