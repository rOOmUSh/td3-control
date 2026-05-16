//! Behringer SynthTribe `.sqs` full-bank file format (64 patterns).
//!
//! Layout (total 7966 bytes):
//! - 30-byte file header: magic `87 43 91 02`, UTF-16BE "TD-3" (len-prefixed),
//!   UTF-16BE "1.3.7" (len-prefixed).
//! - 64 pattern records of 124 bytes each:
//!   +00 BE u32 group       (0..3 → G1..G4)
//!   +04 BE u32 slot_addr   (0..15, = slot_num | (side << 3))
//!   +08 BE u32 payload_len (always 112)
//!   +0C 112   payload      (identical to .seq / SysEx-minus-header body)
//!
//! Each 112-byte payload byte 0..1 is the "unknown 1" marker (see
//! `project_td3_marker_byte_semantics` memory). The device ignores this on
//! upload; it is preserved only for byte-exact round-trip through
//! `bank_manifest.json`.

use std::convert::TryInto;

use crate::error::Td3Error;

// ---------------------------------------------------------------------------
// Format constants
// ---------------------------------------------------------------------------

/// `.sqs` file magic. Distinct from `.seq` (`23 98 54 76`).
pub const MAGIC: [u8; 4] = [0x87, 0x43, 0x91, 0x02];

/// UTF-16BE "TD-3" (product name).
pub const PRODUCT_UTF16BE: [u8; 8] = [0x00, 0x54, 0x00, 0x44, 0x00, 0x2D, 0x00, 0x33];

/// UTF-16BE "1.3.7" (firmware version as captured from reference file).
pub const VERSION_UTF16BE: [u8; 10] = [0x00, 0x31, 0x00, 0x2E, 0x00, 0x33, 0x00, 0x2E, 0x00, 0x37];

/// Bytes per record payload (constant across all TD-3 pattern dumps).
pub const PAYLOAD_LEN: u32 = 112;

/// Bytes per full record (12-byte record header + 112-byte payload).
pub const RECORD_LEN: usize = 12 + PAYLOAD_LEN as usize;

/// Number of records in a valid `.sqs` bank (always 64).
pub const RECORD_COUNT: usize = 64;

/// Fixed file header byte length (magic + 4 + 8 + 4 + 10 = 30).
pub const HEADER_LEN: usize = 30;

/// Total file size of a valid bank.
pub const FILE_LEN: usize = HEADER_LEN + RECORD_COUNT * RECORD_LEN;

/// Upper bound on header string lengths (defensive parsing).
const MAX_STRING_BYTES: u32 = 64;

// Offsets within the 112-byte payload (SysEx offsets minus 3-byte device header).
const PAYLOAD_REST_MASK_OFFSET: usize = 0x6C;
const PAYLOAD_MARKER_OFFSET: usize = 0x00;

/// REST mask value meaning "all 16 steps are REST". Used for semantic silent
/// detection - see `is_silent`.
const ALL_REST_MASK: [u8; 4] = [0x0F, 0x0F, 0x0F, 0x0F];

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// One parsed pattern record from a `.sqs` bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankRecord {
    /// 0..3 (G1..G4).
    pub group: u8,
    /// Raw slot_addr = slot_num | (side << 3). 0..15.
    pub slot_addr: u8,
    /// Full 112-byte payload, including marker bytes at offset 0..1.
    pub payload: Vec<u8>,
}

impl BankRecord {
    /// `slot_num` (0-indexed: P1..P8 → 0..7).
    #[allow(dead_code)] // part of the bank record accessor API
    pub fn slot_num(&self) -> u8 {
        self.slot_addr & 0x7
    }

    /// `side` (0 = A, 1 = B).
    #[allow(dead_code)] // part of the bank record accessor API
    pub fn side(&self) -> u8 {
        self.slot_addr >> 3
    }

    /// Origin marker bytes (payload offsets 0..1).
    pub fn marker(&self) -> [u8; 2] {
        [
            self.payload[PAYLOAD_MARKER_OFFSET],
            self.payload[PAYLOAD_MARKER_OFFSET + 1],
        ]
    }
}

/// Full parsed bank: the 30-byte header fields plus 64 records.
#[derive(Debug, Clone)]
pub struct Bank {
    /// Raw product-name bytes (UTF-16BE). Preserved for byte-exact round-trip.
    pub product_bytes: Vec<u8>,
    /// Raw firmware-version bytes (UTF-16BE). Preserved for byte-exact round-trip.
    pub version_bytes: Vec<u8>,
    /// 64 records in file order: (group=0..3) × (slot_addr=0..15).
    pub records: [BankRecord; RECORD_COUNT],
}

// ---------------------------------------------------------------------------
// Decode
// ---------------------------------------------------------------------------

/// Parse a `.sqs` byte slice into a `Bank`. All fields are strictly validated;
/// any malformed bytes produce `Td3Error::FormatError` with a descriptive message.
pub fn parse_bank(data: &[u8]) -> Result<Bank, Td3Error> {
    if data.len() != FILE_LEN {
        return Err(Td3Error::FormatError(format!(
            ".sqs file size mismatch: expected {} bytes, got {}",
            FILE_LEN,
            data.len()
        )));
    }

    if data[0..4] != MAGIC {
        return Err(Td3Error::FormatError(format!(
            ".sqs file has wrong magic (expected 87 43 91 02, got {:02x} {:02x} {:02x} {:02x})",
            data[0], data[1], data[2], data[3]
        )));
    }

    // Product name (length-prefixed UTF-16BE)
    let product_len = read_u32(data, 4, "product length")?;
    if product_len > MAX_STRING_BYTES {
        return Err(Td3Error::FormatError(format!(
            ".sqs product name length unreasonable: {}",
            product_len
        )));
    }
    let product_end = 8 + product_len as usize;
    if product_end > HEADER_LEN {
        return Err(Td3Error::FormatError(format!(
            ".sqs product name overflows header (end={}, header_len={})",
            product_end, HEADER_LEN
        )));
    }
    let product_bytes = data[8..product_end].to_vec();

    // Firmware version (length-prefixed UTF-16BE)
    let version_len = read_u32(data, product_end, "version length")?;
    if version_len > MAX_STRING_BYTES {
        return Err(Td3Error::FormatError(format!(
            ".sqs version length unreasonable: {}",
            version_len
        )));
    }
    let version_end = product_end + 4 + version_len as usize;
    if version_end != HEADER_LEN {
        return Err(Td3Error::FormatError(format!(
            ".sqs header layout mismatch: version ends at {}, expected {}",
            version_end, HEADER_LEN
        )));
    }
    let version_bytes = data[product_end + 4..version_end].to_vec();

    // 64 records.
    let records: [BankRecord; RECORD_COUNT] = parse_records(data)?;

    Ok(Bank {
        product_bytes,
        version_bytes,
        records,
    })
}

fn parse_records(data: &[u8]) -> Result<[BankRecord; RECORD_COUNT], Td3Error> {
    let mut parsed: Vec<BankRecord> = Vec::with_capacity(RECORD_COUNT);

    for idx in 0..RECORD_COUNT {
        let off = HEADER_LEN + idx * RECORD_LEN;
        let rec = &data[off..off + RECORD_LEN];

        let group = read_u32(rec, 0, "record group")?;
        let slot_addr = read_u32(rec, 4, "record slot_addr")?;
        let payload_len = read_u32(rec, 8, "record payload_len")?;

        if payload_len != PAYLOAD_LEN {
            return Err(Td3Error::FormatError(format!(
                ".sqs record {} has wrong payload_len: expected {}, got {}",
                idx, PAYLOAD_LEN, payload_len
            )));
        }
        if group > 3 {
            return Err(Td3Error::FormatError(format!(
                ".sqs record {} has group out of range: {} (must be 0..=3)",
                idx, group
            )));
        }
        if slot_addr > 15 {
            return Err(Td3Error::FormatError(format!(
                ".sqs record {} has slot_addr out of range: {} (must be 0..=15)",
                idx, slot_addr
            )));
        }

        // File record order is always (group=0..3) × (slot_addr=0..15).
        // Validate the positional invariant so we catch reordered or duplicated records.
        let expected_group = (idx / 16) as u32;
        let expected_slot = (idx % 16) as u32;
        if group != expected_group || slot_addr != expected_slot {
            return Err(Td3Error::FormatError(format!(
                ".sqs record {} out of order: expected group={} slot_addr={}, got group={} slot_addr={}",
                idx, expected_group, expected_slot, group, slot_addr
            )));
        }

        parsed.push(BankRecord {
            group: group as u8,
            slot_addr: slot_addr as u8,
            payload: rec[12..12 + PAYLOAD_LEN as usize].to_vec(),
        });
    }

    parsed.try_into().map_err(|_: Vec<BankRecord>| {
        Td3Error::FormatError(".sqs record collection size mismatch".to_string())
    })
}

fn read_u32(data: &[u8], pos: usize, field: &str) -> Result<u32, Td3Error> {
    if data.len() < pos + 4 {
        return Err(Td3Error::FormatError(format!(
            ".sqs truncated at {} field (offset {})",
            field, pos
        )));
    }
    let bytes: [u8; 4] = data[pos..pos + 4]
        .try_into()
        .map_err(|_| Td3Error::FormatError(format!(".sqs internal slice error at {}", field)))?;
    Ok(u32::from_be_bytes(bytes))
}

// ---------------------------------------------------------------------------
// Encode
// ---------------------------------------------------------------------------

/// Serialize a `Bank` back to 7966 bytes. Round-trips byte-exact with `parse_bank`
/// when the bank was parsed from an on-disk file.
pub fn serialize_bank(bank: &Bank) -> Result<Vec<u8>, Td3Error> {
    let mut out = Vec::with_capacity(FILE_LEN);

    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&(bank.product_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(&bank.product_bytes);
    out.extend_from_slice(&(bank.version_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(&bank.version_bytes);

    if out.len() != HEADER_LEN {
        return Err(Td3Error::FormatError(format!(
            ".sqs serialize produced non-standard header length {} (expected {})",
            out.len(),
            HEADER_LEN
        )));
    }

    for (idx, rec) in bank.records.iter().enumerate() {
        if rec.payload.len() != PAYLOAD_LEN as usize {
            return Err(Td3Error::FormatError(format!(
                ".sqs record {} has wrong payload length: expected {}, got {}",
                idx,
                PAYLOAD_LEN,
                rec.payload.len()
            )));
        }
        if rec.group > 3 {
            return Err(Td3Error::FormatError(format!(
                ".sqs record {}: group {} out of range",
                idx, rec.group
            )));
        }
        if rec.slot_addr > 15 {
            return Err(Td3Error::FormatError(format!(
                ".sqs record {}: slot_addr {} out of range",
                idx, rec.slot_addr
            )));
        }

        out.extend_from_slice(&(rec.group as u32).to_be_bytes());
        out.extend_from_slice(&(rec.slot_addr as u32).to_be_bytes());
        out.extend_from_slice(&PAYLOAD_LEN.to_be_bytes());
        out.extend_from_slice(&rec.payload);
    }

    if out.len() != FILE_LEN {
        return Err(Td3Error::FormatError(format!(
            ".sqs serialize produced non-standard file length {} (expected {})",
            out.len(),
            FILE_LEN
        )));
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Silent detection (semantic, not byte-exact)
// ---------------------------------------------------------------------------

/// Returns true if a 112-byte payload encodes an all-REST sequence.
///
/// The device plays a slot silent iff every one of the 16 steps is REST,
/// which corresponds to the 4-byte REST-mask at payload offset 0x6C being
/// `0F 0F 0F 0F` (each nibble stores 4 of the 16 rest bits).
///
/// This is intentionally NOT a byte-exact comparison against any reference.
/// On-device hardware CLEAR leaves residual bytes in the pitch/accent/slide
/// tables - the device ignores them when all steps are REST, so the slot is
/// audibly silent even though its payload is not byte-identical to any
/// factory-clean template. See `project_td3_marker_byte_semantics`.
pub fn is_silent(payload: &[u8]) -> bool {
    payload.len() >= PAYLOAD_REST_MASK_OFFSET + 4
        && payload[PAYLOAD_REST_MASK_OFFSET..PAYLOAD_REST_MASK_OFFSET + 4] == ALL_REST_MASK
}

// ---------------------------------------------------------------------------
// Address helpers
// ---------------------------------------------------------------------------

/// Bank-folder name for a record: `G{group+1}P{slot_num+1}{A|B}` (no dash).
/// Example: `BankRecord { group: 0, slot_addr: 6 }` → `"G1P7A"`.
pub fn folder_name(group: u8, slot_addr: u8) -> String {
    let slot_num = slot_addr & 0x7;
    let side = slot_addr >> 3;
    format!(
        "G{}P{}{}",
        group + 1,
        slot_num + 1,
        if side == 0 { 'A' } else { 'B' }
    )
}
