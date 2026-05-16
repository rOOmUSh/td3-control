//! `.rbs` ↔ folder full-bank I/O (ReBirth RB-338 song files).
//!
//! ReBirth is treated as a slave target - unlike `.sqs`, writes skip all
//! backup/confirmation ceremony. Per-pattern sidecars
//! go into subfolders `G{n}P{m}{A|B}` with the 7 per-pattern formats.
//!
//! Slot mapping:
//!   * A-side (`G*P*A`) ↔ RBS **Device 1** (flat idx 0..31, group-major)
//!   * B-side (`G*P*B`) ↔ RBS **Device 2** (flat idx 32..63, group-major)
//!
//! Within each device, records are ordered `group * 8 + slot` (0-indexed),
//! so `rbs_idx = (side * 32) + (group * 8) + slot`.

use std::fs;
use std::path::Path;

use crate::error::Td3Error;
use crate::formats::mid::MidiExportOptions;
use crate::formats::rbs;
use crate::formats::{self as formats_mod, Format};
use crate::pattern::{pattern_to_sysex, Pattern};

pub const GROUPS: u8 = 4;
pub const SLOTS_PER_GROUP: u8 = 8;
pub const SIDES: u8 = 2;
pub const TOTAL: usize = (GROUPS * SLOTS_PER_GROUP * SIDES) as usize; // 64

/// Map a `(group, slot, side)` address to the flat RBS index.
/// Device 1 = A-side (side 0), Device 2 = B-side (side 1).
pub fn rbs_index(group: u8, slot: u8, side: u8) -> usize {
    (side as usize) * 32 + (group as usize) * 8 + (slot as usize)
}

/// Bank-folder name for an address: `G{group+1}P{slot+1}{A|B}`.
pub fn folder_name(group: u8, slot: u8, side: u8) -> String {
    format!(
        "G{}P{}{}",
        group + 1,
        slot + 1,
        if side == 0 { 'A' } else { 'B' }
    )
}

// ---------------------------------------------------------------------------
// Extract: .rbs → folder of 64 × 7-format subfolders
// ---------------------------------------------------------------------------

/// Parse an `.rbs` song and write the 64-folder tree to `output_dir`.
/// No manifest is written (RBS has no per-slot metadata worth preserving).
pub fn extract_rbs_bank(
    input: &Path,
    output_dir: &Path,
    force: bool,
    midi_opts: &MidiExportOptions,
) -> Result<(), Td3Error> {
    let data = fs::read(input)?;
    let patterns = rbs::import_bank(&data)?;
    if patterns.len() != TOTAL {
        return Err(Td3Error::FormatError(format!(
            ".rbs bank parse returned {} patterns (expected {})",
            patterns.len(),
            TOTAL
        )));
    }

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

    for side in 0..SIDES {
        for group in 0..GROUPS {
            for slot in 0..SLOTS_PER_GROUP {
                let idx = rbs_index(group, slot, side);
                let pat = &patterns[idx];
                let address = folder_name(group, slot, side);
                let folder = output_dir.join(&address);
                fs::create_dir_all(&folder)?;

                // Synthesize raw SysEx bytes (3-byte device header + 112-byte
                // payload) so `.syx` can be emitted from the Pattern. This
                // marks the slot_addr correctly (slot | (side << 3)).
                let slot_addr = slot | (side << 3);
                let raw_sysex = pattern_to_sysex(pat, group, slot, side)?;
                let _ = slot_addr; // slot_addr encoded inside raw_sysex byte 2

                for fmt in Format::all() {
                    let filename = folder.join(format!("{}.{}", address, fmt.extension()));
                    write_per_pattern(*fmt, pat, &raw_sysex, &filename, &address, midi_opts)?;
                }
            }
        }
    }

    Ok(())
}

fn write_per_pattern(
    fmt: Format,
    pattern: &Pattern,
    raw_sysex: &[u8],
    filename: &Path,
    address: &str,
    midi_opts: &MidiExportOptions,
) -> Result<(), Td3Error> {
    match fmt {
        Format::Syx => fs::write(filename, formats_mod::syx::export_raw(raw_sysex))?,
        Format::Toml => fs::write(filename, formats_mod::toml_fmt::export(pattern)?)?,
        Format::Json => fs::write(filename, formats_mod::json::export(pattern)?)?,
        Format::Mid => fs::write(
            filename,
            formats_mod::mid::export(pattern, address, midi_opts)?,
        )?,
        Format::StepsTxt => fs::write(filename, formats_mod::steps_txt::export(pattern))?,
        Format::Seq => fs::write(filename, formats_mod::seq::export(pattern)?)?,
        Format::Pat => fs::write(filename, formats_mod::pat::export(pattern))?,
        Format::Rbs => {
            return Err(Td3Error::FormatError(
                ".rbs is a bank-level format; not emitted per-slot".to_string(),
            ))
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pack: folder of 64 × per-pattern files → `.rbs`
// ---------------------------------------------------------------------------

/// Source-file preference order for reading a pattern from a subfolder -
/// matches `.sqs` pack order so folders round-trip between the two formats.
/// `.syx` wins because it preserves marker bytes, but for RBS we discard
/// markers anyway. `.seq` is preferred next as it's the closest native form.
const PREF_EXTS: &[&str] = &["syx", "seq", "toml", "json", "steps.txt", "mid", "pat"];

/// Read the 64 subfolders under `input_dir` and emit an `.rbs` at `output`.
pub fn pack_rbs_bank(
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

    // Up-front pass: collect missing folders for a single comprehensive error.
    let mut missing: Vec<String> = Vec::new();
    for side in 0..SIDES {
        for group in 0..GROUPS {
            for slot in 0..SLOTS_PER_GROUP {
                let addr = folder_name(group, slot, side);
                if !input_dir.join(&addr).is_dir() {
                    missing.push(addr);
                }
            }
        }
    }
    if !missing.is_empty() {
        return Err(Td3Error::BankFolderIncomplete(missing.join(", ")));
    }

    // Collect all 64 patterns in RBS flat-index order.
    let mut patterns: Vec<Pattern> = Vec::with_capacity(TOTAL);
    for side in 0..SIDES {
        for group in 0..GROUPS {
            for slot in 0..SLOTS_PER_GROUP {
                let address = folder_name(group, slot, side);
                let folder = input_dir.join(&address);
                let source = pick_source_file(&folder, &address).ok_or_else(|| {
                    Td3Error::CliError(format!(
                        "subfolder {} contains no readable pattern file (looked for {:?})",
                        address, PREF_EXTS
                    ))
                })?;
                let filename = source.to_string_lossy().into_owned();
                let pattern = crate::app::import_file(&filename, midi_import_opts)?;
                patterns.push(pattern);
            }
        }
    }

    let bytes = rbs::export_bank(patterns)?;
    fs::write(output, bytes)?;
    Ok(())
}

fn pick_source_file(folder: &Path, address: &str) -> Option<std::path::PathBuf> {
    for ext in PREF_EXTS {
        let candidate = folder.join(format!("{}.{}", address, ext));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
