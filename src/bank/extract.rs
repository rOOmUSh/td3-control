//! `.sqs` → folder of 64 subfolders × 6 formats + `bank_manifest.json`.
//!
//! Per-record flow:
//!   1. Synthesize a full 115-byte SysEx payload `[0x78, group, slot_addr] + record.payload`.
//!   2. `.syx` is written via `syx::export_raw` - preserves the exact 112 bytes
//!      from the bank (marker + pitch/accent/slide/footer). This is the only
//!      format that carries byte-identical round-trip through `pack-bank`.
//!   3. Other formats are derived from the decoded `Pattern`.
//!
//! The manifest at the folder root captures what the decoded `Pattern` can't:
//!   - UTF-16BE product/version bytes (from file header)
//!   - Per-record marker bytes (`00 00` vs `00 01`, see the marker-semantics
//!     memory).

use std::fs;
use std::path::Path;

use crate::bank::manifest::write_manifest;
use crate::error::Td3Error;
use crate::formats::mid::MidiExportOptions;
use crate::formats::sqs::{self, parse_bank, BankRecord};
use crate::formats::{self as formats_mod, Format};
use crate::pattern::{sysex_to_pattern, Pattern};

/// Parse `input` as a `.sqs` bank and write a folder tree at `output_dir`:
///
/// ```text
/// output_dir/
/// ├── bank_manifest.json
/// ├── G1P1A/
/// │   ├── G1P1A.syx
/// │   ├── G1P1A.toml
/// │   ├── G1P1A.steps.txt
/// │   ├── G1P1A.json
/// │   ├── G1P1A.mid
/// │   └── G1P1A.seq
/// ├── G1P2A/
/// │   └── …
/// …
/// └── G4P8B/…
/// ```
///
/// `force = false` refuses to proceed if `output_dir` already exists.
pub fn extract_bank(
    input: &Path,
    output_dir: &Path,
    force: bool,
    midi_opts: &MidiExportOptions,
) -> Result<(), Td3Error> {
    let data = fs::read(input)?;
    let bank = parse_bank(&data)?;

    if output_dir.exists() {
        if !force {
            return Err(Td3Error::CliError(format!(
                "output directory already exists: {} (use --force to overwrite)",
                output_dir.display()
            )));
        }
    } else {
        fs::create_dir_all(output_dir)?;
    }

    for rec in bank.records.iter() {
        write_record(output_dir, rec, midi_opts)?;
    }

    write_manifest(output_dir, &bank)?;
    Ok(())
}

/// Write all 6 exportable formats for one record into its subfolder.
fn write_record(
    output_dir: &Path,
    rec: &BankRecord,
    midi_opts: &MidiExportOptions,
) -> Result<(), Td3Error> {
    let address = sqs::folder_name(rec.group, rec.slot_addr);
    let folder = output_dir.join(&address);
    fs::create_dir_all(&folder)?;

    // Raw SysEx payload (115 bytes): 3-byte device header + original 112-byte
    // record payload. Marker bytes inside the payload are preserved.
    let mut raw_sysex = Vec::with_capacity(3 + rec.payload.len());
    raw_sysex.push(0x78);
    raw_sysex.push(rec.group);
    raw_sysex.push(rec.slot_addr);
    raw_sysex.extend_from_slice(&rec.payload);

    let pattern = sysex_to_pattern(&raw_sysex)?;

    for fmt in Format::all() {
        let filename = folder.join(format!("{}.{}", address, fmt.extension()));
        write_format(*fmt, &pattern, &raw_sysex, &filename, &address, midi_opts)?;
    }

    Ok(())
}

/// Dispatch one format write. Mirrors `app::write_format` but works on `&Path`
/// and does not print per-file progress (the bank-extract caller prints once).
fn write_format(
    fmt: Format,
    pattern: &Pattern,
    raw_sysex: &[u8],
    filename: &Path,
    address: &str,
    midi_opts: &MidiExportOptions,
) -> Result<(), Td3Error> {
    match fmt {
        Format::Syx => {
            let data = formats_mod::syx::export_raw(raw_sysex);
            fs::write(filename, data)?;
        }
        Format::Toml => {
            let data = formats_mod::toml_fmt::export(pattern)?;
            fs::write(filename, data)?;
        }
        Format::Json => {
            let data = formats_mod::json::export(pattern)?;
            fs::write(filename, data)?;
        }
        Format::Mid => {
            let data = formats_mod::mid::export(pattern, address, midi_opts)?;
            fs::write(filename, data)?;
        }
        Format::StepsTxt => {
            let data = formats_mod::steps_txt::export(pattern);
            fs::write(filename, data)?;
        }
        Format::Seq => {
            let data = formats_mod::seq::export(pattern)?;
            fs::write(filename, data)?;
        }
        Format::Pat => {
            let data = formats_mod::pat::export(pattern);
            fs::write(filename, data)?;
        }
        // `.rbs` is a bank-level format; never a per-slot sidecar. Guarded
        // here in case `Format::all()` ever starts including it by accident.
        Format::Rbs => {
            return Err(Td3Error::FormatError(
                ".rbs is a bank-level format; per-pattern extraction is not supported".to_string(),
            ))
        }
    }
    Ok(())
}
