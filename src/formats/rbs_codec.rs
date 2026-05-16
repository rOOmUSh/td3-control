//! Step-level codec for Propellerhead ReBirth `.rbs` files.
//!
//! Translates between the TD-3 domain `Step` and the 2-byte `(pitch, flag)`
//! encoding used inside a `303 ` chunk. The 34-byte pattern record wrapping
//! these 16 step pairs is handled in `rbs.rs`.
//!

use crate::error::Td3Error;
use crate::step::{Accent, Slide, Step, Time, Transpose};

// Flag-byte bitmap.
const FLAG_SLIDE: u8 = 0x01;
const FLAG_ACCENT: u8 = 0x02;
const FLAG_UP: u8 = 0x04;
const FLAG_DOWN: u8 = 0x08;
const FLAG_REST: u8 = 0x10;

/// Canonical 2-byte step encodings. These are the only forms our encoder
/// emits for the non-Normal timings - conservative normalisation of any
/// observed variants.
pub const STEP_REST: (u8, u8) = (0x00, FLAG_REST);
pub const STEP_TIE: (u8, u8) = (0x00, FLAG_SLIDE);

/// Encode a TD-3 `Step` into an RBS `(pitch, flag)` pair.
///
/// Rest/TieRest encode as REST rows. Rest control bits are retained when
/// present. Tie encodes as canonical TIE. Normal steps pack SLIDE, ACCENT,
/// and the three-way transpose (Down/Normal/Up) into the flag byte alongside
/// the chromatic pitch.
pub fn encode_step(step: &Step) -> (u8, u8) {
    match step.time {
        Time::Rest | Time::TieRest => encode_rest_step(step),
        Time::Tie => STEP_TIE,
        Time::Normal => {
            let mut flag = 0u8;
            if step.slide == Slide::On {
                flag |= FLAG_SLIDE;
            }
            if step.accent == Accent::On {
                flag |= FLAG_ACCENT;
            }
            match step.transpose {
                Transpose::Up => flag |= FLAG_UP,
                Transpose::Down => flag |= FLAG_DOWN,
                Transpose::Normal => {}
            }
            (step.note, flag)
        }
    }
}

/// Encode a full 16-step pattern into RBS step pairs.
pub fn encode_step_sequence(steps: &[Step; 16]) -> [(u8, u8); 16] {
    let mut out = [STEP_REST; 16];
    let mut idx = 0usize;

    while idx < steps.len() {
        if can_encode_held_run_as_slide_rest(&steps[idx]) {
            let mut end = idx + 1;
            while end < steps.len() && steps[end].time == Time::Tie {
                end += 1;
            }

            if end > idx + 1 {
                for (target, encoded) in out.iter_mut().enumerate().take(end - 1).skip(idx) {
                    *encoded = encode_held_slide_step(&steps[idx], target == idx);
                }
                out[end - 1] = encode_held_rest_step(&steps[idx]);
                idx = end;
                continue;
            }
        }

        out[idx] = encode_step(&steps[idx]);
        idx += 1;
    }

    out
}

fn encode_held_slide_step(step: &Step, first: bool) -> (u8, u8) {
    let mut flag = FLAG_SLIDE | transpose_flag(step.transpose);
    if first && step.accent == Accent::On {
        flag |= FLAG_ACCENT;
    }
    (step.note, flag)
}

fn encode_held_rest_step(step: &Step) -> (u8, u8) {
    (step.note, FLAG_REST | transpose_flag(step.transpose))
}

fn transpose_flag(transpose: Transpose) -> u8 {
    match transpose {
        Transpose::Up => FLAG_UP,
        Transpose::Down => FLAG_DOWN,
        Transpose::Normal => 0,
    }
}

fn can_encode_held_run_as_slide_rest(step: &Step) -> bool {
    step.time == Time::Normal
        && step.slide == Slide::Off
        && step.accent == Accent::Off
        && !(step.note == 0 && step.transpose == Transpose::Normal)
}

/// Carry state for decoding a sequence of steps. Tie/Rest steps in the RBS
/// stream carry no explicit pitch - we replay the last sounding note so the
/// decoded TD-3 `Step` has a sensible `note`/`transpose` for display.
#[derive(Debug, Clone, Copy)]
pub struct DecodeCarry {
    pub note: u8,
    pub transpose: Transpose,
    pub has_sounding_note: bool,
}

impl Default for DecodeCarry {
    fn default() -> Self {
        Self {
            note: 0,
            transpose: Transpose::Normal,
            has_sounding_note: false,
        }
    }
}

fn encode_rest_step(step: &Step) -> (u8, u8) {
    let mut flag = FLAG_REST;
    if step.slide == Slide::On {
        flag |= FLAG_SLIDE;
    }
    if step.accent == Accent::On {
        flag |= FLAG_ACCENT;
    }
    flag |= transpose_flag(step.transpose);

    if flag == FLAG_REST {
        STEP_REST
    } else {
        (step.note, flag)
    }
}

/// Decode an RBS `(pitch, flag)` pair into a TD-3 `Step`, mutating `carry`
/// when a sounding note is encountered.
///
/// Ambiguity rules:
///   - `flag & REST` keeps the other control bits and starts as REST.
///   - `pitch == 0x00 && flag == SLIDE` carries a prior non-C note as TIE.
///   - Otherwise the row starts as a Normal note.
pub fn decode_step(pitch: u8, flag: u8, carry: &mut DecodeCarry) -> Result<Step, Td3Error> {
    if pitch > 12 {
        return Err(Td3Error::FormatError(format!(
            ".rbs step pitch {:#04x} out of range (expected 0..=0x0C)",
            pitch
        )));
    }

    if pitch == 0x00
        && flag == FLAG_SLIDE
        && carry.has_sounding_note
        && !(carry.note == 0 && carry.transpose == Transpose::Normal)
    {
        return Ok(Step {
            note: carry.note,
            transpose: carry.transpose,
            accent: Accent::Off,
            slide: Slide::Off,
            time: Time::Tie,
        });
    }

    let slide = if flag & FLAG_SLIDE != 0 {
        Slide::On
    } else {
        Slide::Off
    };
    let accent = if flag & FLAG_ACCENT != 0 {
        Accent::On
    } else {
        Accent::Off
    };
    let transpose = match (flag & FLAG_UP, flag & FLAG_DOWN) {
        (0, 0) => Transpose::Normal,
        (_, 0) => Transpose::Up,
        (0, _) => Transpose::Down,
        // UP + DOWN simultaneously appears in ReBirth's "empty slot"
        // pseudo-random padding data (e.g. JAM fixture record 16, step 1
        // = 0x0F = SLIDE|ACCENT|UP|DOWN). Treat as cancelling → Normal so
        // these otherwise-unreachable slots decode cleanly. Audibly
        // irrelevant: empty slots are not played.
        _ => Transpose::Normal,
    };

    let rest = flag & FLAG_REST != 0;
    let mut step = Step {
        note: if rest && pitch == 0 {
            carry.note
        } else {
            pitch
        },
        transpose,
        accent,
        slide,
        time: if rest { Time::Rest } else { Time::Normal },
    };
    if rest && flag & (FLAG_UP | FLAG_DOWN) == 0 {
        step.transpose = carry.transpose;
    }
    if step.time == Time::Normal {
        carry.note = step.note;
        carry.transpose = step.transpose;
        carry.has_sounding_note = true;
    }
    Ok(step)
}

/// Convert RBS held-note runs into internal ties.
pub fn normalize_decoded_tie_runs(raw: &[(u8, u8); 16], steps: &mut [Step; 16]) {
    promote_slide_rest_rows(raw, steps);
    collapse_slide_rest_tails(raw, steps);
    refresh_carried_display_notes(raw, steps);
}

fn promote_slide_rest_rows(raw: &[(u8, u8); 16], steps: &mut [Step; 16]) {
    for idx in 1..steps.len() {
        let (pitch, flag) = raw[idx];
        if flag & FLAG_REST == 0 {
            continue;
        }
        let previous = steps[idx - 1];
        if previous.time != Time::Normal || previous.slide != Slide::On {
            continue;
        }

        let transpose = transpose_from_flag(flag);
        if flag & FLAG_SLIDE != 0 {
            steps[idx].time = Time::Normal;
            steps[idx].note = pitch;
            steps[idx].transpose = transpose;
            steps[idx].slide = Slide::On;
            continue;
        }

        if pitch != previous.note || transpose != previous.transpose {
            steps[idx].time = Time::Normal;
            steps[idx].note = pitch;
            steps[idx].transpose = transpose;
            steps[idx].slide = Slide::Off;
        } else {
            steps[idx].time = Time::Tie;
            steps[idx].note = previous.note;
            steps[idx].transpose = previous.transpose;
            steps[idx].accent = Accent::Off;
            steps[idx].slide = Slide::Off;
        }
    }
}

fn collapse_slide_rest_tails(raw: &[(u8, u8); 16], steps: &mut [Step; 16]) {
    let mut idx = 0usize;

    while idx < steps.len() {
        if !is_hold_slide_start(&steps[idx]) {
            idx += 1;
            continue;
        }
        let held_note = steps[idx].note;
        let held_transpose = steps[idx].transpose;
        if held_note == 0 && held_transpose == Transpose::Normal {
            idx += 1;
            continue;
        }
        let mut end = idx;

        while end < steps.len()
            && is_hold_slide_row(&steps[end], held_note, held_transpose, end == idx)
        {
            end += 1;
        }

        if end < steps.len() && is_matching_hold_rest(raw[end], held_note, held_transpose) {
            steps[idx].slide = Slide::Off;
            steps[idx].time = Time::Normal;

            for step in steps.iter_mut().take(end + 1).skip(idx + 1) {
                step.note = held_note;
                step.transpose = held_transpose;
                step.accent = Accent::Off;
                step.slide = Slide::Off;
                step.time = Time::Tie;
            }

            idx = end + 1;
            continue;
        }

        idx += 1;
    }
}

fn refresh_carried_display_notes(raw: &[(u8, u8); 16], steps: &mut [Step; 16]) {
    let mut note = 0u8;
    let mut transpose = Transpose::Normal;

    for (idx, step) in steps.iter_mut().enumerate() {
        match step.time {
            Time::Normal => {
                note = step.note;
                transpose = step.transpose;
            }
            Time::Tie => {
                step.note = note;
                step.transpose = transpose;
            }
            Time::Rest | Time::TieRest => {
                let (pitch, flag) = raw[idx];
                if pitch == 0 && flag & (FLAG_UP | FLAG_DOWN) == 0 {
                    step.note = note;
                    step.transpose = transpose;
                }
            }
        }
    }
}

fn is_hold_slide_start(step: &Step) -> bool {
    step.time == Time::Normal && step.slide == Slide::On && step.accent == Accent::Off
}

fn is_hold_slide_row(step: &Step, note: u8, transpose: Transpose, first: bool) -> bool {
    step.time == Time::Normal
        && step.slide == Slide::On
        && step.note == note
        && step.transpose == transpose
        && (first || step.accent == Accent::Off)
}

fn is_matching_hold_rest(raw: (u8, u8), note: u8, transpose: Transpose) -> bool {
    let (pitch, flag) = raw;
    if pitch != note || flag & FLAG_REST == 0 {
        return false;
    }
    if flag & (FLAG_SLIDE | FLAG_ACCENT) != 0 {
        return false;
    }
    if flag & FLAG_UP != 0 && flag & FLAG_DOWN != 0 {
        return false;
    }
    transpose_from_flag(flag) == transpose
}

fn transpose_from_flag(flag: u8) -> Transpose {
    match (flag & FLAG_UP, flag & FLAG_DOWN) {
        (0, 0) => Transpose::Normal,
        (_, 0) => Transpose::Up,
        (0, _) => Transpose::Down,
        _ => Transpose::Normal,
    }
}
