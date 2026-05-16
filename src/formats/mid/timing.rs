use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

pub(super) fn step_ticks(triplet: bool, ppqn: u16) -> Result<u32, Td3Error> {
    let divisor = if triplet { 3 } else { 4 };
    if !ppqn.is_multiple_of(divisor) {
        return Err(Td3Error::FormatError(format!(
            "ppqn={} must be divisible by {} for exact {} timing",
            ppqn,
            divisor,
            if triplet { "triplet" } else { "normal" }
        )));
    }
    let step = ppqn / divisor;
    if step == 0 {
        return Err(Td3Error::FormatError(format!(
            "ppqn={} too low for {} timing",
            ppqn,
            if triplet { "triplet" } else { "normal" }
        )));
    }
    Ok(step as u32)
}

pub(super) fn has_slide_connection(pattern: &Pattern, i: usize) -> bool {
    if i == 0 {
        return false;
    }
    let mut j = i - 1;
    loop {
        match pattern.step[j].time {
            step::Time::Normal => {
                return pattern.step[j].slide == step::Slide::On;
            }
            step::Time::Tie => {
                if j == 0 {
                    return false;
                }
                j -= 1;
            }
            step::Time::Rest | step::Time::TieRest => {
                return false;
            }
        }
    }
}
