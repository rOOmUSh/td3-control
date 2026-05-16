pub mod json;
pub mod mid;
pub mod mid_import;
pub mod pat;
pub mod rbs;
pub mod rbs_codec;
pub mod seq;
pub mod sqs;
pub mod steps_txt;
pub mod syx;
pub mod toml_fmt;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

// ---------------------------------------------------------------------------
// Format enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Syx,
    Toml,
    StepsTxt,
    Json,
    Mid,
    Seq,
    Pat,
    /// Propellerhead ReBirth RB-338 song file (64-pattern bank-level format).
    /// In single-pattern contexts the pattern is placed at/read from slot A1.
    Rbs,
}

impl Format {
    pub fn extension(&self) -> &'static str {
        match self {
            Format::Syx => "syx",
            Format::Toml => "toml",
            Format::StepsTxt => "steps.txt",
            Format::Json => "json",
            Format::Mid => "mid",
            Format::Seq => "seq",
            Format::Pat => "pat",
            Format::Rbs => "rbs",
        }
    }

    /// Per-pattern formats that `extract-bank` emits into every subfolder.
    /// `.rbs` is intentionally excluded here because a bank extraction already
    /// has a single top-level `.rbs` file; emitting 64 single-pattern `.rbs`
    /// files alongside the per-slot `.seq/.syx/...` sidecars would be noise.
    /// The single-pattern CLI `export` path adds `.rbs` separately via
    /// `all_single_pattern()`.
    pub fn all() -> &'static [Format] {
        &[
            Format::Syx,
            Format::Toml,
            Format::StepsTxt,
            Format::Json,
            Format::Mid,
            Format::Seq,
            Format::Pat,
        ]
    }

    /// Per-pattern formats emitted when the CLI `export` command writes a
    /// package folder for a *single* pattern pulled from the device. This is
    /// `all()` plus `.rbs`: the `.rbs` file has that one pattern embedded at
    /// its real slot address (A-side → ReBirth Device 1, B-side → Device 2),
    /// so the export is ReBirth-importable at the original `G*P*` address.
    pub fn all_single_pattern() -> &'static [Format] {
        &[
            Format::Syx,
            Format::Toml,
            Format::StepsTxt,
            Format::Json,
            Format::Mid,
            Format::Seq,
            Format::Pat,
            Format::Rbs,
        ]
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::Syx => write!(f, "syx"),
            Format::Toml => write!(f, "toml"),
            Format::StepsTxt => write!(f, "steps"),
            Format::Json => write!(f, "json"),
            Format::Mid => write!(f, "mid"),
            Format::Seq => write!(f, "seq"),
            Format::Pat => write!(f, "pat"),
            Format::Rbs => write!(f, "rbs"),
        }
    }
}

impl FromStr for Format {
    type Err = Td3Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "syx" => Ok(Format::Syx),
            "toml" => Ok(Format::Toml),
            "steps" => Ok(Format::StepsTxt),
            "json" => Ok(Format::Json),
            "mid" => Ok(Format::Mid),
            "seq" => Ok(Format::Seq),
            "pat" => Ok(Format::Pat),
            "rbs" => Ok(Format::Rbs),
            _ => Err(Td3Error::FormatError(format!(
                "unknown format '{}' (valid: syx, toml, steps, json, mid, seq, pat, rbs)",
                s
            ))),
        }
    }
}

/// Detect format from a filename extension.
/// `.steps.txt` is checked before `.txt`.
pub fn detect_format(filename: &str) -> Option<Format> {
    let lower = filename.to_lowercase();
    if lower.ends_with(".steps.txt") {
        Some(Format::StepsTxt)
    } else if lower.ends_with(".syx") {
        Some(Format::Syx)
    } else if lower.ends_with(".toml") {
        Some(Format::Toml)
    } else if lower.ends_with(".json") {
        Some(Format::Json)
    } else if lower.ends_with(".mid") {
        Some(Format::Mid)
    } else if lower.ends_with(".seq") {
        Some(Format::Seq)
    } else if lower.ends_with(".pat") {
        Some(Format::Pat)
    } else if lower.ends_with(".rbs") {
        Some(Format::Rbs)
    } else {
        None
    }
}

/// Format a 1-indexed pattern address string like "G1-P4A".
pub fn format_address(patgroup: u8, slot: u8, side: u8) -> String {
    format!(
        "G{}-P{}{}",
        patgroup + 1,
        slot + 1,
        if side == 0 { "A" } else { "B" }
    )
}

// ---------------------------------------------------------------------------
// Note name helpers
// ---------------------------------------------------------------------------

pub const NOTE_NAMES: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C^",
];

pub fn note_name(note: u8) -> &'static str {
    NOTE_NAMES.get(note as usize).unwrap_or(&"??")
}

pub fn parse_note_name(name: &str) -> Result<u8, Td3Error> {
    NOTE_NAMES
        .iter()
        .position(|&n| n == name)
        .map(|i| i as u8)
        .ok_or_else(|| Td3Error::FormatError(format!("unknown note: '{}'", name)))
}

// ---------------------------------------------------------------------------
// Shared serde model for TOML and JSON formats
// ---------------------------------------------------------------------------

/// Current format version for TOML and JSON exports.
pub const FORMAT_VERSION: u32 = 1;
pub const FORMAT_TAG: &str = "td3-control";
pub const DEVICE_TAG: &str = "TD-3";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternFile {
    pub format: String,
    pub format_version: u32,
    pub device: String,
    pub active_steps: u8,
    pub triplet_time: bool,
    pub steps: Vec<StepEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepEntry {
    pub index: u8,
    pub note: String,
    pub transpose: String,
    pub accent: bool,
    pub slide: bool,
    pub time: String,
}

// ---------------------------------------------------------------------------
// PatternFile contract conversion
// ---------------------------------------------------------------------------

impl StepEntry {
    fn from_step(index: u8, step_data: &step::Step) -> Self {
        Self {
            index,
            note: note_name(step_data.note).to_string(),
            transpose: step_data.transpose.contract_name().to_string(),
            accent: step_data.accent.enabled(),
            slide: step_data.slide.enabled(),
            time: step_data.time.contract_name().to_string(),
        }
    }

    fn to_step(&self) -> Result<step::Step, Td3Error> {
        let note = parse_note_name(&self.note)?;
        let transpose = step::Transpose::from_contract(&self.transpose).map_err(|_| {
            Td3Error::FormatError(format!("invalid transpose: '{}'", self.transpose))
        })?;
        let time = step::Time::from_contract(&self.time)
            .map_err(|_| Td3Error::FormatError(format!("invalid time: '{}'", self.time)))?;

        Ok(step::Step::new(
            note,
            transpose,
            step::Accent::from_enabled(self.accent),
            step::Slide::from_enabled(self.slide),
            time,
        ))
    }
}

impl PatternFile {
    pub fn from_pattern(pattern: &Pattern) -> Self {
        let steps = (0..step::Step::COUNT)
            .map(|idx| StepEntry::from_step((idx + 1) as u8, &pattern.step[idx]))
            .collect();

        PatternFile {
            format: FORMAT_TAG.to_string(),
            format_version: FORMAT_VERSION,
            device: DEVICE_TAG.to_string(),
            active_steps: pattern.active_steps,
            triplet_time: pattern.triplet,
            steps,
        }
    }

    pub fn to_pattern(&self) -> Result<Pattern, Td3Error> {
        if self.steps.len() != step::Step::COUNT {
            return Err(Td3Error::FormatError(format!(
                "expected {} steps, got {}",
                step::Step::COUNT,
                self.steps.len()
            )));
        }
        let mut steps: [step::Step; 16] = Default::default();
        let mut seen = [false; step::Step::COUNT];

        for entry in &self.steps {
            let idx = entry.index as usize;
            if !(1..=step::Step::COUNT).contains(&idx) {
                return Err(Td3Error::FormatError(format!(
                    "step index out of range: {}",
                    idx
                )));
            }
            if seen[idx - 1] {
                return Err(Td3Error::FormatError(format!(
                    "duplicate step index: {}",
                    idx
                )));
            }
            seen[idx - 1] = true;
            steps[idx - 1] = entry.to_step()?;
        }

        Pattern::new(self.triplet_time, self.active_steps, steps)
    }
}
