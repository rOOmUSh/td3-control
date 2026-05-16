//! Full-bank folder ↔ file I/O.
//!
//! Pure file-system operations - no MIDI, no device I/O.
//!
//! Two supported bank file formats:
//!   - `.sqs` (Behringer SynthTribe) - the primary canonical TD-3 format,
//!     with marker-byte preservation and a manifest at the folder root.
//!   - `.rbs` (Propellerhead ReBirth RB-338) - slave target, 64 patterns
//!     mapped A-side→Device 1, B-side→Device 2. No manifest, no markers.
//!
//! Public entry points:
//!   - [`extract_bank`] - `<bank file>` → folder of 64 × 7 formats.
//!   - [`pack_bank`] - folder → `<bank file>`.
//!
//! Both dispatch on the file extension to choose the right backend.

pub mod address;
pub mod backup;
pub mod diff;
pub mod extract;
pub mod import;
pub mod inventory;
pub mod manifest;
pub mod pack;
pub mod rbs_bank;

pub use inventory::{scan_backup_dir, BackupInventoryEntry};

use std::path::Path;

use crate::error::Td3Error;
use crate::formats::{detect_format, Format};

/// Extract a bank file (`.sqs` or `.rbs`) into a 64-subfolder tree.
pub fn extract_bank(
    input: &Path,
    output_dir: &Path,
    force: bool,
    midi_opts: &crate::formats::mid::MidiExportOptions,
) -> Result<(), Td3Error> {
    match detect_format_of(input)? {
        Format::Rbs => rbs_bank::extract_rbs_bank(input, output_dir, force, midi_opts),
        // SQS is the default - any unrecognised extension goes through the
        // SQS parser, which will report a clear magic-byte error.
        _ => extract::extract_bank(input, output_dir, force, midi_opts),
    }
}

/// Pack a 64-subfolder tree into a bank file (`.sqs` or `.rbs`).
pub fn pack_bank(
    input_dir: &Path,
    output: &Path,
    force: bool,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> Result<(), Td3Error> {
    match detect_format_of(output)? {
        Format::Rbs => rbs_bank::pack_rbs_bank(input_dir, output, force, midi_import_opts),
        _ => pack::pack_bank(input_dir, output, force, midi_import_opts),
    }
}

fn detect_format_of(path: &Path) -> Result<Format, Td3Error> {
    let name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        Td3Error::CliError(format!("cannot read filename from {}", path.display()))
    })?;
    // `.sqs` isn't in the `Format` enum - callers treat unknown extensions as
    // SQS for backwards compatibility, so map Option → SQS-default here.
    match detect_format(name) {
        Some(Format::Rbs) => Ok(Format::Rbs),
        _ => Ok(Format::Syx), // sentinel meaning "fall through to SQS backend"
    }
}
