use std::fmt::Write;

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

use super::{note_name, parse_note_name};

const STEPDSL_TAG: &str = "td3-stepdsl-v1";

/// Export pattern to the compact step DSL format.
pub fn export(pattern: &Pattern) -> String {
    let mut out = String::new();
    writeln!(&mut out, "format={STEPDSL_TAG}").ok();
    writeln!(&mut out, "active_steps={}", pattern.active_steps).ok();
    writeln!(
        &mut out,
        "triplet_time={}",
        if pattern.triplet { "on" } else { "off" }
    )
    .ok();
    writeln!(&mut out).ok();

    for idx in 0..step::Step::COUNT {
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
    let mut active_steps: Option<u8> = None;
    let mut triplet: Option<bool> = None;
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

        if line.len() < 10 {
            return Err(Td3Error::FormatError(format!(
                "line {}: step line too short: '{}'",
                line_num + 1,
                line
            )));
        }

        let idx_text = &line[..2];
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

        let body = &line[3..];
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

    if seen.iter().any(|present| !present) {
        let missing: Vec<u8> = seen
            .iter()
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

    Pattern::new(triplet.unwrap_or(false), active_steps.unwrap_or(16), steps)
}
