use crate::error::Td3Error;

use super::parse::{MidiEvent, TimedEvent};

// ---------------------------------------------------------------------------
// Note-on/off pairing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub(super) struct NoteEvent {
    pub(super) start_tick: u32,
    pub(super) end_tick: u32,
    pub(super) pitch: u8,
    pub(super) velocity: u8,
}

/// Pair note-ons with matching note-offs by pitch. Unmatched note-ons are
/// closed at the last tick seen (defensive - real DAWs always emit offs).
pub(super) fn collect_note_events(events: &[TimedEvent]) -> Result<Vec<NoteEvent>, Td3Error> {
    let mut open: Vec<(usize, u8, u8, u32)> = Vec::new(); // (note_idx, pitch, vel, start)
    let mut notes: Vec<NoteEvent> = Vec::new();
    let mut last_tick = 0u32;

    for ev in events {
        last_tick = ev.tick;
        match ev.event {
            MidiEvent::NoteOn { pitch, velocity } => {
                // If the same pitch is already open, close it first - some
                // DAWs emit overlapping note-ons when the user re-triggers.
                if let Some(pos) = open.iter().position(|(_, p, _, _)| *p == pitch) {
                    let (idx, _, _, _) = open.remove(pos);
                    notes[idx].end_tick = ev.tick;
                }
                let idx = notes.len();
                notes.push(NoteEvent {
                    start_tick: ev.tick,
                    end_tick: ev.tick, // provisional, set on note-off
                    pitch,
                    velocity,
                });
                open.push((idx, pitch, velocity, ev.tick));
            }
            MidiEvent::NoteOff { pitch } => {
                if let Some(pos) = open.iter().position(|(_, p, _, _)| *p == pitch) {
                    let (idx, _, _, _) = open.remove(pos);
                    notes[idx].end_tick = ev.tick;
                }
                // Unmatched note-off is harmless - just skip.
            }
        }
    }

    // Any still-open notes get closed at the last event tick.
    for (idx, _, _, start) in open {
        notes[idx].end_tick = notes[idx].end_tick.max(start + 1).max(last_tick);
    }

    Ok(notes)
}

// ---------------------------------------------------------------------------
// Grid detection
// ---------------------------------------------------------------------------

/// Pick between `ppqn/4` (straight 1/16 grid) and `ppqn/3` (triplet grid) by
/// summing each note-on's distance to its nearest grid line. The smaller
/// total wins. Falls back to straight 1/16 on ties or ambiguity.
pub(super) fn detect_grid(ppqn: u32, notes: &[NoteEvent]) -> Result<(u32, bool), Td3Error> {
    let straight = grid_fit(ppqn, 4, notes);
    let triplet = grid_fit(ppqn, 3, notes);

    match (straight, triplet) {
        (Some((e_s, t_s)), Some((e_t, t_t))) => {
            // Prefer straight unless triplet fits meaningfully better.
            if e_t * 4 < e_s * 3 {
                Ok((t_t, true))
            } else {
                Ok((t_s, false))
            }
        }
        (Some((_, t_s)), None) => Ok((t_s, false)),
        (None, Some((_, t_t))) => Ok((t_t, true)),
        (None, None) => Err(Td3Error::FormatError(format!(
            "PPQN={} is not divisible by 3 or 4 - cannot build a step grid",
            ppqn
        ))),
    }
}

/// Return (total_error, step_ticks) if `ppqn` divides evenly by `divisor`.
fn grid_fit(ppqn: u32, divisor: u32, notes: &[NoteEvent]) -> Option<(u64, u32)> {
    if divisor == 0 || !ppqn.is_multiple_of(divisor) {
        return None;
    }
    let step_ticks = ppqn / divisor;
    if step_ticks == 0 {
        return None;
    }
    let total_error: u64 = notes
        .iter()
        .map(|n| {
            let m = n.start_tick % step_ticks;
            u64::from(m.min(step_ticks - m))
        })
        .sum();
    Some((total_error, step_ticks))
}

// ---------------------------------------------------------------------------
// Step binning & pattern derivation
// ---------------------------------------------------------------------------

/// For each note event, compute its step index (nearest boundary) and
/// return a Vec<Vec<note_idx>> of length 16 (indices past 15 are dropped).
pub(super) fn bin_to_steps(notes: &[NoteEvent], step_ticks: u32) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = (0..16).map(|_| Vec::new()).collect();
    for (idx, note) in notes.iter().enumerate() {
        let half = step_ticks / 2;
        let step = ((note.start_tick + half) / step_ticks) as usize;
        if step < 16 {
            out[step].push(idx);
        }
        // Notes beyond step 15 are ignored - the TD-3 can only hold 16.
    }
    // Sort each bucket so the polyphony resolver sees a stable pitch-sorted list.
    for bucket in out.iter_mut() {
        bucket.sort();
    }
    out
}

/// Pick a `(step_ticks, triplet)` pair for a .mid that has no note-on
/// events at all - the kind that comes from exporting an all-rest TD-3
/// pattern (legitimate user data: many users park empty patterns between
/// real sequences as recording markers). The note-onset alignment that
/// `detect_grid` relies on is unavailable here, so we use the End-of-Track
/// tick (passed in as `last_tick`) as the only signal.
///
/// Returns `Some((step_ticks, triplet))` when the EOT tick is a clean
/// multiple of either `ppqn/4` (straight) or `ppqn/3` (triplet) for a step
/// count in 1..=16. Straight wins when both fit, matching the TD-3's
/// device default and the most common user setup. Returns `None` when
/// neither grid produces a sensible step count, leaving the caller free
/// to apply its own fallback.
pub(super) fn detect_grid_for_empty(ppqn: u32, last_tick: u32) -> Option<(u32, bool)> {
    let try_grid = |divisor: u32| -> Option<(u32, bool)> {
        if divisor == 0 || !ppqn.is_multiple_of(divisor) {
            return None;
        }
        let step_ticks = ppqn / divisor;
        if step_ticks == 0 || last_tick == 0 || !last_tick.is_multiple_of(step_ticks) {
            return None;
        }
        let n = last_tick / step_ticks;
        if (1..=16).contains(&n) {
            Some((step_ticks, divisor == 3))
        } else {
            None
        }
    };
    try_grid(4).or_else(|| try_grid(3))
}

/// active_steps = max(last_onset_step + 1, round(last_tick / step_ticks)).
///
/// The last_tick term catches trailing silence (ties/rests after the final
/// note-on) that the exporter preserves via the End-of-Track position.
pub(super) fn derive_active_steps(
    steps_by_index: &[Vec<usize>],
    last_tick: u32,
    step_ticks: u32,
) -> u8 {
    let last_onset_plus_one = steps_by_index
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, v)| if !v.is_empty() { Some(i + 1) } else { None })
        .unwrap_or(1);

    let ticks_based = ((last_tick + step_ticks / 2) / step_ticks) as usize;
    let combined = last_onset_plus_one.max(ticks_based).clamp(1, 16);
    combined as u8
}
