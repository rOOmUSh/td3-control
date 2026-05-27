use crate::error::Td3Error;
use crate::formats::mid::{build_timeline, MidiExportOptions, MidiSlideMode};
use crate::pattern::Pattern;

/// MIDI ticks per quarter note used when laying out the audition
/// schedule. 480 is divisible by both 4 (normal steps) and 3 (triplet
/// steps), so step boundaries land on whole ticks for either timing.
const AUDITION_PPQN: u16 = 480;

/// Channel-voice MIDI channel 1 (status nibble target). `build_timeline`
/// encodes status as `0x90 | (channel - 1)`, so channel 1 yields the
/// `0x90`/`0x80` bytes the single-note preview uses.
const AUDITION_CHANNEL: u8 = 1;

/// Accent velocity. Matches `note_preview` and the `.mid` export default
/// so accented audition notes sound identical to the keyboard preview.
const ACCENT_VELOCITY: u8 = 110;

/// Normal (un-accented) velocity. Matches `note_preview` and the `.mid`
/// export default.
const NORMAL_VELOCITY: u8 = 78;

/// One scheduled MIDI message: raw bytes plus the offset, in
/// microseconds, from the start of the pattern cycle at which they must
/// be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledMidi {
    pub offset_us: u64,
    pub bytes: Vec<u8>,
}

/// A full pattern cycle expressed as wall-clock-scheduled MIDI messages.
/// `cycle_period_us` is the duration of one complete active-step pass;
/// the runner loops on this boundary so the tail silence/sustain of the
/// last step is preserved before the cycle repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditionSchedule {
    pub events: Vec<ScheduledMidi>,
    pub cycle_period_us: u64,
}

/// Microsecond offset of MIDI `tick` at `centibpm` (BPM x 100) and the
/// fixed audition PPQN. The multiply happens before the divide so each
/// offset carries full precision instead of accumulating per-tick
/// rounding error.
///
/// Derivation: a quarter note lasts `60_000_000 / bpm` microseconds and
/// spans `ppqn` ticks, so one tick is `60_000_000 / (bpm * ppqn)`.
/// Substituting `bpm = centibpm / 100`:
///     offset = tick * 60_000_000 * 100 / (centibpm * ppqn)
///            = tick * 6_000_000_000 / (centibpm * ppqn).
fn tick_offset_us(tick: u32, centibpm: u32, ppqn: u16) -> u64 {
    let centibpm = centibpm.max(1) as u64;
    let ppqn = ppqn.max(1) as u64;
    (tick as u64).saturating_mul(6_000_000_000u64) / (centibpm * ppqn)
}

/// Build the audition schedule for `pattern` at `centibpm`.
///
/// Reuses the `.mid` export timeline, then keeps only channel-voice
/// Note On/Off events (status nibble `0x80`/`0x90`), discarding the
/// track-name, tempo, time-signature, and end-of-track meta events.
/// Events are converted from MIDI ticks to microsecond offsets and
/// stably sorted by tick so that at an identical tick a Note Off is
/// emitted before a Note On (the timeline builds them in that order).
pub fn prepare_schedule(pattern: &Pattern, centibpm: u32) -> Result<AuditionSchedule, Td3Error> {
    let options = MidiExportOptions {
        bpm: (centibpm / 100).max(1),
        ppqn: AUDITION_PPQN,
        channel: AUDITION_CHANNEL,
        octave_offset: 0,
        accent_velocity: ACCENT_VELOCITY,
        normal_velocity: NORMAL_VELOCITY,
        slide_mode: MidiSlideMode::Td3,
        loop_count: 1,
    };

    let timeline = build_timeline(pattern, "audition", &options)?;

    let mut events: Vec<(u32, ScheduledMidi)> = timeline
        .into_iter()
        .filter(|ev| {
            // Keep only Note Off (0x80) / Note On (0x90) channel-voice
            // messages; every meta event begins with 0xFF.
            matches!(ev.data.first().map(|b| b & 0xF0), Some(0x80) | Some(0x90))
        })
        .map(|ev| {
            (
                ev.tick,
                ScheduledMidi {
                    offset_us: tick_offset_us(ev.tick, centibpm, AUDITION_PPQN),
                    bytes: ev.data,
                },
            )
        })
        .collect();

    // Stable sort by tick preserves the timeline's build order at equal
    // ticks (Note Off pushed before the next step's Note On), so a note
    // ending exactly when the next begins never cuts the new note.
    events.sort_by_key(|(tick, _)| *tick);

    let divisor: u32 = if pattern.triplet { 3 } else { 4 };
    let step_ticks = AUDITION_PPQN as u32 / divisor;
    let pattern_ticks = (pattern.active_steps as u32).max(1) * step_ticks;
    let cycle_period_us = tick_offset_us(pattern_ticks, centibpm, AUDITION_PPQN);

    Ok(AuditionSchedule {
        events: events.into_iter().map(|(_, ev)| ev).collect(),
        cycle_period_us,
    })
}
