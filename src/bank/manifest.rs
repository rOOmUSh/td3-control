//! `bank_manifest.json` reader/writer.
//!
//! The manifest captures the fields that the decoded `Pattern` model does
//! NOT preserve, so that a folder → `.sqs` → device round-trip can be made
//! byte-identical to the original `.sqs`.
//!
//! The manifest is **optional**: `pack-bank` warns and falls back to the
//! canonical marker bytes (`00 01`) for missing records and for missing
//! headers.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Td3Error;
use crate::formats::sqs::{self, Bank};

/// On-disk name of the manifest, at the extract-folder root.
pub const MANIFEST_FILENAME: &str = "bank_manifest.json";

/// JSON schema version - bump if the manifest shape ever changes.
const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BankManifest {
    pub format: String,
    pub format_version: u32,
    /// Hex string of the raw UTF-16BE product-name bytes from the `.sqs` header.
    pub product_bytes_hex: String,
    /// Hex string of the raw UTF-16BE firmware-version bytes.
    pub version_bytes_hex: String,
    pub records: Vec<RecordMeta>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordMeta {
    /// Folder name, e.g. `G1P1A`.
    pub address: String,
    pub group: u8,
    pub slot_addr: u8,
    /// Two-byte marker as hex, e.g. `"0001"`.
    pub marker_hex: String,
}

impl BankManifest {
    pub fn from_bank(bank: &Bank) -> Self {
        let records = bank
            .records
            .iter()
            .map(|rec| RecordMeta {
                address: sqs::folder_name(rec.group, rec.slot_addr),
                group: rec.group,
                slot_addr: rec.slot_addr,
                marker_hex: hex_encode(&rec.marker()),
            })
            .collect();
        BankManifest {
            format: "td3-bank-manifest".to_string(),
            format_version: MANIFEST_VERSION,
            product_bytes_hex: hex_encode(&bank.product_bytes),
            version_bytes_hex: hex_encode(&bank.version_bytes),
            records,
        }
    }

    /// Return the marker for a given folder address, or `None` if the
    /// manifest doesn't list it. Folder-name comparison is exact (case-sensitive).
    pub fn marker_for(&self, address: &str) -> Option<[u8; 2]> {
        self.records
            .iter()
            .find(|r| r.address == address)
            .and_then(|r| decode_marker(&r.marker_hex).ok())
    }

    /// Decode product/version hex to raw bytes. Returns `None` if either
    /// field is unparseable.
    pub fn header_bytes(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        let p = hex_decode(&self.product_bytes_hex).ok()?;
        let v = hex_decode(&self.version_bytes_hex).ok()?;
        Some((p, v))
    }
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

pub fn write_manifest(dir: &Path, bank: &Bank) -> Result<(), Td3Error> {
    let manifest = BankManifest::from_bank(bank);
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Td3Error::FormatError(format!("manifest serialization failed: {}", e)))?;
    let path = dir.join(MANIFEST_FILENAME);
    fs::write(&path, json)?;
    Ok(())
}

/// Read `bank_manifest.json` from `dir`. Returns `Ok(None)` if the file does
/// not exist (allowed - pack-bank will use defaults). Returns `Err` only if
/// the file exists but is unparseable.
pub fn read_manifest(dir: &Path) -> Result<Option<BankManifest>, Td3Error> {
    let path = dir.join(MANIFEST_FILENAME);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let manifest: BankManifest = serde_json::from_str(&text)
        .map_err(|e| Td3Error::FormatError(format!("{} parse failed: {}", MANIFEST_FILENAME, e)))?;
    Ok(Some(manifest))
}

// ---------------------------------------------------------------------------
// Hex helpers (no extra crate dep - simple enough inline)
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>, Td3Error> {
    if !s.len().is_multiple_of(2) {
        return Err(Td3Error::FormatError(format!(
            "hex string has odd length: {}",
            s.len()
        )));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(c: u8) -> Result<u8, Td3Error> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(Td3Error::FormatError(format!(
            "invalid hex character: 0x{:02x}",
            c
        ))),
    }
}

fn decode_marker(s: &str) -> Result<[u8; 2], Td3Error> {
    let bytes = hex_decode(s)?;
    if bytes.len() != 2 {
        return Err(Td3Error::FormatError(format!(
            "marker hex must encode 2 bytes, got {}",
            bytes.len()
        )));
    }
    Ok([bytes[0], bytes[1]])
}
