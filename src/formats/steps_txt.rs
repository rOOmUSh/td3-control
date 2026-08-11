use std::fmt::Write;

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

use super::{note_name, parse_note_name};

const STEPDSL_TAG: &str = "td3-stepdsl-v1";
const MIN_CENTIBPM: u32 = 2_000;
const MAX_CENTIBPM: u32 = 30_000;

/// A StepDSL document and its optional playback tempo metadata.
#[derive(Debug)]
pub struct StepsTxtDocument {
    pub pattern: Pattern,
    pub centibpm: Option<u32>,
}

/// Export pattern to the compact step DSL format.
#[allow(dead_code)] // Public compatibility helper; production saves include BPM.
pub fn export(pattern: &Pattern) -> String {
    render(pattern, None)
}

/// Export a pattern with canonical StepDSL BPM metadata.
pub fn export_with_bpm(pattern: &Pattern, centibpm: u32) -> Result<String, Td3Error> {
    pattern.validate()?;
    let bpm = format_bpm_centibpm(centibpm)?;
    Ok(render(pattern, Some(&bpm)))
}

/// Export using an integer BPM supplied by CLI and backend configuration.
pub fn export_with_integer_bpm(pattern: &Pattern, bpm: u32) -> Result<String, Td3Error> {
    let centibpm = centibpm_from_integer_bpm(bpm)?;
    export_with_bpm(pattern, centibpm)
}

/// Convert a configured integer BPM to validated StepDSL centi-BPM.
pub fn centibpm_from_integer_bpm(bpm: u32) -> Result<u32, Td3Error> {
    let centibpm = bpm.checked_mul(100).ok_or_else(|| {
        Td3Error::FormatError(format!("bpm is too large to convert to centibpm: {}", bpm))
    })?;
    validate_centibpm(centibpm)?;
    Ok(centibpm)
}

fn render(pattern: &Pattern, bpm: Option<&str>) -> String {
    let mut out = String::new();
    writeln!(&mut out, "format={STEPDSL_TAG}").ok();
    writeln!(&mut out, "active_steps={}", pattern.active_steps).ok();
    writeln!(
        &mut out,
        "triplet_time={}",
        if pattern.triplet { "on" } else { "off" }
    )
    .ok();
    if let Some(value) = bpm {
        writeln!(&mut out, "bpm={value}").ok();
    }
    writeln!(&mut out).ok();

    let row_count = usize::from(pattern.active_steps).min(step::Step::COUNT);
    for idx in 0..row_count {
        let current = &pattern.step[idx];
        writeln!(
            &mut out,
            "{:02} {:>2}:{}{}{}:{}",
            idx + 1,
            note_name(current.note),
            current.transpose.steps_symbol() as char,
            current.accent.steps_symbol() as char,
            current.slide.steps_symbol() as char,
            current.time.steps_token()
        )
        .ok();
    }

    writeln!(&mut out).ok();
    writeln!(&mut out, "# NOTE:TAS:TIME").ok();
    writeln!(&mut out, "# transpose: U|D|-").ok();
    writeln!(&mut out, "# accent: A|-").ok();
    writeln!(&mut out, "# slide: S|-").ok();
    writeln!(&mut out, "# time: N|T|R|TR").ok();

    out
}

/// Import pattern from the compact step DSL format.
pub fn import(data: &str) -> Result<Pattern, Td3Error> {
    Ok(import_document(data)?.pattern)
}

/// Import a StepDSL document, preserving optional playback tempo metadata.
pub fn import_document(data: &str) -> Result<StepsTxtDocument, Td3Error> {
    let mut active_steps: Option<u8> = None;
    let mut triplet: Option<bool> = None;
    let mut centibpm: Option<u32> = None;
    let mut steps: [step::Step; 16] = Default::default();
    let mut seen = [false; step::Step::COUNT];

    for (line_num, raw_line) in data.lines().enumerate() {
        // Only treat lines starting with '#' (after trimming) as comments.
        // Inline '#' is NOT a comment - note names like C# contain '#'.
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(val) = line.strip_prefix("format=") {
            if val.trim() != STEPDSL_TAG {
                return Err(Td3Error::FormatError(format!(
                    "line {}: unknown format '{}'",
                    line_num + 1,
                    val.trim()
                )));
            }
            continue;
        }
        if let Some(val) = line.strip_prefix("active_steps=") {
            let parsed = val.trim().parse().map_err(|_| {
                Td3Error::FormatError(format!(
                    "line {}: invalid active_steps '{}'",
                    line_num + 1,
                    val.trim()
                ))
            })?;
            active_steps = Some(parsed);
            continue;
        }
        if let Some(val) = line.strip_prefix("triplet_time=") {
            let parsed = match val.trim() {
                value if value.eq_ignore_ascii_case("on") => true,
                value if value.eq_ignore_ascii_case("off") => false,
                value => {
                    return Err(Td3Error::FormatError(format!(
                        "line {}: invalid triplet_time '{}' (expected on/off)",
                        line_num + 1,
                        value
                    )))
                }
            };
            triplet = Some(parsed);
            continue;
        }
        if let Some(val) = line.strip_prefix("bpm=") {
            if centibpm.is_some() {
                return Err(Td3Error::FormatError(format!(
                    "line {}: duplicate bpm field",
                    line_num + 1
                )));
            }
            if raw_line.trim_end() != raw_line {
                return Err(Td3Error::FormatError(format!(
                    "line {}: invalid bpm '{}'",
                    line_num + 1,
                    val
                )));
            }
            centibpm =
                Some(parse_bpm_centibpm(val).map_err(|err| {
                    Td3Error::FormatError(format!("line {}: {}", line_num + 1, err))
                })?);
            continue;
        }

        if line.len() < 10 {
            return Err(Td3Error::FormatError(format!(
                "line {}: step line too short: '{}'",
                line_num + 1,
                line
            )));
        }

        let idx_text = line.get(..2).ok_or_else(|| {
            Td3Error::FormatError(format!(
                "line {}: invalid step index encoding",
                line_num + 1
            ))
        })?;
        let step_index: usize = idx_text.trim().parse().map_err(|_| {
            Td3Error::FormatError(format!(
                "line {}: invalid step index '{}'",
                line_num + 1,
                idx_text
            ))
        })?;
        if !(1..=step::Step::COUNT).contains(&step_index) {
            return Err(Td3Error::FormatError(format!(
                "line {}: step index out of range: {}",
                line_num + 1,
                step_index
            )));
        }
        if seen[step_index - 1] {
            return Err(Td3Error::FormatError(format!(
                "line {}: duplicate step index: {}",
                line_num + 1,
                step_index
            )));
        }

        let body = line.get(3..).ok_or_else(|| {
            Td3Error::FormatError(format!("line {}: invalid step line encoding", line_num + 1))
        })?;
        let parts: Vec<&str> = body.split(':').collect();
        if parts.len() != 3 {
            return Err(Td3Error::FormatError(format!(
                "line {}: expected NOTE:TAS:TIME, got '{}'",
                line_num + 1,
                body
            )));
        }

        let note_text = parts[0].trim();
        let control_text = parts[1];
        let time_text = parts[2].trim();

        if control_text.len() != 3 {
            return Err(Td3Error::FormatError(format!(
                "line {}: TAS field must be 3 chars, got '{}'",
                line_num + 1,
                control_text
            )));
        }

        let control = control_text.as_bytes();
        let transpose = step::Transpose::from_steps_symbol(control[0]).map_err(|_| {
            Td3Error::FormatError(format!(
                "line {}: invalid transpose '{}' (expected U/D/-)",
                line_num + 1,
                control[0] as char
            ))
        })?;
        let accent = step::Accent::from_steps_symbol(control[1]).map_err(|_| {
            Td3Error::FormatError(format!(
                "line {}: invalid accent '{}' (expected A/-)",
                line_num + 1,
                control[1] as char
            ))
        })?;
        let slide = step::Slide::from_steps_symbol(control[2]).map_err(|_| {
            Td3Error::FormatError(format!(
                "line {}: invalid slide '{}' (expected S/-)",
                line_num + 1,
                control[2] as char
            ))
        })?;
        let time = step::Time::from_steps_token(time_text).map_err(|_| {
            Td3Error::FormatError(format!(
                "line {}: invalid time '{}' (expected N/T/R/TR)",
                line_num + 1,
                time_text
            ))
        })?;

        steps[step_index - 1] =
            step::Step::new(parse_note_name(note_text)?, transpose, accent, slide, time);
        seen[step_index - 1] = true;
    }

    let declared_active_steps = active_steps.unwrap_or(16);
    if !(1..=step::Step::COUNT as u8).contains(&declared_active_steps) {
        return Pattern::new(triplet.unwrap_or(false), declared_active_steps, steps)
            .map(|pattern| StepsTxtDocument { pattern, centibpm });
    }

    let active_range = usize::from(declared_active_steps);

    if seen[..active_range].iter().any(|present| !present) {
        let missing: Vec<u8> = seen
            .iter()
            .take(active_range)
            .enumerate()
            .filter_map(|(idx, present)| {
                if *present {
                    None
                } else {
                    Some((idx + 1) as u8)
                }
            })
            .collect();
        return Err(Td3Error::FormatError(format!(
            "missing steps: {:?}",
            missing
        )));
    }

    let pattern = Pattern::new(triplet.unwrap_or(false), declared_active_steps, steps)?;
    Ok(StepsTxtDocument { pattern, centibpm })
}

fn parse_bpm_centibpm(raw: &str) -> Result<u32, String> {
    let (whole_raw, fraction_raw) = match raw.split_once('.') {
        Some((whole, fraction)) => {
            if fraction.contains('.') {
                return Err(format!("invalid bpm '{}'", raw));
            }
            (whole, fraction)
        }
        None => (raw, ""),
    };

    if whole_raw.is_empty() || !whole_raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid bpm '{}'", raw));
    }
    if fraction_raw.len() > 2 || !fraction_raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid bpm '{}'", raw));
    }

    let whole = whole_raw
        .parse::<u32>()
        .map_err(|_| format!("invalid bpm '{}'", raw))?;
    let fraction = match fraction_raw.len() {
        0 => 0,
        1 => u32::from(fraction_raw.as_bytes()[0] - b'0') * 10,
        2 => fraction_raw
            .parse::<u32>()
            .map_err(|_| format!("invalid bpm '{}'", raw))?,
        _ => return Err(format!("invalid bpm '{}'", raw)),
    };
    let centibpm = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| format!("invalid bpm '{}'", raw))?;
    validate_centibpm(centibpm)
        .map_err(|_| format!("bpm '{}' is outside the supported range 20.00-300.00", raw))?;
    Ok(centibpm)
}

fn format_bpm_centibpm(centibpm: u32) -> Result<String, Td3Error> {
    validate_centibpm(centibpm)?;
    let whole = centibpm / 100;
    let fraction = centibpm % 100;
    Ok(match fraction {
        0 => whole.to_string(),
        value if value % 10 == 0 => format!("{}.{}", whole, value / 10),
        value => format!("{}.{:02}", whole, value),
    })
}

fn validate_centibpm(centibpm: u32) -> Result<(), Td3Error> {
    if !(MIN_CENTIBPM..=MAX_CENTIBPM).contains(&centibpm) {
        return Err(Td3Error::FormatError(format!(
            "centibpm must be {}-{}, got {}",
            MIN_CENTIBPM, MAX_CENTIBPM, centibpm
        )));
    }
    Ok(())
}
