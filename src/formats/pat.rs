//! ABL3 `.pat` single-pattern text format.
//!
//! A `.pat` file is a plain-text export used by the Audiorealism ABL3 plugin
//! (and by SynthTribe when round-tripping through ABL3). Structure:
//!
//!   ; ABL3 Meta tag: <N>                  # N = step count (1..=16)
//!   ; <knob metadata line>                # 8 synth knobs on export; 2 rhythm
//!                                         # params on ABL3 round-trip save
//!   <col0> <col1> <col2> <col3> <col4> <col5>
//!   ... (N rows total)
//!
//! Line endings are `\r\r\n` (double-CR LF) as emitted by ABL3. We write
//! that exact separator to stay bytewise close to ABL3 output; on import
//! we tolerate any combination of CR/LF.
//!
//! # Column encoding
//!
//! | col | meaning                                                            |
//! |-----|--------------------------------------------------------------------|
//! |  0  | Effective pitch semitone, 0..24 = `note + (12 if transpose==UP)`   |
//! |  1  | `1` iff transpose == DOWN (col0 still carries raw note index)      |
//! |  2  | Reserved - always `0` in every observed ABL3 export                |
//! |  3  | Accent flag                                                        |
//! |  4  | Slide/legato marker - see tie-collapse rule below                  |
//! |  5  | Gate: `0` = rest, `1` = note plays                                 |
//!
//! # The tie-collapse rule (ABL3 has no TIE concept)
//!
//! ABL3 expresses TD-3 TIE semantics as "two adjacent slides at the same
//! effective pitch". So the encoder/decoder pair normalizes:
//!
//!   Encode (TD-3 -> pat):
//!     For every step N with time == TIE (or TIE_REST):
//!       - set col4[N-1] = 1 (idempotent - may already be 1)
//!       - set col4[N]   = 1, emitted as a slide-Normal step (tie is dissolved)
//!     All other steps: col4 = this step's raw slide flag.
//!
//!   Decode (pat -> TD-3):
//!     Scan for runs of consecutive rows where col4==1 AND col5==1 AND the
//!     effective pitch (col0,col1) is identical.
//!     Runs of length >= 2:
//!       - first step  -> slide ON, time Normal
//!       - rest of run -> slide OFF, time TIE
//!     All other rows: col4 maps directly to the slide flag.
//!
//! # Acceptable lossiness
//!
//! Round-trip asymmetry:
//!   device `[Normal, Tie]`     --encode-->  pat `[col4=1, col4=1]`
//!   pat    `[col4=1, col4=1]`  --decode-->  device `[Normal+Slide, Tie]`
//! i.e. a plain NORM+TIE on the device re-imports as SLIDE+TIE. This is
//! audibly identical on the TD-3 (same-pitch "slide" produces no portamento,
//! since the tied second step has the same pitch).
//!
//! Likewise, `TieRest` downgrades to `Rest` on import (ABL3 has no
//! tie-rest distinction). Again audibly identical.

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};

/// ABL3 line separator (`CR CR LF`). Preserved exactly so a
/// `TD-3 -> pat -> ABL3 -> save` cycle stays close to ABL3-native bytes.
const LINE_SEP: &str = "\r\r\n";

/// Default knob line written on export. TD-3 patterns don't carry synth
/// knob state, so we emit all-centered (0.500000) values - matching what
/// SynthTribe writes for a freshly-exported pattern.
const DEFAULT_KNOBS: &str = "; Tune: 0.500000 Cutoff: 0.500000 Resonance: 0.500000 Envmod: 0.500000 Decay: 0.500000 Accent: 0.500000 Waveform: 0.500000 Volume: 0.500000 ";

// ---------------------------------------------------------------------------
// Export: Pattern -> .pat text
// ---------------------------------------------------------------------------

pub fn export(pattern: &Pattern) -> String {
    let n = pattern.active_steps as usize;

    let mut slide_flag = [false; 16];
    let mut gate_flag = [false; 16];

    // Base state.
    // ABL3 PAT has no TieRest: Rest wins.
    for i in 0..n {
        match pattern.step[i].time {
            Time::Normal => {
                slide_flag[i] = pattern.step[i].slide == Slide::On;
                gate_flag[i] = true;
            }
            Time::Tie => {
                slide_flag[i] = false;
                gate_flag[i] = false;
            }
            Time::Rest | Time::TieRest => {
                slide_flag[i] = false;
                gate_flag[i] = false;
            }
        }
    }

    // ABL3 PAT has no Tie.
    // Encode internal Normal,Tie,Tie... as:
    // N,T       -> N+S,R
    // N,T,T     -> N+S,N+S,R
    // N,T,T,T   -> N+S,N+S,N+S,R
    let mut i = 0;
    while i < n {
        if !matches!(pattern.step[i].time, Time::Normal) {
            i += 1;
            continue;
        }

        let start = i;
        let mut j = i + 1;

        while j < n && matches!(pattern.step[j].time, Time::Tie) {
            j += 1;
        }

        if j - start >= 2 {
            // Rows start..j-2 are Normal+Slide.
            for k in start..(j - 1) {
                slide_flag[k] = true;
                gate_flag[k] = true;
            }

            // Final held row becomes Rest.
            slide_flag[j - 1] = false;
            gate_flag[j - 1] = false;
        }

        i = j;
    }

    let mut out = String::new();
    out.push_str(&format!("; ABL3 Meta tag: {}", n));
    out.push_str(LINE_SEP);
    out.push_str(DEFAULT_KNOBS);
    out.push_str(LINE_SEP);

    for (idx, s) in pattern.step.iter().take(n).enumerate() {
        let (col0, col1) = encode_pitch(s);
        let col2 = 0u8;
        let col3 = if s.accent == Accent::On { 1u8 } else { 0 };
        let col4 = if slide_flag[idx] { 1u8 } else { 0 };
        let col5 = if gate_flag[idx] { 1u8 } else { 0 };

        out.push_str(&format!(
            "{} {} {} {} {} {}",
            col0, col1, col2, col3, col4, col5
        ));
        out.push_str(LINE_SEP);
    }

    out
}

fn encode_pitch(s: &Step) -> (u8, u8) {
    match s.transpose {
        Transpose::Down => (s.note, 1),
        Transpose::Normal => (s.note, 0),
        Transpose::Up => (s.note + 12, 0),
    }
}

// ---------------------------------------------------------------------------
// Import: .pat text -> Pattern
// ---------------------------------------------------------------------------

pub fn import(data: &str) -> Result<Pattern, Td3Error> {
    let (tag, rows) = parse_rows(data)?;
    if rows.is_empty() {
        return Err(Td3Error::FormatError(
            ".pat file has no step rows".to_string(),
        ));
    }
    if rows.len() > 16 {
        return Err(Td3Error::FormatError(format!(
            ".pat file has {} step rows (max 16)",
            rows.len()
        )));
    }

    let mut steps: [Step; 16] = Default::default();

    // First pass: populate pitch/accent/gate and a PROVISIONAL slide/time
    // (treating col4 as raw slide, col5 as gate). The tie-collapse pass
    // below rewrites runs that are actually tied sequences.
    for (i, row) in rows.iter().enumerate() {
        let (note, transpose) = decode_pitch(row[0], row[1])?;
        steps[i].note = note;
        steps[i].transpose = transpose;
        steps[i].accent = if row[3] == 1 { Accent::On } else { Accent::Off };
        steps[i].slide = if row[4] == 1 { Slide::On } else { Slide::Off };
        steps[i].time = if row[5] == 0 {
            Time::Rest
        } else {
            Time::Normal
        };
    }

    // Second pass: decode ABL3 PAT held-note encoding.
    //
    // ABL3 PAT has no Tie. Our exporter encodes internal held notes as:
    //
    //   N,T       -> N+S,R
    //   N,T,T     -> N+S,N+S,R
    //   N,T,T,T   -> N+S,N+S,N+S,R
    //
    // Therefore import must reverse:
    //
    //   N+S,R         -> N,T
    //   N+S,N+S,R     -> N,T,T
    //   N+S,N+S,N+S,R -> N,T,T,T
    let n = rows.len();
    let mut i = 0;

    while i < n {
        if rows[i][4] == 1 && rows[i][5] == 1 {
            let start_pitch = (rows[i][0], rows[i][1]);

            // Count consecutive Normal+Slide rows with the same pitch.
            let mut j = i;
            while j < n
                && rows[j][4] == 1
                && rows[j][5] == 1
                && (rows[j][0], rows[j][1]) == start_pitch
            {
                j += 1;
            }

            // Valid encoded tie-run must end with Rest.
            // Do NOT require the Rest row to have the same pitch:
            // in PAT, gate=0 means pitch is not musically meaningful.
            if j < n && rows[j][5] == 0 {
                let held_note = steps[i].note;
                let held_transpose = steps[i].transpose;

                // First row becomes plain Normal.
                steps[i].slide = Slide::Off;
                steps[i].time = Time::Normal;

                // Every following row, including the terminal Rest row,
                // becomes Tie with the held pitch.
                for step in steps.iter_mut().take(j + 1).skip(i + 1) {
                    step.note = held_note;
                    step.transpose = held_transpose;
                    step.slide = Slide::Off;
                    step.time = Time::Tie;
                }

                i = j + 1;
                continue;
            }
        }

        i += 1;
    }

    let active_steps = tag.unwrap_or(rows.len() as u8);
    Pattern::new(false, active_steps, steps)
}

fn decode_pitch(col0: u8, col1: u8) -> Result<(u8, Transpose), Td3Error> {
    // The `{note=0, UP}` and `{note=12 (C^), Normal}` states both encode
    // as col0=12. We commit to UP for col0 >= 12 because every other UP
    // value (13..=24) is unambiguously UP, so this gives a consistent
    // "col0 >= 12 with col1=0 means UP" rule. The cosmetic alternative
    // `{note=12, Normal}` is audibly identical so the convention is
    // lossless in the only sense that matters on the wire.
    match col1 {
        0 => {
            if col0 < 12 {
                Ok((col0, Transpose::Normal))
            } else if col0 <= 24 {
                Ok((col0 - 12, Transpose::Up))
            } else {
                Err(Td3Error::FormatError(format!(
                    ".pat col0={} out of range (max 24 for UP-octave)",
                    col0
                )))
            }
        }
        1 => {
            if col0 <= 12 {
                Ok((col0, Transpose::Down))
            } else {
                Err(Td3Error::FormatError(format!(
                    ".pat col0={} with DOWN flag (col1=1) out of range (max 12)",
                    col0
                )))
            }
        }
        other => Err(Td3Error::FormatError(format!(
            ".pat col1={} invalid (expected 0 or 1)",
            other
        ))),
    }
}

// ---------------------------------------------------------------------------
// Row parser
// ---------------------------------------------------------------------------

fn parse_rows(data: &str) -> Result<(Option<u8>, Vec<[u8; 6]>), Td3Error> {
    let mut tag: Option<u8> = None;
    let mut rows: Vec<[u8; 6]> = Vec::new();

    // Tolerate any combination of CR / LF (ABL3 uses \r\r\n; other tools
    // may produce \r\n or \n). Splitting on either character and skipping
    // empty fragments handles all three.
    for raw in data.split(['\r', '\n']) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(body) = line.strip_prefix(';') {
            if let Some(rest) = body.trim_start().strip_prefix("ABL3 Meta tag:") {
                tag = rest.trim().parse::<u8>().ok();
            }
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 6 {
            return Err(Td3Error::FormatError(format!(
                ".pat row must have 6 ints, got {} in '{}'",
                parts.len(),
                line
            )));
        }
        let mut row = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            row[i] = part.parse::<u8>().map_err(|_| {
                Td3Error::FormatError(format!(
                    ".pat row has non-integer (or out-of-byte-range) column {}: '{}'",
                    i, part
                ))
            })?;
        }
        rows.push(row);
    }

    Ok((tag, rows))
}
