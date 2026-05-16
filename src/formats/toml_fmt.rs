use crate::error::Td3Error;
use crate::pattern::Pattern;

use super::{PatternFile, FORMAT_TAG};

/// Export pattern to TOML string.
pub fn export(pattern: &Pattern) -> Result<String, Td3Error> {
    let pf = PatternFile::from_pattern(pattern);
    let s = toml::to_string_pretty(&pf)
        .map_err(|e| Td3Error::FormatError(format!("TOML serialization failed: {}", e)))?;
    Ok(s)
}

/// Import pattern from TOML string.
pub fn import(data: &str) -> Result<Pattern, Td3Error> {
    let pf: PatternFile = toml::from_str(data)
        .map_err(|e| Td3Error::FormatError(format!("TOML parse error: {}", e)))?;
    if pf.format != FORMAT_TAG {
        return Err(Td3Error::FormatError(format!(
            "unexpected format field: '{}' (expected '{}')",
            pf.format, FORMAT_TAG
        )));
    }
    if pf.format_version != super::FORMAT_VERSION {
        return Err(Td3Error::FormatError(format!(
            "unsupported format_version: {} (supported: {})",
            pf.format_version,
            super::FORMAT_VERSION
        )));
    }
    pf.to_pattern()
}
