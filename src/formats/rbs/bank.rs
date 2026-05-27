use crate::error::Td3Error;
use crate::pattern::Pattern;

use super::{RbsSong, DEFAULT_TEMPLATE, TOTAL_SLOTS};

/// Parse a blob and return the 64 patterns as a vector.
pub fn import_bank(data: &[u8]) -> Result<Vec<Pattern>, Td3Error> {
    let song = RbsSong::parse(data)?;
    let RbsSong { patterns, .. } = song;
    Ok(patterns)
}

/// Build an `.rbs` blob from 64 patterns using the bundled template.
pub fn export_bank(patterns: Vec<Pattern>) -> Result<Vec<u8>, Td3Error> {
    if patterns.len() != TOTAL_SLOTS {
        return Err(Td3Error::FormatError(format!(
            ".rbs export expects {} patterns, got {}",
            TOTAL_SLOTS,
            patterns.len()
        )));
    }
    let song = RbsSong {
        template: DEFAULT_TEMPLATE.to_vec(),
        patterns,
    };
    song.serialize()
}
