use crate::error::Td3Error;
use crate::formats::mid::MidiSlideMode;

pub(super) fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

pub(super) fn parse_bool(key: &str, value: &str) -> Result<bool, Td3Error> {
    match value.trim() {
        "0" | "false" | "FALSE" | "no" | "NO" => Ok(false),
        "1" | "true" | "TRUE" | "yes" | "YES" => Ok(true),
        other => Err(config_value_error(
            key,
            "must be 0/1 (or true/false)",
            other,
        )),
    }
}

pub(crate) fn parse_u64(key: &str, value: &str) -> Result<u64, Td3Error> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| config_value_error(key, "must be a non-negative integer", value))
}

pub(super) fn parse_u32(key: &str, value: &str) -> Result<u32, Td3Error> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| config_value_error(key, "must be a non-negative integer (u32)", value))
}

pub(crate) fn parse_u16(key: &str, value: &str) -> Result<u16, Td3Error> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| config_value_error(key, "must be a non-negative integer in 0..=65535", value))
}

pub(crate) fn parse_u8_range(key: &str, value: &str, min: u8, max: u8) -> Result<u8, Td3Error> {
    let n: u8 = value.trim().parse().map_err(|_| {
        let mut requirement = String::from("must be an integer in ");
        requirement.push_str(&min.to_string());
        requirement.push_str("..=");
        requirement.push_str(&max.to_string());
        config_value_error(key, &requirement, value)
    })?;
    if n < min || n > max {
        return Err(config_range_error(key, &n.to_string(), min, max));
    }
    Ok(n)
}

pub(super) fn parse_u32_range(key: &str, value: &str, min: u32, max: u32) -> Result<u32, Td3Error> {
    let n = parse_u32(key, value)?;
    if n < min || n > max {
        return Err(config_range_error(key, &n.to_string(), min, max));
    }
    Ok(n)
}

pub(crate) fn parse_i8(key: &str, value: &str) -> Result<i8, Td3Error> {
    value
        .trim()
        .parse::<i8>()
        .map_err(|_| config_value_error(key, "must be an integer in -128..=127", value))
}

pub(super) fn parse_slide_mode(value: &str) -> Result<MidiSlideMode, Td3Error> {
    let v = strip_quotes(value.trim());
    match v.to_lowercase().as_str() {
        "td3" => Ok(MidiSlideMode::Td3),
        "generic" => Ok(MidiSlideMode::Generic),
        "none" => Ok(MidiSlideMode::None),
        _ => Err(cfg_err(format!(
            "TD3_CONFIG.env 'MIDI_EXPORT_SLIDE_MODE' must be td3|generic|none, got '{}'",
            value
        ))),
    }
}

pub(super) fn cfg_err(msg: String) -> Td3Error {
    Td3Error::CliError(msg)
}

fn config_value_error(key: &str, requirement: &str, value: &str) -> Td3Error {
    let mut message = String::from("TD3_CONFIG.env '");
    message.push_str(key);
    message.push_str("' ");
    message.push_str(requirement);
    message.push_str(", got '");
    message.push_str(value);
    message.push('\'');
    cfg_err(message)
}

fn config_range_error<T: std::fmt::Display>(key: &str, value: &str, min: T, max: T) -> Td3Error {
    let mut message = String::from("TD3_CONFIG.env '");
    message.push_str(key);
    message.push_str("' = ");
    message.push_str(value);
    message.push_str(" out of range (expected ");
    message.push_str(&min.to_string());
    message.push_str("..=");
    message.push_str(&max.to_string());
    message.push(')');
    cfg_err(message)
}
