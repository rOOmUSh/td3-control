use std::fs;

use crate::config::Config;
use crate::error::Td3Error;
use crate::formats;
use crate::pattern::Pattern;

use super::pattern_import::import_file;
use super::{required_input_path, required_output_path};

/// Convert an input pattern file to another format based on the output path
/// extension. No MIDI device is opened.
pub(super) fn run_convert(config: Config) -> Result<(), Td3Error> {
    let input = required_input_path(&config, "convert")?;
    let output = required_output_path(&config, "convert")?;

    let pattern = import_file(input, &config.midi_import_options())?;
    let fmt = formats::detect_format(output).ok_or_else(|| {
        Td3Error::FormatError(format!(
            "cannot detect output format from extension: '{}' (supported: .syx .toml .steps.txt .json .mid .seq .pat .rbs)",
            output
        ))
    })?;

    let address = formats::format_address(0, 0, 0);
    let midi_options = config.midi_export_options_for_pattern(&pattern)?;

    // .syx export needs a raw payload - synthesize one from the pattern since
    // we don't have a device dump here.
    let raw_payload = match fmt {
        formats::Format::Syx => crate::pattern::pattern_to_sysex(&pattern, 0, 0, 0)?,
        _ => Vec::new(),
    };

    write_format(fmt, &pattern, &raw_payload, output, &address, &midi_options)?;
    eprintln!("Converted {} -> {}", input, output);
    Ok(())
}

// ---------------------------------------------------------------------------
// File I/O workflows
// ---------------------------------------------------------------------------

/// Export a single file, detecting format from extension.
pub(super) fn export_single_file(
    pattern: &Pattern,
    raw_payload: &[u8],
    config: &Config,
    address: &str,
) -> Result<(), Td3Error> {
    let filename = required_output_path(config, "single-file export")?;
    let fmt = formats::detect_format(filename).unwrap_or(formats::Format::StepsTxt);
    let midi_options = config.midi_export_options_for_pattern(pattern)?;

    write_format(fmt, pattern, raw_payload, filename, address, &midi_options)?;
    eprintln!("Saved {} to {}", fmt, filename);
    Ok(())
}

/// Export a package folder with selected (or all) formats.
pub(super) fn export_package(
    pattern: &Pattern,
    raw_payload: &[u8],
    config: &Config,
    address: &str,
) -> Result<(), Td3Error> {
    let folder = format!("PATTERN_{}", address);
    fs::create_dir_all(&folder)?;
    let midi_options = config.midi_export_options_for_pattern(pattern)?;

    let fmts: &[formats::Format] = if config.render.requested_formats.is_empty() {
        formats::Format::all_single_pattern()
    } else {
        &config.render.requested_formats
    };

    for fmt in fmts {
        let filename = format!("{}/{}.{}", folder, address, fmt.extension());
        write_format(
            *fmt,
            pattern,
            raw_payload,
            &filename,
            address,
            &midi_options,
        )?;
        eprintln!("  {}", filename);
    }
    eprintln!("Package exported to {}/", folder);
    Ok(())
}

/// Write a pattern in the given format to a file.
fn write_format(
    fmt: formats::Format,
    pattern: &Pattern,
    raw_payload: &[u8],
    filename: &str,
    address: &str,
    midi_options: &formats::mid::MidiExportOptions,
) -> Result<(), Td3Error> {
    match fmt {
        formats::Format::Syx => {
            let data = formats::syx::export_raw(raw_payload);
            fs::write(filename, data)?;
        }
        formats::Format::Toml => {
            let data = formats::toml_fmt::export(pattern)?;
            fs::write(filename, data)?;
        }
        formats::Format::Json => {
            let data = formats::json::export(pattern)?;
            fs::write(filename, data)?;
        }
        formats::Format::Mid => {
            let data = formats::mid::export(pattern, address, midi_options)?;
            fs::write(filename, data)?;
        }
        formats::Format::StepsTxt => {
            let data = formats::steps_txt::export(pattern);
            fs::write(filename, data)?;
        }
        formats::Format::Seq => {
            let data = formats::seq::export(pattern)?;
            fs::write(filename, data)?;
        }
        formats::Format::Pat => {
            let data = formats::pat::export(pattern);
            fs::write(filename, data)?;
        }
        formats::Format::Rbs => {
            // Single-pattern `.rbs` export: build a fresh song from the
            // bundled template and place `pattern` at its original slot
            // address (A-side → Device 1, B-side → Device 2). Every other
            // slot remains silent so the export is ReBirth-importable at
            // the same G*P* address the user pulled it from.
            let (device, group, slot) = parse_address_to_rbs_slot(address)?;
            let owned = Pattern::new(pattern.triplet, pattern.active_steps, pattern.step)?;
            let data = formats::rbs::export_single_at(owned, device, group, slot)?;
            fs::write(filename, data)?;
        }
    }
    Ok(())
}

/// Parse a user pattern address like `"G1-P1A"` or `"G1P1A"` into the
/// `(device, group, slot)` tuple used by `rbs::export_single_at`. Accepts
/// either the dashed form (produced by `formats::format_address`) or the
/// undashed form (produced by `formats::sqs::folder_name`).
fn parse_address_to_rbs_slot(address: &str) -> Result<(usize, usize, usize), Td3Error> {
    let bytes = address.as_bytes();
    let invalid = || {
        Td3Error::FormatError(format!(
            "cannot parse pattern address '{}' (expected G<1-4>[-]P<1-8>[AB])",
            address
        ))
    };
    if bytes.len() < 5 {
        return Err(invalid());
    }
    if !matches!(bytes[0], b'G' | b'g') {
        return Err(invalid());
    }
    let group_num = (bytes[1] as char).to_digit(10).ok_or_else(invalid)? as usize;
    let mut idx = 2;
    if bytes[idx] == b'-' {
        idx += 1;
    }
    if idx >= bytes.len() || !matches!(bytes[idx], b'P' | b'p') {
        return Err(invalid());
    }
    idx += 1;
    if idx >= bytes.len() {
        return Err(invalid());
    }
    let slot_num = (bytes[idx] as char).to_digit(10).ok_or_else(invalid)? as usize;
    idx += 1;
    if idx >= bytes.len() {
        return Err(invalid());
    }
    let device = match bytes[idx] {
        b'A' | b'a' => 0,
        b'B' | b'b' => 1,
        _ => return Err(invalid()),
    };
    if !(1..=4).contains(&group_num) || !(1..=8).contains(&slot_num) {
        return Err(invalid());
    }
    Ok((device, group_num - 1, slot_num - 1))
}
