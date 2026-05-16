use std::str::FromStr;

use crate::error::Td3Error;
use crate::formats::Format;

use super::model::PatternAddress;

/// Parse patgroup string to 0-indexed value.
pub(crate) fn parse_group(s: &str) -> Result<u8, Td3Error> {
    let group_value: u8 = s
        .parse()
        .map_err(|_| Td3Error::CliError(format!("invalid patgroup '{}' (expected 1-4)", s)))?;
    match group_value {
        1..=4 => Ok(group_value - 1),
        _ => Err(Td3Error::CliError(format!(
            "patgroup {} out of range (expected 1-4)",
            group_value
        ))),
    }
}

/// Parse pattern string like "1A" to (slot: 0-indexed, side: 0 or 1).
pub(crate) fn parse_pattern(s: &str) -> Result<(u8, u8), Td3Error> {
    if s.len() != 2 {
        return Err(Td3Error::CliError(format!(
            "pattern '{}' must be 2 characters: number (1-8) + letter (A/B)",
            s
        )));
    }
    let slot_number: u8 = s[0..1].parse().map_err(|_| {
        Td3Error::CliError(format!(
            "pattern must start with number 1-8, got '{}'",
            &s[0..1]
        ))
    })?;
    if !(1..=8).contains(&slot_number) {
        return Err(Td3Error::CliError(format!(
            "pattern number {} out of range (expected 1-8)",
            slot_number
        )));
    }
    let side_code = match &s[1..2] {
        "A" | "a" => 0u8,
        "B" | "b" => 1u8,
        other => {
            return Err(Td3Error::CliError(format!(
                "pattern must end with A or B, got '{}'",
                other
            )))
        }
    };
    Ok((slot_number - 1, side_code))
}

/// Parse a pattern slot address like "G1P1A" into PatternAddress.
///
/// Accepted formats: G1P1A, g1p1a, G1 P1A, G1-P1A (case-insensitive).
pub(crate) fn parse_pattern_address(s: &str) -> Result<PatternAddress, Td3Error> {
    let normalized = s.trim().to_uppercase().replace([' ', '-'], "");
    if normalized.len() < 4 || !normalized.starts_with('G') {
        return Err(Td3Error::CliError(format!(
            "invalid pattern address '{}' - expected format like G1P1A (G1-4, P1-8, A/B)",
            normalized
        )));
    }
    let after_g = &normalized[1..];
    let p_pos = after_g.find('P').ok_or_else(|| {
        Td3Error::CliError(format!(
            "invalid pattern address '{}' - missing P (expected format like G1P1A)",
            normalized
        ))
    })?;
    let patgroup_str = &after_g[..p_pos];
    let slot_str = &after_g[p_pos + 1..];
    let patgroup = parse_group(patgroup_str)?;
    let (slot, side) = parse_pattern(slot_str)?;
    Ok(PatternAddress {
        patgroup,
        slot,
        side,
    })
}

/// Parse comma-separated format list.
pub(crate) fn parse_formats(s: &str) -> Result<Vec<Format>, Td3Error> {
    let mut fmts = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if !part.is_empty() {
            fmts.push(Format::from_str(part)?);
        }
    }
    Ok(fmts)
}
