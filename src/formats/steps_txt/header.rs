//! Header lines (`key=value`).
//!
//! v1 keys keep their strict rules. The v1.1 keys are lenient: an
//! unusable value leaves the field absent, and under the v1.1 tag any
//! unrecognised `key=value` line is ignored. Under the v1 tag an
//! unrecognised line falls through to the row parser, which rejects it
//! as before.

use crate::error::Td3Error;

use super::bpm::parse_bpm_centibpm;
use super::{STEPDSL_TAG_V1, STEPDSL_TAG_V1_1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Version {
    V1,
    V1_1,
}

#[derive(Debug, Default)]
pub(super) struct Header {
    pub version: Option<Version>,
    pub active_steps: Option<u8>,
    pub triplet: Option<bool>,
    pub centibpm: Option<u32>,
    pub triplet_morph: Option<bool>,
    pub triplet_morph_percent: Option<u8>,
    pub live_update: Option<bool>,
    pub cutoff_lane_on: Option<bool>,
    pub gate_lane_on: Option<bool>,
}

fn parse_on_off(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("on") {
        Some(true)
    } else if value.eq_ignore_ascii_case("off") {
        Some(false)
    } else {
        None
    }
}

/// True when `line` has the shape of a header line: a letter first, then
/// a `key=` before any `:`. Row lines start with a digit.
fn looks_like_header(line: &str) -> bool {
    let first_letter = line.bytes().next().is_some_and(|b| b.is_ascii_alphabetic());
    let eq = line.find('=');
    let colon = line.find(':');
    first_letter && eq.is_some() && colon.is_none_or(|c| eq.is_some_and(|e| e < c))
}

impl Header {
    /// Consume `line` when it is a header line. Returns `Ok(false)` when
    /// the line is not a header line and must be parsed as a row.
    pub(super) fn apply(
        &mut self,
        raw_line: &str,
        line: &str,
        line_num: usize,
    ) -> Result<bool, Td3Error> {
        let Some((key, val)) = line.split_once('=') else {
            return Ok(false);
        };
        if !looks_like_header(line) {
            return Ok(false);
        }
        let value = val.trim();
        match key.trim() {
            "format" => {
                self.version = Some(match value {
                    v if v == STEPDSL_TAG_V1 => Version::V1,
                    v if v == STEPDSL_TAG_V1_1 => Version::V1_1,
                    other => {
                        return Err(Td3Error::FormatError(format!(
                            "line {}: unknown format '{}'",
                            line_num, other
                        )))
                    }
                });
            }
            "active_steps" => {
                let parsed = value.parse().map_err(|_| {
                    Td3Error::FormatError(format!(
                        "line {}: invalid active_steps '{}'",
                        line_num, value
                    ))
                })?;
                self.active_steps = Some(parsed);
            }
            "triplet_time" => {
                self.triplet = Some(parse_on_off(value).ok_or_else(|| {
                    Td3Error::FormatError(format!(
                        "line {}: invalid triplet_time '{}' (expected on/off)",
                        line_num, value
                    ))
                })?);
            }
            "bpm" => {
                if self.centibpm.is_some() {
                    return Err(Td3Error::FormatError(format!(
                        "line {}: duplicate bpm field",
                        line_num
                    )));
                }
                if raw_line.trim_end() != raw_line {
                    return Err(Td3Error::FormatError(format!(
                        "line {}: invalid bpm '{}'",
                        line_num, val
                    )));
                }
                self.centibpm =
                    Some(parse_bpm_centibpm(val).map_err(|err| {
                        Td3Error::FormatError(format!("line {}: {}", line_num, err))
                    })?);
            }
            "triplet_morph" => self.triplet_morph = parse_on_off(value),
            "triplet_morph_percentage" => {
                self.triplet_morph_percent = value.parse::<u32>().ok().map(|v| v.min(100) as u8);
            }
            "live_update" => self.live_update = parse_on_off(value),
            "pattern_co_lane" => self.cutoff_lane_on = parse_on_off(value),
            "pattern_gt_lane" => self.gate_lane_on = parse_on_off(value),
            _ => {
                // Unknown key: ignored under v1.1, a row-parse error under v1.
                if self.version != Some(Version::V1_1) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
