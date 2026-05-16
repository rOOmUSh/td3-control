use std::fs;
use std::io::{self, BufRead, Write};

use crate::error::Td3Error;
use crate::formats;
use crate::pattern::Pattern;

pub fn import_file(
    filename: &str,
    midi_import_opts: &formats::mid_import::MidiImportOptions,
) -> Result<Pattern, Td3Error> {
    let fmt = formats::detect_format(filename).ok_or_else(|| {
        Td3Error::FormatError(format!(
            "cannot detect format from extension: '{}' (supported: .syx .toml .steps.txt .json .mid .seq .pat .rbs)",
            filename
        ))
    })?;

    match fmt {
        formats::Format::Syx => {
            let data = fs::read(filename)?;
            formats::syx::import(&data)
        }
        formats::Format::Toml => {
            let data = fs::read_to_string(filename)?;
            formats::toml_fmt::import(&data)
        }
        formats::Format::Json => {
            let data = fs::read_to_string(filename)?;
            formats::json::import(&data)
        }
        formats::Format::Mid => {
            let data = fs::read(filename)?;
            let mut resolver = StdinPolyphonyResolver;
            formats::mid_import::import(&data, midi_import_opts, &mut resolver)
        }
        formats::Format::StepsTxt => {
            let data = fs::read_to_string(filename)?;
            formats::steps_txt::import(&data)
        }
        formats::Format::Seq => {
            let data = fs::read(filename)?;
            formats::seq::import(&data)
        }
        formats::Format::Pat => {
            let data = fs::read_to_string(filename)?;
            formats::pat::import(&data)
        }
        formats::Format::Rbs => {
            // Single-pattern `.rbs` import: return Device 1 / Group A / Slot 1
            // (the user's primary slot) and discard the other 63 patterns.
            // Use `bank::convert_rbs_to_folder` for full-bank extraction.
            let data = fs::read(filename)?;
            formats::rbs::import_single(&data, 0, 0, 0)
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive polyphony resolver for CLI import
// ---------------------------------------------------------------------------

/// Prompts the user on stdin/stdout to pick one pitch per polyphonic step.
/// The TD-3 is monophonic, so the only way to preserve user intent when a
/// DAW file carries chords is to ask which note to keep.
struct StdinPolyphonyResolver;

impl formats::mid_import::PolyphonyResolver for StdinPolyphonyResolver {
    fn choose(
        &mut self,
        step_index: usize,
        candidates: &[formats::mid_import::PolyphonyCandidate],
    ) -> Result<usize, Td3Error> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            eprintln!(
                "Polyphony detected. What note to keep? STEP {}",
                step_index + 1
            );
            eprint!("  Choose note to keep: ");
            for (i, c) in candidates.iter().enumerate() {
                eprint!("{}.{} ", i + 1, midi_pitch_name(c.midi_pitch));
            }
            eprintln!();
            eprint!("Enter number (1..={}): ", candidates.len());
            stdout.flush().ok();

            let mut line = String::new();
            let n = stdin
                .lock()
                .read_line(&mut line)
                .map_err(|e| Td3Error::FormatError(format!("stdin read failed: {}", e)))?;
            if n == 0 {
                return Err(Td3Error::FormatError(
                    "stdin closed before polyphony choice was made".to_string(),
                ));
            }
            match line.trim().parse::<usize>() {
                Ok(v) if (1..=candidates.len()).contains(&v) => return Ok(v - 1),
                _ => {
                    eprintln!("Invalid choice - enter 1..={}.", candidates.len());
                }
            }
        }
    }
}

/// Render a MIDI pitch as "C4", "F#3", etc. - just for the prompt label.
fn midi_pitch_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES[(pitch as usize) % 12];
    let octave = (pitch as i32) / 12 - 1;
    format!("{}{}", name, octave)
}
