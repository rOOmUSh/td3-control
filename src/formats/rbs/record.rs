use crate::error::Td3Error;
use crate::formats::rbs_codec::{
    decode_step, encode_step_sequence, normalize_decoded_tie_runs, DecodeCarry,
};
use crate::pattern::Pattern;
use crate::step::{Step, Time};

use super::{DEFAULT_ACTIVE_STEPS, RECORD_LEN, STEPS_PER_PATTERN};

/// Decode a pattern record into a TD-3 pattern.
pub(super) fn decode_record(
    rec: &[u8],
    device_idx: usize,
    rec_idx: usize,
) -> Result<Pattern, Td3Error> {
    if rec.len() != RECORD_LEN {
        return Err(Td3Error::FormatError(format!(
            ".rbs record dev{} rec{} has length {} (expected {})",
            device_idx,
            rec_idx,
            rec.len(),
            RECORD_LEN
        )));
    }
    let active_steps = rec[1];
    if active_steps == 0 || active_steps > STEPS_PER_PATTERN as u8 {
        return Err(Td3Error::FormatError(format!(
            ".rbs record dev{} rec{} active_steps={} out of range (1..=16)",
            device_idx, rec_idx, active_steps
        )));
    }

    let mut steps: [Step; 16] = Default::default();
    let mut raw_steps = [(0u8, 0u8); 16];
    let mut carry = DecodeCarry::default();
    for (step_idx, step) in steps.iter_mut().take(STEPS_PER_PATTERN).enumerate() {
        let off = 2 + step_idx * 2;
        let pitch = rec[off];
        let flag = rec[off + 1];
        raw_steps[step_idx] = (pitch, flag);
        *step = decode_step(pitch, flag, &mut carry).map_err(|e| {
            Td3Error::FormatError(format!(
                ".rbs record dev{} rec{} step{}: {}",
                device_idx,
                rec_idx,
                step_idx + 1,
                e
            ))
        })?;
    }
    normalize_decoded_tie_runs(&raw_steps, &mut steps);

    Pattern::new(false, active_steps, steps)
}

/// Encode a TD-3 pattern into a pattern record.
pub(super) fn encode_record(pattern: &Pattern) -> Result<[u8; RECORD_LEN], Td3Error> {
    pattern.validate()?;
    let mut out = [0u8; RECORD_LEN];
    out[0] = 0x00;
    out[1] = pattern.active_steps;
    for (step_idx, (pitch, flag)) in encode_step_sequence(&pattern.step)
        .iter()
        .copied()
        .enumerate()
    {
        out[2 + step_idx * 2] = pitch;
        out[2 + step_idx * 2 + 1] = flag;
    }
    Ok(out)
}

/// Build the canonical silent pattern used for empty slots.
pub(super) fn silent_pattern() -> Result<Pattern, Td3Error> {
    let mut steps: [Step; 16] = Default::default();
    for step in steps.iter_mut() {
        step.time = Time::Rest;
    }
    Pattern::new(false, DEFAULT_ACTIVE_STEPS, steps)
}
