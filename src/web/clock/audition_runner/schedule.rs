use crate::error::Td3Error;
use crate::formats::mid::{
    build_timeline, build_timeline_with_gate, build_timeline_with_lanes, MidiExportOptions,
    MidiSlideMode, StepLanes, TimedMidiEvent,
};
use crate::pattern::Pattern;

/// MIDI ticks per quarter note used when laying out the audition
/// schedule. 480 is divisible by both 4 (normal steps) and 3 (triplet
/// steps), so step boundaries land on whole ticks for either timing.
pub(super) const AUDITION_PPQN: u16 = 480;

/// The channel host audition used before it became configurable.
/// `build_timeline` encodes status as `0x90 | (channel - 1)`, so 1 yields
/// the `0x90`/`0x80` bytes earlier releases emitted. Tests name it to
/// assert that the shipped default reproduces that byte stream exactly.
#[cfg(test)]
pub(crate) const DEFAULT_AUDITION_CHANNEL: u8 = 1;

/// Accent velocity. Matches `note_preview` and the `.mid` export default
/// so accented audition notes sound identical to the keyboard preview.
pub(super) const ACCENT_VELOCITY: u8 = 110;

/// Normal (un-accented) velocity. Matches `note_preview` and the `.mid`
/// export default.
pub(super) const NORMAL_VELOCITY: u8 = 78;

/// One scheduled MIDI message: raw bytes plus the offset, in
/// microseconds, from the start of the pattern cycle at which they must
/// be sent. `event_id` is optional scheduler metadata carrying a stable
/// source identity for morph schedules; it is never part of the MIDI
/// bytes. Legacy non-morph schedules leave it absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledMidi {
    pub offset_us: u64,
    pub bytes: Vec<u8>,
    pub event_id: Option<crate::triplet_morph::MorphEventId>,
}

/// A full pattern cycle expressed as wall-clock-scheduled MIDI messages.
/// `cycle_period_us` is the duration of one complete active-step pass;
/// the runner loops on this boundary so the tail silence/sustain of the
/// last step is preserved before the cycle repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditionSchedule {
    pub events: Vec<ScheduledMidi>,
    pub cycle_period_us: u64,
    /// MIDI channel, 1 through 16, that every event in `events` was
    /// encoded on. The runner silences the audition on this channel, so a
    /// note sounded by the schedule is always stopped by a Note Off the
    /// device accepts.
    pub channel: u8,
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
pub(super) fn tick_offset_us(tick: u32, centibpm: u32, ppqn: u16) -> u64 {
    let centibpm = centibpm.max(1) as u64;
    let ppqn = ppqn.max(1) as u64;
    (tick as u64).saturating_mul(6_000_000_000u64) / (centibpm * ppqn)
}

/// Build the audition schedule for `pattern` at `centibpm` on `channel`.
///
/// Uses the legacy 50 percent ordinary-note gate retained by MIDI export.
pub fn prepare_schedule(
    pattern: &Pattern,
    centibpm: u32,
    channel: u8,
) -> Result<AuditionSchedule, Td3Error> {
    let options = audition_options(centibpm, channel);
    let timeline = build_timeline(pattern, "audition", &options)?;
    Ok(prepare_schedule_from_timeline(
        pattern, centibpm, channel, timeline,
    ))
}

/// Build a host-audition schedule with an explicit ordinary-note gate.
/// The default schedule builder and MIDI export retain the legacy gate.
pub(crate) fn prepare_schedule_with_gate(
    pattern: &Pattern,
    centibpm: u32,
    gate_percent: u32,
    channel: u8,
) -> Result<AuditionSchedule, Td3Error> {
    if gate_percent == 50 {
        return prepare_schedule(pattern, centibpm, channel);
    }
    let options = audition_options(centibpm, channel);
    let timeline = build_timeline_with_gate(pattern, "audition", &options, gate_percent)?;
    Ok(prepare_schedule_from_timeline(
        pattern, centibpm, channel, timeline,
    ))
}

/// Build a host-audition schedule with per-step gate and cutoff lanes.
/// With both lanes absent this is `prepare_schedule_with_gate`.
pub(crate) fn prepare_schedule_with_lanes(
    pattern: &Pattern,
    centibpm: u32,
    gate_percent: u32,
    lanes: StepLanes,
    channel: u8,
) -> Result<AuditionSchedule, Td3Error> {
    if lanes.is_empty() {
        return prepare_schedule_with_gate(pattern, centibpm, gate_percent, channel);
    }
    let options = audition_options(centibpm, channel);
    let timeline = build_timeline_with_lanes(pattern, "audition", &options, gate_percent, lanes)?;
    Ok(prepare_schedule_from_timeline(
        pattern, centibpm, channel, timeline,
    ))
}

pub(super) fn audition_options(centibpm: u32, channel: u8) -> MidiExportOptions {
    MidiExportOptions {
        bpm: (centibpm / 100).max(1),
        ppqn: AUDITION_PPQN,
        channel,
        octave_offset: 0,
        accent_velocity: ACCENT_VELOCITY,
        normal_velocity: NORMAL_VELOCITY,
        slide_mode: MidiSlideMode::Td3,
        loop_count: 1,
    }
}

fn prepare_schedule_from_timeline(
    pattern: &Pattern,
    centibpm: u32,
    channel: u8,
    timeline: Vec<TimedMidiEvent>,
) -> AuditionSchedule {
    let mut events: Vec<(u32, u8, ScheduledMidi)> = timeline
        .into_iter()
        .filter(|ev| {
            // Keep Note Off (0x80), Note On (0x90) and Control Change
            // (0xB0) channel-voice messages; every meta event begins
            // with 0xFF.
            matches!(
                ev.data.first().map(|b| b & 0xF0),
                Some(0x80) | Some(0x90) | Some(0xB0)
            )
        })
        .map(|ev| {
            (
                ev.tick,
                ev.order,
                ScheduledMidi {
                    offset_us: tick_offset_us(ev.tick, centibpm, AUDITION_PPQN),
                    bytes: ev.data,
                    event_id: None,
                },
            )
        })
        .collect();

    // Timeline order makes a full-step Note Off precede the following
    // Note On at an identical tick, so the old note cannot cut the new one.
    events.sort_by_key(|(tick, order, _)| (*tick, *order));

    let divisor: u32 = if pattern.triplet { 3 } else { 4 };
    let step_ticks = AUDITION_PPQN as u32 / divisor;
    let pattern_ticks = (pattern.active_steps as u32).max(1) * step_ticks;
    let cycle_period_us = tick_offset_us(pattern_ticks, centibpm, AUDITION_PPQN);

    AuditionSchedule {
        events: events.into_iter().map(|(_, _, ev)| ev).collect(),
        cycle_period_us,
        channel,
    }
}
