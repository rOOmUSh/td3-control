//! Step rows: `NN NOTE:TAS:TIME` followed by zero or more `|KEY:VALUE`
//! fields. The pattern part keeps the v1 rules exactly; the fields are
//! lenient: a numeric value out of range is clamped, a non-numeric value
//! is recorded as invalid, an unknown key is ignored.

use crate::error::Td3Error;
use crate::step;

use super::super::parse_note_name;

/// One optional per-row field as read from the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Absent,
    Invalid,
    Value(u8),
}

#[derive(Debug)]
pub(super) struct ParsedRow {
    /// 1-based step index.
    pub index: usize,
    pub step: step::Step,
    pub cutoff: Field,
    pub gate: Field,
}

fn parse_field(raw: &str, min: u8, max: u8) -> Field {
    match raw.trim().parse::<i64>() {
        Ok(value) => Field::Value(value.clamp(i64::from(min), i64::from(max)) as u8),
        Err(_) => Field::Invalid,
    }
}

pub(super) fn parse_row(line: &str, line_num: usize) -> Result<ParsedRow, Td3Error> {
    if line.len() < 10 {
        return Err(Td3Error::FormatError(format!(
            "line {}: step line too short: '{}'",
            line_num, line
        )));
    }

    let idx_text = line.get(..2).ok_or_else(|| {
        Td3Error::FormatError(format!("line {}: invalid step index encoding", line_num))
    })?;
    let index: usize = idx_text.trim().parse().map_err(|_| {
        Td3Error::FormatError(format!(
            "line {}: invalid step index '{}'",
            line_num, idx_text
        ))
    })?;
    if !(1..=step::Step::COUNT).contains(&index) {
        return Err(Td3Error::FormatError(format!(
            "line {}: step index out of range: {}",
            line_num, index
        )));
    }

    let body = line.get(3..).ok_or_else(|| {
        Td3Error::FormatError(format!("line {}: invalid step line encoding", line_num))
    })?;
    let mut segments = body.split('|');
    let pattern_part = segments.next().unwrap_or("");
    let parts: Vec<&str> = pattern_part.split(':').collect();
    if parts.len() != 3 {
        return Err(Td3Error::FormatError(format!(
            "line {}: expected NOTE:TAS:TIME, got '{}'",
            line_num, pattern_part
        )));
    }

    let note_text = parts[0].trim();
    let control_text = parts[1];
    let time_text = parts[2].trim();

    if control_text.len() != 3 {
        return Err(Td3Error::FormatError(format!(
            "line {}: TAS field must be 3 chars, got '{}'",
            line_num, control_text
        )));
    }

    let control = control_text.as_bytes();
    let transpose = step::Transpose::from_steps_symbol(control[0]).map_err(|_| {
        Td3Error::FormatError(format!(
            "line {}: invalid transpose '{}' (expected U/D/-)",
            line_num, control[0] as char
        ))
    })?;
    let accent = step::Accent::from_steps_symbol(control[1]).map_err(|_| {
        Td3Error::FormatError(format!(
            "line {}: invalid accent '{}' (expected A/-)",
            line_num, control[1] as char
        ))
    })?;
    let slide = step::Slide::from_steps_symbol(control[2]).map_err(|_| {
        Td3Error::FormatError(format!(
            "line {}: invalid slide '{}' (expected S/-)",
            line_num, control[2] as char
        ))
    })?;
    let time = step::Time::from_steps_token(time_text).map_err(|_| {
        Td3Error::FormatError(format!(
            "line {}: invalid time '{}' (expected N/T/R/TR)",
            line_num, time_text
        ))
    })?;

    let mut cutoff = Field::Absent;
    let mut gate = Field::Absent;
    for segment in segments {
        let Some((key, value)) = segment.split_once(':') else {
            continue;
        };
        match key.trim().to_ascii_uppercase().as_str() {
            "CO" => cutoff = parse_field(value, 0, 127),
            "GT" => gate = parse_field(value, 1, 100),
            _ => {}
        }
    }

    Ok(ParsedRow {
        index,
        step: step::Step::new(parse_note_name(note_text)?, transpose, accent, slide, time),
        cutoff,
        gate,
    })
}
