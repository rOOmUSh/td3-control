//! SynthTribe .seq single-pattern file format.
//!
//! A .seq file is a 146-byte SynthTribe save with:
//!   - file header (magic + UTF-16BE product + firmware version, each
//!     preceded by a big-endian u32 byte length)
//!   - BE u32 payload size (always 112)
//!   - 112-byte pattern payload, identical to the TD-3 SysEx payload
//!     minus its 3-byte device header (message kind + patgroup + slot)
//!
//! The payload starts with a 2-byte `00 00` marker (SynthTribe's equivalent
//! of the `00 01` SysEx marker), then pitch/accent/slide/footer exactly
//! as in a SysEx pattern dump.

use crate::error::Td3Error;
use crate::pattern::{pattern_to_sysex, sysex_to_pattern, Pattern};

/// SynthTribe .seq magic bytes.
const MAGIC: [u8; 4] = [0x23, 0x98, 0x54, 0x76];

/// UTF-16BE "TD-3" (product name).
const PRODUCT_UTF16BE: [u8; 8] = [0x00, 0x54, 0x00, 0x44, 0x00, 0x2D, 0x00, 0x33];

/// UTF-16BE "1.3.7" (firmware version captured from reference file).
const VERSION_UTF16BE: [u8; 10] = [0x00, 0x31, 0x00, 0x2E, 0x00, 0x33, 0x00, 0x2E, 0x00, 0x37];

/// Pattern payload length (constant across SynthTribe single-pattern saves).
const PAYLOAD_LEN: u32 = 112;

/// Upper bound for the product/version string lengths. Prevents a
/// malformed length prefix from pointing far past the end of the file.
const MAX_STRING_BYTES: u32 = 64;

pub fn export(pattern: &Pattern) -> Result<Vec<u8>, Td3Error> {
    // Reuse the SysEx encoder and strip its 3-byte device header.
    let sysex = pattern_to_sysex(pattern, 0, 0, 0)?;
    let mut payload = sysex[3..].to_vec();

    // SynthTribe single-pattern files use marker 0x00 0x00 rather than the
    // SysEx dump's 0x00 0x01. Match the reference file byte-for-byte.
    payload[0] = 0x00;
    payload[1] = 0x00;

    let mut out = Vec::with_capacity(34 + PAYLOAD_LEN as usize);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&(PRODUCT_UTF16BE.len() as u32).to_be_bytes());
    out.extend_from_slice(&PRODUCT_UTF16BE);
    out.extend_from_slice(&(VERSION_UTF16BE.len() as u32).to_be_bytes());
    out.extend_from_slice(&VERSION_UTF16BE);
    out.extend_from_slice(&PAYLOAD_LEN.to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn import(data: &[u8]) -> Result<Pattern, Td3Error> {
    // Magic
    if data.len() < 4 {
        return Err(Td3Error::FormatError(
            ".seq file too short for magic".to_string(),
        ));
    }
    if data[..4] != MAGIC {
        return Err(Td3Error::FormatError(
            ".seq file has wrong magic (expected 23 98 54 76)".to_string(),
        ));
    }
    let mut pos = 4;

    // Product name (length-prefixed UTF-16BE)
    let product_len = read_u32(data, pos, "product length")?;
    if product_len > MAX_STRING_BYTES {
        return Err(Td3Error::FormatError(format!(
            ".seq product name length unreasonable: {}",
            product_len
        )));
    }
    pos += 4 + product_len as usize;

    // Firmware version (length-prefixed UTF-16BE)
    if data.len() < pos + 4 {
        return Err(Td3Error::FormatError(
            ".seq truncated before version length".to_string(),
        ));
    }
    let version_len = read_u32(data, pos, "version length")?;
    if version_len > MAX_STRING_BYTES {
        return Err(Td3Error::FormatError(format!(
            ".seq version length unreasonable: {}",
            version_len
        )));
    }
    pos += 4 + version_len as usize;

    // Payload size
    let payload_len = read_u32(data, pos, "payload size")?;
    if payload_len != PAYLOAD_LEN {
        return Err(Td3Error::FormatError(format!(
            ".seq payload length mismatch: expected {}, got {}",
            PAYLOAD_LEN, payload_len
        )));
    }
    pos += 4;

    if data.len() < pos + payload_len as usize {
        return Err(Td3Error::FormatError(format!(
            ".seq file truncated: need {} payload bytes, got {}",
            payload_len,
            data.len().saturating_sub(pos)
        )));
    }

    let payload = &data[pos..pos + payload_len as usize];

    // Reconstruct a 115-byte SysEx payload so we can reuse sysex_to_pattern.
    // Device header bytes (message kind + patgroup + slot+side<<3) are
    // synthesized because a .seq file doesn't record them.
    let mut sysex = Vec::with_capacity(115);
    sysex.push(0x78);
    sysex.push(0x00);
    sysex.push(0x00);
    sysex.extend_from_slice(payload);
    sysex_to_pattern(&sysex)
}

fn read_u32(data: &[u8], pos: usize, field: &str) -> Result<u32, Td3Error> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| Td3Error::FormatError(format!(".seq {} field offset overflow", field)))?;
    if data.len() < end {
        return Err(Td3Error::FormatError(format!(
            ".seq truncated at {} field",
            field
        )));
    }
    let bytes = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
    Ok(u32::from_be_bytes(bytes))
}
