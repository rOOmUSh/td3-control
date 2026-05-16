use super::*;

pub(super) fn build_combined_rbs(
    acid: &[Pattern; 4],
    bass: &[Pattern; 4],
    bass_full: Option<&[Pattern; 20]>,
) -> Result<Vec<u8>, Td3Error> {
    let mut slots: Vec<Pattern> = Vec::with_capacity(rbs::TOTAL_SLOTS);
    for _ in 0..rbs::TOTAL_SLOTS {
        slots.push(silent_pattern()?);
    }
    for (i, pattern) in acid.iter().enumerate() {
        slots[rbs::index_for(0, 0, i)] = clone_pattern(pattern)?;
    }
    match bass_full {
        Some(full) => {
            for (idx, pattern) in full.iter().enumerate() {
                let group = idx / 8;
                let slot = idx % 8;
                slots[rbs::index_for(1, group, slot)] = clone_pattern(pattern)?;
            }
        }
        None => {
            for (i, pattern) in bass.iter().enumerate() {
                slots[rbs::index_for(1, 0, i)] = clone_pattern(pattern)?;
            }
        }
    }
    rbs::export_bank(slots)
}

pub(super) fn silent_pattern() -> Result<Pattern, Td3Error> {
    let mut steps: [Step; 16] = Default::default();
    for s in steps.iter_mut() {
        s.time = Time::Rest;
    }
    Pattern::new(false, 16, steps)
}
