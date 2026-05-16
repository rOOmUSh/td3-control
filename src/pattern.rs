use std::convert::TryInto;

use crate::error::Td3Error;
use crate::step;

// ---------------------------------------------------------------------------
// Pattern - the core 16-step sequencer data
// ---------------------------------------------------------------------------

/// A TD-3 sequencer pattern: 16 steps with metadata.
#[derive(Debug)]
pub struct Pattern {
    pub(crate) triplet: bool,
    pub(crate) active_steps: u8,
    pub(crate) step: [step::Step; 16],
}

impl Pattern {
    /// Construct a validated Pattern. Rejects invalid active_steps or note values.
    pub fn new(triplet: bool, active_steps: u8, steps: [step::Step; 16]) -> Result<Self, Td3Error> {
        let created = Self {
            triplet,
            active_steps,
            step: steps,
        };
        created.validate()?;
        Ok(created)
    }

    /// Validate all pattern invariants.
    /// - active_steps must be 1..=16
    /// - each step note must be 0..=12 (C through C^)
    pub fn validate(&self) -> Result<(), Td3Error> {
        if self.active_steps == 0 || self.active_steps > 16 {
            return Err(Td3Error::InvalidActiveSteps {
                value: self.active_steps,
            });
        }
        for (idx, entry) in self.step.iter().enumerate() {
            if entry.note > 12 {
                return Err(Td3Error::InvalidNote {
                    step: idx + 1,
                    value: entry.note,
                });
            }
        }
        Ok(())
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            triplet: false,
            active_steps: 1,
            step: [step::Step::default(); 16],
        }
    }
}

pub(crate) fn validate_pattern_address(
    patgroup: u8,
    slot_num: u8,
    side: u8,
) -> Result<u8, Td3Error> {
    if patgroup > 3 || slot_num > 7 || side > 1 {
        return Err(Td3Error::InvalidPatternAddress {
            patgroup,
            slot: slot_num,
            side,
        });
    }
    Ok(slot_num + (side << 3))
}

// ---------------------------------------------------------------------------
// SysEx payload layout - byte offsets within the 115-byte body
// ---------------------------------------------------------------------------

pub const PATTERN_SYSEX_PAYLOAD_LEN: usize = 115;
const MESSAGE_KIND: u8 = 0x78;

/// Pitch data: 16 slots x 2 nibble-bytes each, starting at byte 5.
const PITCH_OFFSET: usize = 0x05;
/// Accent flags: 16 slots x 2 bytes each (high byte always 0x00).
const ACCENT_OFFSET: usize = 0x26;
/// Slide flags: 16 slots x 2 bytes each (high byte always 0x00).
const SLIDE_OFFSET: usize = 0x46;
/// Triplet flag: 0x00 or 0x01.
const TRIPLET_OFFSET: usize = 0x66;
/// Active step count stored as two nibble-bytes (high, low).
const STEP_COUNT_HIGH: usize = 0x67;
const STEP_COUNT_LOW: usize = 0x68;
/// Tie bitmask: 4 nibble-bytes encoding a 16-bit flag word.
const TIE_MASK_OFFSET: usize = 0x6B;
/// Rest bitmask: 4 nibble-bytes encoding a 16-bit flag word.
const REST_MASK_OFFSET: usize = 0x6F;

// ---------------------------------------------------------------------------
// Decode: SysEx payload → Pattern
// ---------------------------------------------------------------------------

/// Decode an exact 115-byte SysEx payload into a validated Pattern.
///
/// The TD-3 uses 303-style packed note layout: pitch, accent, and slide
/// arrays are consumed sequentially only by Normal (note-on) steps.
/// Tie, Rest, and TieRest steps do not consume entries from these arrays.
pub fn sysex_to_pattern(payload: &[u8]) -> Result<Pattern, Td3Error> {
    if payload.len() < PATTERN_SYSEX_PAYLOAD_LEN {
        return Err(Td3Error::PayloadTooShort {
            expected: PATTERN_SYSEX_PAYLOAD_LEN,
            actual: payload.len(),
        });
    }
    if payload.len() > PATTERN_SYSEX_PAYLOAD_LEN {
        return Err(Td3Error::InvalidPayloadLength {
            expected: PATTERN_SYSEX_PAYLOAD_LEN,
            actual: payload.len(),
        });
    }
    if payload[0] != MESSAGE_KIND {
        return Err(Td3Error::WrongMessageId { actual: payload[0] });
    }

    // Decode tie/rest bitmasks into per-step gate states.
    let tie_word = unpack_mask(payload, TIE_MASK_OFFSET, "tie mask")?;
    let rest_word = unpack_mask(payload, REST_MASK_OFFSET, "rest mask")?;

    let mut gate = [step::Time::Normal; 16];
    for position in 0..16u16 {
        let combined = ((tie_word >> position) & 1) + (((rest_word >> position) & 1) << 1);
        gate[position as usize] = combined
            .try_into()
            .map_err(|_| Td3Error::InvalidTime(combined))?;
    }

    // Unpack 303-packed pitch, accent, and slide data.
    //
    // Only Normal steps consume entries from the packed arrays.
    // Tie/TieRest steps carry the preceding sounding note forward.
    // Rest steps carry the preceding note for display purposes.
    let mut steps: [step::Step; 16] = Default::default();
    let mut cursor: usize = 0;
    let mut prior_note: u8 = 0;
    let mut prior_shift = step::Transpose::Normal;

    for position in 0..16 {
        let target = &mut steps[position];

        match gate[position] {
            step::Time::Normal => {
                let byte_pair = cursor * 2;
                let (chromatic, shift) = decode_pitch(
                    payload[PITCH_OFFSET + byte_pair],
                    payload[PITCH_OFFSET + byte_pair + 1],
                )?;
                target.note = chromatic;
                target.transpose = shift;

                let accent_byte = decode_paired_flag(payload, ACCENT_OFFSET, byte_pair, "accent")?;
                target.accent = accent_byte
                    .try_into()
                    .map_err(|_| Td3Error::InvalidAccent(accent_byte))?;

                let slide_byte = decode_paired_flag(payload, SLIDE_OFFSET, byte_pair, "slide")?;
                target.slide = slide_byte
                    .try_into()
                    .map_err(|_| Td3Error::InvalidSlide(slide_byte))?;

                prior_note = chromatic;
                prior_shift = shift;
                cursor += 1;
            }
            step::Time::Tie => {
                // Hold the preceding sounding note through this step
                target.note = prior_note;
                target.transpose = prior_shift;
            }
            step::Time::Rest | step::Time::TieRest => {
                // Silence - carry prior note for display consistency
                target.note = prior_note;
                target.transpose = prior_shift;
            }
        }

        target.time = gate[position];
    }

    let step_count = (validate_nibble(payload[STEP_COUNT_HIGH], "active steps high")? << 4)
        + validate_nibble(payload[STEP_COUNT_LOW], "active steps low")?;
    let is_triplet = decode_binary_flag(payload[TRIPLET_OFFSET], "triplet")?;

    // Centralized validation via Pattern::new
    Pattern::new(is_triplet, step_count, steps)
}

// ---------------------------------------------------------------------------
// Encode: Pattern → SysEx payload
// ---------------------------------------------------------------------------

/// Encode a validated Pattern into a 115-byte SysEx payload.
///
/// Packs notes in TD-3/303 sequential format where only Normal steps
/// consume pitch/accent/slide slots. Returns an error if the pattern
/// fails validation.
pub fn pattern_to_sysex(
    pat: &Pattern,
    patgroup: u8,
    slot_num: u8,
    side: u8,
) -> Result<Vec<u8>, Td3Error> {
    pat.validate()?;
    let slot_addr = validate_pattern_address(patgroup, slot_num, side)?;

    // Collect gate states.
    let mut gate = [step::Time::Normal; 16];
    for (idx, slot) in gate.iter_mut().enumerate() {
        *slot = pat.step[idx].time;
    }

    // Pack pitch, accent, and slide (only Normal steps get slots).
    let mut pitch_bytes = [0u8; 32];
    let mut accent_bytes = [0u8; 32];
    let mut slide_bytes = [0u8; 32];
    let mut cursor: usize = 0;

    for (idx, &gate_state) in gate.iter().enumerate() {
        if gate_state == step::Time::Normal {
            let entry = &pat.step[idx];
            let (hi, lo) = encode_pitch(entry.note, entry.transpose);
            let byte_pair = cursor * 2;
            pitch_bytes[byte_pair] = hi;
            pitch_bytes[byte_pair + 1] = lo;
            accent_bytes[byte_pair + 1] = entry.accent as u8;
            slide_bytes[byte_pair + 1] = entry.slide as u8;
            cursor += 1;
        }
    }

    // Fill remaining packed slots with default pitch (C Normal = 0x01 0x08)
    for trailing in cursor..16 {
        let byte_pair = trailing * 2;
        pitch_bytes[byte_pair] = 0x01;
        pitch_bytes[byte_pair + 1] = 0x08;
        // accent and slide remain zeroed
    }

    // Build tie/rest bitmasks from gate states.
    let mut tie_word = 0u16;
    let mut rest_word = 0u16;
    for (idx, &gate_state) in gate.iter().enumerate() {
        tie_word |= ((gate_state as u16) & 0b01) << idx;
        rest_word |= (((gate_state as u16) & 0b10) >> 1) << idx;
    }

    // Assemble the 115-byte payload
    let mut result: Vec<u8> = Vec::with_capacity(PATTERN_SYSEX_PAYLOAD_LEN);
    result.push(MESSAGE_KIND);
    result.extend_from_slice(&[patgroup, slot_addr]);
    result.extend_from_slice(&[0x00, 0x01]);
    result.extend_from_slice(&pitch_bytes);
    result.extend_from_slice(&accent_bytes);
    result.extend_from_slice(&slide_bytes);
    result.extend_from_slice(&[0x00, pat.triplet as u8]);
    result.extend_from_slice(&[(pat.active_steps & 0xF0) >> 4, pat.active_steps & 0x0F]);
    result.extend_from_slice(&[0x00, 0x00]);
    result.extend_from_slice(&pack_mask(tie_word));
    result.extend_from_slice(&pack_mask(rest_word));
    Ok(result)
}

// ---------------------------------------------------------------------------
// Pitch encoding / decoding helpers
// ---------------------------------------------------------------------------

/// Decode a pair of SysEx nibble-bytes into a note index (0..=12) and
/// transpose value.
///
/// The TD-3 encodes pitch as `base = 12 + chromatic + (octave * 12)`.
/// For upper C (note 12 / C^), bit 3 of the high nibble is set as a marker,
/// or the 7-bit value equals 0x30. C^ supports all three transpose values.
fn decode_pitch(high_nibble: u8, low_nibble: u8) -> Result<(u8, step::Transpose), Td3Error> {
    let high = validate_nibble(high_nibble, "pitch high")?;
    let low = validate_nibble(low_nibble, "pitch low")?;
    let pitch_raw = ((high << 4) | low) & 0x7F;
    let is_upper_c = (high & 0x08) != 0 || pitch_raw == 0x30;

    let chromatic = (pitch_raw % 12) + if is_upper_c { 12 } else { 0 };
    let octave = pitch_raw / 12;
    let shift_code = octave
        .wrapping_sub(1)
        .wrapping_sub(if is_upper_c { 1 } else { 0 });

    let shift = shift_code
        .try_into()
        .map_err(|_| Td3Error::InvalidTranspose(shift_code))?;

    Ok((chromatic, shift))
}

/// Encode a note index (0..=12) and transpose into a pair of SysEx
/// nibble-bytes (high, low).
///
/// For upper C (note >= 12), bit 7 is set in the composed byte to mark
/// the C^ flag. All three transpose values are supported for C^.
fn encode_pitch(chromatic: u8, shift: step::Transpose) -> (u8, u8) {
    let shift_offset: u8 = match shift {
        step::Transpose::Down => 0,
        step::Transpose::Normal => 12,
        step::Transpose::Up => 24,
    };

    let high_bit = if chromatic >= 12 { 0x80u8 } else { 0u8 };
    let encoded = 12u8
        .wrapping_add(chromatic)
        .wrapping_add(shift_offset)
        .wrapping_add(high_bit);

    ((encoded & 0xF0) >> 4, encoded & 0x0F)
}

// ---------------------------------------------------------------------------
// Bitmask packing / unpacking
// ---------------------------------------------------------------------------

/// Unpack four nibble-bytes from the payload into a 16-bit flag word.
///
/// The TD-3 stores 16-bit masks as four low-nibble bytes in the order
/// `[bits 7..4, bits 3..0, bits 15..12, bits 11..8]`.
fn validate_nibble(value: u8, field: &'static str) -> Result<u8, Td3Error> {
    if value <= 0x0F {
        Ok(value)
    } else {
        Err(Td3Error::InvalidNibble { field, value })
    }
}

fn decode_binary_flag(value: u8, field: &'static str) -> Result<bool, Td3Error> {
    match value {
        0x00 => Ok(false),
        0x01 => Ok(true),
        _ => Err(Td3Error::InvalidFlag { field, value }),
    }
}

fn decode_paired_flag(
    payload: &[u8],
    low_offset: usize,
    byte_pair: usize,
    field: &'static str,
) -> Result<u8, Td3Error> {
    let high = payload[low_offset + byte_pair - 1];
    if high != 0x00 {
        return Err(Td3Error::InvalidFlag { field, value: high });
    }

    let low = payload[low_offset + byte_pair];
    match low {
        0x00 | 0x01 => Ok(low),
        _ => Err(Td3Error::InvalidFlag { field, value: low }),
    }
}

fn unpack_mask(payload: &[u8], offset: usize, field: &'static str) -> Result<u16, Td3Error> {
    let b0 = validate_nibble(payload[offset], field)? as u16;
    let b1 = validate_nibble(payload[offset + 1], field)? as u16;
    let b2 = validate_nibble(payload[offset + 2], field)? as u16;
    let b3 = validate_nibble(payload[offset + 3], field)? as u16;
    Ok((b0 << 4) | b1 | (b2 << 12) | (b3 << 8))
}

/// Pack a 16-bit flag word into four nibble-bytes using the TD-3 layout.
fn pack_mask(word: u16) -> [u8; 4] {
    [
        ((word >> 4) & 0x0F) as u8,
        (word & 0x0F) as u8,
        ((word >> 12) & 0x0F) as u8,
        ((word >> 8) & 0x0F) as u8,
    ]
}
