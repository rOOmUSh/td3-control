//! `bpm=` header value: two-decimal text in 20.00 through 300.00, carried
//! internally as integer centi-BPM.

use crate::error::Td3Error;

pub(super) const MIN_CENTIBPM: u32 = 2_000;
pub(super) const MAX_CENTIBPM: u32 = 30_000;

/// Convert a configured integer BPM to validated StepDSL centi-BPM.
pub fn centibpm_from_integer_bpm(bpm: u32) -> Result<u32, Td3Error> {
    let centibpm = bpm.checked_mul(100).ok_or_else(|| {
        Td3Error::FormatError(format!("bpm is too large to convert to centibpm: {}", bpm))
    })?;
    validate_centibpm(centibpm)?;
    Ok(centibpm)
}

pub(super) fn parse_bpm_centibpm(raw: &str) -> Result<u32, String> {
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

pub(super) fn format_bpm_centibpm(centibpm: u32) -> Result<String, Td3Error> {
    validate_centibpm(centibpm)?;
    let whole = centibpm / 100;
    let fraction = centibpm % 100;
    Ok(match fraction {
        0 => whole.to_string(),
        value if value % 10 == 0 => format!("{}.{}", whole, value / 10),
        value => format!("{}.{:02}", whole, value),
    })
}

pub(super) fn validate_centibpm(centibpm: u32) -> Result<(), Td3Error> {
    if !(MIN_CENTIBPM..=MAX_CENTIBPM).contains(&centibpm) {
        return Err(Td3Error::FormatError(format!(
            "centibpm must be {}-{}, got {}",
            MIN_CENTIBPM, MAX_CENTIBPM, centibpm
        )));
    }
    Ok(())
}
