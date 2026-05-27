use crate::error::Td3Error;
use crate::pattern::Pattern;

use super::{
    index_for, song::validate_address, RbsSong, DEVICES, GROUPS_PER_DEVICE, SLOTS_PER_GROUP,
};

/// Parse an `.rbs` file and return the pattern at `(device, group, slot)`.
pub fn import_single(
    data: &[u8],
    device: usize,
    group: usize,
    slot: usize,
) -> Result<Pattern, Td3Error> {
    let mut song = RbsSong::parse(data)?;
    let idx = index_for(device, group, slot);
    if idx >= song.patterns.len() {
        return Err(Td3Error::FormatError(format!(
            ".rbs pattern index {} out of range ({}×{}×{})",
            idx, DEVICES, GROUPS_PER_DEVICE, SLOTS_PER_GROUP
        )));
    }
    Ok(song.patterns.swap_remove(idx))
}

/// Place a single pattern at the first slot in the bundled template.
pub fn export_single(pattern: Pattern) -> Result<Vec<u8>, Td3Error> {
    export_single_at(pattern, 0, 0, 0)
}

/// Place a single pattern at the specified address in the bundled template.
pub fn export_single_at(
    pattern: Pattern,
    device: usize,
    group: usize,
    slot: usize,
) -> Result<Vec<u8>, Td3Error> {
    validate_address(device, group, slot)?;
    let mut song = RbsSong::blank()?;
    song.set_pattern(device, group, slot, pattern);
    song.serialize()
}
