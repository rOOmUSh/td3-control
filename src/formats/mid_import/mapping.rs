use crate::error::Td3Error;
use crate::formats::mid::{TD3_MIDI_BASE_PITCH, TD3_MIDI_TOP_PITCH};
use crate::step;

use super::quantize::NoteEvent;
use super::{MidiImportOptions, PolyphonyCandidate, PolyphonyResolver};

/// Build the 16-step array. Steps outside `active_steps` get defaults.
pub(super) fn derive_pattern_steps(
    steps_by_index: &[Vec<usize>],
    notes: &[NoteEvent],
    step_ticks: u32,
    active_steps: u8,
    options: &MidiImportOptions,
    resolver: &mut dyn PolyphonyResolver,
) -> Result<[step::Step; 16], Td3Error> {
    let mut out: [step::Step; 16] = Default::default();
    let active = active_steps as usize;

    // Pre-compute which note each Normal step picks so we can reference the
    // note for slide detection while walking the grid.
    let mut selected: [Option<usize>; 16] = [None; 16];
    for (i, bucket) in steps_by_index.iter().enumerate().take(active) {
        if bucket.is_empty() {
            continue;
        }
        let candidates: Vec<PolyphonyCandidate> = bucket
            .iter()
            .map(|&ni| PolyphonyCandidate {
                midi_pitch: notes[ni].pitch,
                velocity: notes[ni].velocity,
            })
            .collect();
        let picked = if candidates.len() == 1 {
            0
        } else {
            let idx = resolver.choose(i, &candidates)?;
            if idx >= candidates.len() {
                return Err(Td3Error::FormatError(format!(
                    "polyphony resolver returned index {} but only {} candidates were offered at step {}",
                    idx,
                    candidates.len(),
                    i + 1
                )));
            }
            idx
        };
        selected[i] = Some(bucket[picked]);
    }

    for i in 0..active {
        match selected[i] {
            Some(note_idx) => {
                let note = &notes[note_idx];
                let (td3_note, transpose) = pitch_to_td3(note.pitch, options.octave_offset);
                let accent = if note.velocity >= options.accent_threshold {
                    step::Accent::On
                } else {
                    step::Accent::Off
                };
                let slide = detect_slide(i, note, &selected, step_ticks, active);
                out[i] = step::Step {
                    note: td3_note,
                    transpose,
                    accent,
                    slide,
                    time: step::Time::Normal,
                };
            }
            None => {
                // Silent step - Tie if a prior note still covers this tick,
                // Rest otherwise.
                let step_start = (i as u32) * step_ticks;
                let ringing = selected
                    .iter()
                    .take(i)
                    .rev()
                    .find_map(|s| s.map(|ni| &notes[ni]))
                    .map(|n| n.end_tick > step_start + step_ticks / 4)
                    .unwrap_or(false);

                out[i] = step::Step {
                    time: if ringing {
                        step::Time::Tie
                    } else {
                        step::Time::Rest
                    },
                    ..step::Step::default()
                };
            }
        }
    }

    Ok(out)
}

/// Convert a MIDI pitch to TD-3 (note, transpose), clamping by octave if the
/// pitch falls outside the valid 3-octave TD-3 range.
///
/// Valid TD-3 MIDI pitches are 24..=60:
///   - 24..=35: Down octave
///   - 36..=47: Normal octave
///   - 48..=59: Up octave
///   - 60: Up octave with note=12
///
/// The top boundary is represented as note=12 ("C^") rather than splitting
/// into a higher octave band.
fn pitch_to_td3(midi_pitch: u8, octave_offset: i8) -> (u8, step::Transpose) {
    let mut raw = midi_pitch as i16 - octave_offset as i16;
    while raw < TD3_MIDI_BASE_PITCH {
        raw += 12;
    }
    while raw > TD3_MIDI_TOP_PITCH {
        raw -= 12;
    }
    let td3_pitch = raw as u8;
    let base = TD3_MIDI_BASE_PITCH as u8;
    if td3_pitch < base + 12 {
        (td3_pitch - base, step::Transpose::Down)
    } else if td3_pitch < base + 24 {
        (td3_pitch - base - 12, step::Transpose::Normal)
    } else if td3_pitch < TD3_MIDI_TOP_PITCH as u8 {
        (td3_pitch - base - 24, step::Transpose::Up)
    } else {
        (12, step::Transpose::Up)
    }
}

/// Reverses the exporter's slide encoding. Two possibilities:
///
/// **Case 1 - there is a next onset within the active range:** the exporter
/// emits a TD-3-style slide as a 1/8-step overlap - the outgoing note's gate
/// closes ~step/8 *after* the next note-on starts. So if this note's
/// `end_tick` lies past the next onset tick (with a small jitter margin),
/// it's a slide. Otherwise it's not.
///
/// **Case 2 - no next onset:** the gate ends either on a step boundary
/// (`group_end_tick + step_ticks` for slide=On) or half a step past one
/// (`group_end_tick + step_ticks/2` for slide=Off). We decide by how close
/// `end_tick mod step_ticks` sits to 0 vs. half a step. Closer to a step
/// boundary → slide; closer to a half-step → no slide.
fn detect_slide(
    step_idx: usize,
    note: &NoteEvent,
    selected: &[Option<usize>; 16],
    step_ticks: u32,
    active_steps: usize,
) -> step::Slide {
    let next_onset = ((step_idx + 1)..active_steps).find(|&j| selected[j].is_some());

    if let Some(next_step) = next_onset {
        let next_tick = (next_step as u32) * step_ticks;
        // Tolerate small timing jitter (< step/16) on the non-slide side.
        return if note.end_tick > next_tick + step_ticks / 16 {
            step::Slide::On
        } else {
            step::Slide::Off
        };
    }

    let remainder = note.end_tick % step_ticks;
    if remainder < step_ticks / 4 || remainder > step_ticks * 3 / 4 {
        step::Slide::On
    } else {
        step::Slide::Off
    }
}
