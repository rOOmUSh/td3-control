//! Folder → `.sqs` packer.
//!
//! For each of the 64 expected subfolders, pick one source file in preference
//! order and produce a 112-byte bank-record payload:
//!
//! * `.syx` first - the only format that carries the original 112 bytes
//!   byte-for-byte (including the marker and any pitch/accent/slide residue
//!   from an on-device CLEAR). Read raw bytes, strip the sysex framing.
//! * Other formats → decode to `Pattern` → `pattern_to_sysex` → strip the
//!   3-byte device header. This drops junk bytes from CLEAR'd slots but
//!   preserves everything the device actually plays.
//!
//! Marker bytes (payload offsets 0..1) are overwritten from the manifest if
//! present; otherwise `pattern_to_sysex`'s hardcoded `00 01` stands for
//! encoder-derived payloads, and the original marker survives for `.syx`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::bank::manifest::{read_manifest, BankManifest};
use crate::error::Td3Error;
use crate::formats::sqs::{
    folder_name, serialize_bank, Bank, BankRecord, PAYLOAD_LEN, PRODUCT_UTF16BE, RECORD_COUNT,
    VERSION_UTF16BE,
};
use crate::pattern::pattern_to_sysex;

/// Source-file preference order for reading a pattern from a subfolder.
/// `.syx` wins because it round-trips byte-for-byte.
const PREF_EXTS: &[&str] = &["syx", "seq", "toml", "json", "steps.txt", "mid", "pat"];

/// Sysex framing bytes for a TD-3 pattern dump (same constants used by
/// `formats::syx` - duplicated here to avoid a cross-module pub).
const SYX_PRE: &[u8] = &[0xF0, 0x00, 0x20, 0x32, 0x00, 0x01, 0x0A];
const SYX_POST_LEN: usize = 1;

/// Read 64 subfolders under `input_dir` and emit a `.sqs` at `output`.
///
/// `force = false` refuses to overwrite an existing output file.
pub fn pack_bank(
    input_dir: &Path,
    output: &Path,
    force: bool,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> Result<(), Td3Error> {
    if output.exists() && !force {
        return Err(Td3Error::CliError(format!(
            "output file already exists: {} (use --force to overwrite)",
            output.display()
        )));
    }

    let manifest_opt = read_manifest(input_dir)?;
    if manifest_opt.is_none() {
        eprintln!(
            "warn: no {} at folder root - marker bytes will default to 00 01",
            crate::bank::manifest::MANIFEST_FILENAME
        );
    }

    // Up-front pass: collect every missing subfolder at once so the user can
    // fix them in one go instead of rerunning the command per missing slot.
    let mut missing: Vec<String> = Vec::new();
    for idx in 0..RECORD_COUNT {
        let group = (idx / 16) as u8;
        let slot_addr = (idx % 16) as u8;
        let addr = folder_name(group, slot_addr);
        if !input_dir.join(&addr).is_dir() {
            missing.push(addr);
        }
    }
    if !missing.is_empty() {
        return Err(Td3Error::BankFolderIncomplete(missing.join(", ")));
    }

    let mut records: Vec<BankRecord> = Vec::with_capacity(RECORD_COUNT);
    for idx in 0..RECORD_COUNT {
        let group = (idx / 16) as u8;
        let slot_addr = (idx % 16) as u8;
        let rec = build_record(
            input_dir,
            group,
            slot_addr,
            manifest_opt.as_ref(),
            midi_import_opts,
        )?;
        records.push(rec);
    }

    let records_arr: [BankRecord; RECORD_COUNT] =
        records.try_into().map_err(|_: Vec<BankRecord>| {
            Td3Error::FormatError("record collection size mismatch".to_string())
        })?;

    let (product_bytes, version_bytes) = header_bytes(manifest_opt.as_ref());

    let bank = Bank {
        product_bytes,
        version_bytes,
        records: records_arr,
    };

    let out_bytes = serialize_bank(&bank)?;
    fs::write(output, out_bytes)?;
    Ok(())
}

/// Build a single `BankRecord` by reading the subfolder `G{g}P{s}{A|B}`.
/// Returns a hard error if the subfolder is missing - `.sqs` bank files
/// always carry 64 records, so silently filling a gap would be a data-loss
/// risk.
fn build_record(
    input_dir: &Path,
    group: u8,
    slot_addr: u8,
    manifest: Option<&BankManifest>,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> Result<BankRecord, Td3Error> {
    let address = folder_name(group, slot_addr);
    let folder = input_dir.join(&address);
    // Existence already verified up-front in `pack_bank` - this is a defence-in-depth check.
    if !folder.is_dir() {
        return Err(Td3Error::BankFolderIncomplete(address));
    }

    let source = pick_source_file(&folder, &address).ok_or_else(|| {
        Td3Error::CliError(format!(
            "subfolder {} contains no readable pattern file (looked for {:?})",
            address, PREF_EXTS
        ))
    })?;

    let mut payload = load_payload(&source, group, slot_addr, midi_import_opts)?;
    apply_manifest_marker(&mut payload, &address, manifest);

    if payload.len() != PAYLOAD_LEN as usize {
        return Err(Td3Error::FormatError(format!(
            "record {}: payload length {} (expected {})",
            address,
            payload.len(),
            PAYLOAD_LEN
        )));
    }

    Ok(BankRecord {
        group,
        slot_addr,
        payload,
    })
}

/// Return the first matching source file for the address, using the
/// canonical preference order.
fn pick_source_file(folder: &Path, address: &str) -> Option<PathBuf> {
    for ext in PREF_EXTS {
        let candidate = folder.join(format!("{}.{}", address, ext));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Read a 112-byte bank-record payload from a source file. `.syx` is handled
/// as raw bytes (to preserve CLEAR-residue bytes); every other format goes
/// through the `Pattern` decode/encode path.
fn load_payload(
    source: &Path,
    group: u8,
    slot_addr: u8,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> Result<Vec<u8>, Td3Error> {
    let fname = source.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if fname.to_lowercase().ends_with(".syx") {
        return load_payload_from_syx(source);
    }

    // Fall through to the Pattern-based formats via `app::import_file`.
    let filename = source.to_string_lossy().into_owned();
    let pattern = crate::app::import_file(&filename, midi_import_opts)?;
    let sysex = pattern_to_sysex(&pattern, group, slot_addr & 0x7, slot_addr >> 3)?;
    if sysex.len() != 3 + PAYLOAD_LEN as usize {
        return Err(Td3Error::FormatError(format!(
            "pattern_to_sysex returned {} bytes (expected {})",
            sysex.len(),
            3 + PAYLOAD_LEN
        )));
    }
    Ok(sysex[3..].to_vec())
}

/// Parse a `.syx` file and return just the 112-byte bank-record payload
/// (raw - marker and junk bytes preserved).
fn load_payload_from_syx(source: &Path) -> Result<Vec<u8>, Td3Error> {
    let data = fs::read(source)?;
    let min_len = SYX_PRE.len() + 3 + PAYLOAD_LEN as usize + SYX_POST_LEN;
    if data.len() < min_len {
        return Err(Td3Error::FormatError(format!(
            ".syx file {} too short: {} bytes (minimum {})",
            source.display(),
            data.len(),
            min_len
        )));
    }
    if &data[..SYX_PRE.len()] != SYX_PRE {
        return Err(Td3Error::FormatError(format!(
            ".syx file {} has wrong header",
            source.display()
        )));
    }
    if data.last() != Some(&0xF7) {
        return Err(Td3Error::FormatError(format!(
            ".syx file {} missing F7 terminator",
            source.display()
        )));
    }

    // Between F0-prefix and F7 terminator we have 115 bytes:
    //   [0x78, group, slot_addr, ...112-byte payload]
    let after_pre = SYX_PRE.len();
    let before_post = data.len() - SYX_POST_LEN;
    let sysex_body = &data[after_pre..before_post];
    if sysex_body.len() != 3 + PAYLOAD_LEN as usize {
        return Err(Td3Error::FormatError(format!(
            ".syx file {} has unexpected body length {} (expected {})",
            source.display(),
            sysex_body.len(),
            3 + PAYLOAD_LEN
        )));
    }
    if sysex_body[0] != 0x78 {
        return Err(Td3Error::FormatError(format!(
            ".syx file {} has wrong message ID 0x{:02x} (expected 0x78)",
            source.display(),
            sysex_body[0]
        )));
    }

    Ok(sysex_body[3..].to_vec())
}

/// Overwrite marker bytes 0..1 of `payload` from the manifest, if the manifest
/// lists a marker for this address. Otherwise leave the encoder-derived bytes
/// in place (for Pattern-decoded sources) or the original bytes (for `.syx`).
fn apply_manifest_marker(payload: &mut [u8], address: &str, manifest: Option<&BankManifest>) {
    if let Some(m) = manifest {
        if let Some(marker) = m.marker_for(address) {
            payload[0] = marker[0];
            payload[1] = marker[1];
        }
    }
}

/// Resolve the header byte slices - from manifest if available, otherwise the
/// defined defaults.
fn header_bytes(manifest: Option<&BankManifest>) -> (Vec<u8>, Vec<u8>) {
    if let Some(m) = manifest {
        if let Some(pair) = m.header_bytes() {
            return pair;
        }
    }
    (PRODUCT_UTF16BE.to_vec(), VERSION_UTF16BE.to_vec())
}
