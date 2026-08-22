use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

use super::events::{
    control_change_event, note_off_event, note_on_event, tempo_meta_event,
    time_signature_meta_event, track_name_meta_event, TimedMidiEvent, ORDER_CONTROL_CHANGE,
    ORDER_END_OF_TRACK, ORDER_META, ORDER_NOTE_OFF, ORDER_NOTE_ON, ORDER_SLIDE_NOTE_OFF,
    ORDER_SLIDE_NOTE_ON,
};
use super::note::{midi_note_number, velocity_for_step};
use super::options::{MidiExportOptions, MidiSlideMode};
use super::timing::{has_slide_connection, step_ticks};

#[derive(Debug, Clone, Copy)]
struct SoundingNote {
    note: u8,
}

/// Filter Cutoff controller number.
pub(crate) const FILTER_CUTOFF_CC: u8 = 74;

/// Optional per-step overrides applied on top of a pattern while a
/// timeline is built. Each array is indexed by step position.
///
/// `gates`: ordinary-note gate per step, 1 through 100 percent of one
/// step, read at the step that starts a note group. `None` keeps the
/// pattern-wide gate for every step.
///
/// `cutoffs`: a Control Change 74 value, 0 through 127, emitted at the
/// start of every active step, rests and ties included, so the filter
/// position follows the step grid regardless of note content. `None`
/// emits no Control Change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepLanes {
    pub gates: Option<[u32; step::Step::COUNT]>,
    pub cutoffs: Option<[u8; step::Step::COUNT]>,
}

impl StepLanes {
    pub fn is_empty(&self) -> bool {
        self.gates.is_none() && self.cutoffs.is_none()
    }
}

#[derive(Debug, Clone, Copy)]
enum OrdinaryGate {
    LegacyHalfStep,
    GatePercent(u32),
}

pub(crate) fn build_timeline(
    pattern: &Pattern,
    address: &str,
    options: &MidiExportOptions,
) -> Result<Vec<TimedMidiEvent>, Td3Error> {
    build_timeline_inner(
        pattern,
        address,
        options,
        OrdinaryGate::LegacyHalfStep,
        StepLanes::default(),
    )
}

pub(crate) fn build_timeline_with_gate(
    pattern: &Pattern,
    address: &str,
    options: &MidiExportOptions,
    gate_percent: u32,
) -> Result<Vec<TimedMidiEvent>, Td3Error> {
    build_timeline_inner(
        pattern,
        address,
        options,
        OrdinaryGate::GatePercent(gate_percent),
        StepLanes::default(),
    )
}

/// Like `build_timeline_with_gate`, with per-step gate and cutoff lanes.
/// A step whose lane gate is absent uses `gate_percent`.
pub(crate) fn build_timeline_with_lanes(
    pattern: &Pattern,
    address: &str,
    options: &MidiExportOptions,
    gate_percent: u32,
    lanes: StepLanes,
) -> Result<Vec<TimedMidiEvent>, Td3Error> {
    build_timeline_inner(
        pattern,
        address,
        options,
        OrdinaryGate::GatePercent(gate_percent),
        lanes,
    )
}

fn build_timeline_inner(
    pattern: &Pattern,
    address: &str,
    options: &MidiExportOptions,
    ordinary_gate: OrdinaryGate,
    lanes: StepLanes,
) -> Result<Vec<TimedMidiEvent>, Td3Error> {
    let step_ticks = step_ticks(pattern.triplet, options.ppqn)?;
    let pattern_gate_ticks = match ordinary_gate {
        OrdinaryGate::LegacyHalfStep => step_ticks / 2,
        OrdinaryGate::GatePercent(gate_percent) => gate_ticks(step_ticks, gate_percent),
    };
    let gate_ticks_for_step = |i: usize| match lanes.gates {
        Some(gates) => gate_ticks(step_ticks, gates[i]),
        None => pattern_gate_ticks,
    };
    let mut events = vec![
        TimedMidiEvent {
            tick: 0,
            order: ORDER_META,
            data: track_name_meta_event(address),
        },
        TimedMidiEvent {
            tick: 0,
            order: ORDER_META,
            data: tempo_meta_event(options.bpm),
        },
        TimedMidiEvent {
            tick: 0,
            order: ORDER_META,
            data: time_signature_meta_event(),
        },
    ];

    let total_steps = pattern.active_steps as usize;
    let pattern_ticks = (pattern.active_steps as u32) * step_ticks;

    for loop_index in 0..options.loop_count {
        let tick_offset = loop_index * pattern_ticks;
        let mut sounding: Option<SoundingNote> = None;

        for i in 0..total_steps {
            let tick = tick_offset + (i as u32) * step_ticks;
            let s = &pattern.step[i];

            if let Some(cutoffs) = lanes.cutoffs {
                events.push(TimedMidiEvent {
                    tick,
                    order: ORDER_CONTROL_CHANGE,
                    data: control_change_event(options.channel, FILTER_CUTOFF_CC, cutoffs[i]),
                });
            }

            match s.time {
                step::Time::Tie | step::Time::Rest | step::Time::TieRest => {}
                step::Time::Normal => {
                    let next_note = midi_note_number(s, options.octave_offset)?;
                    let velocity = velocity_for_step(s, options);

                    let mut group_end = i;
                    while group_end + 1 < total_steps
                        && pattern.step[group_end + 1].time == step::Time::Tie
                    {
                        group_end += 1;
                    }

                    let slide_on = s.slide == step::Slide::On;
                    let connects_to_next_normal = slide_on
                        && options.slide_mode == MidiSlideMode::Td3
                        && group_end + 1 < total_steps
                        && pattern.step[group_end + 1].time == step::Time::Normal;

                    let connected_from_prev = if has_slide_connection(pattern, i) {
                        sounding.as_ref().map(|current| current.note)
                    } else {
                        None
                    };

                    if let Some(current_note) = connected_from_prev {
                        if current_note != next_note {
                            if options.slide_mode == MidiSlideMode::Td3 {
                                events.push(TimedMidiEvent {
                                    tick,
                                    order: ORDER_SLIDE_NOTE_ON,
                                    data: note_on_event(options.channel, next_note, velocity),
                                });
                                events.push(TimedMidiEvent {
                                    tick: tick + step_ticks / 8,
                                    order: ORDER_SLIDE_NOTE_OFF,
                                    data: note_off_event(options.channel, current_note),
                                });
                            } else {
                                events.push(TimedMidiEvent {
                                    tick,
                                    order: ORDER_NOTE_OFF,
                                    data: note_off_event(options.channel, current_note),
                                });
                                events.push(TimedMidiEvent {
                                    tick,
                                    order: ORDER_NOTE_ON,
                                    data: note_on_event(options.channel, next_note, velocity),
                                });
                            }
                        }
                    } else {
                        events.push(TimedMidiEvent {
                            tick,
                            order: ORDER_NOTE_ON,
                            data: note_on_event(options.channel, next_note, velocity),
                        });
                    }

                    if connects_to_next_normal {
                        sounding = Some(SoundingNote { note: next_note });
                    } else {
                        let group_end_tick = tick_offset + (group_end as u32) * step_ticks;
                        let release_tick = if slide_on {
                            group_end_tick + step_ticks
                        } else {
                            group_end_tick + gate_ticks_for_step(i)
                        };
                        events.push(TimedMidiEvent {
                            tick: release_tick,
                            order: ORDER_NOTE_OFF,
                            data: note_off_event(options.channel, next_note),
                        });
                        sounding = None;
                    }
                }
            }
        }

        let loop_end_tick = tick_offset + pattern_ticks;
        if let Some(current) = sounding.take() {
            events.push(TimedMidiEvent {
                tick: loop_end_tick,
                order: ORDER_NOTE_OFF,
                data: note_off_event(options.channel, current.note),
            });
        }
    }

    let total_ticks = pattern_ticks * options.loop_count;
    events.push(TimedMidiEvent {
        tick: total_ticks,
        order: ORDER_END_OF_TRACK,
        data: vec![0xFF, 0x2F, 0x00],
    });

    Ok(events)
}

fn gate_ticks(step_ticks: u32, gate_percent: u32) -> u32 {
    let step_ticks = u64::from(step_ticks);
    let rounded = (step_ticks * u64::from(gate_percent) + 50) / 100;
    rounded.clamp(1, step_ticks) as u32
}
