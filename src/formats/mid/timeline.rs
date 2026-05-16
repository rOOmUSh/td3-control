use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

use super::events::{
    note_off_event, note_on_event, tempo_meta_event, time_signature_meta_event,
    track_name_meta_event, TimedMidiEvent, ORDER_END_OF_TRACK, ORDER_META, ORDER_NOTE_OFF,
    ORDER_NOTE_ON, ORDER_SLIDE_NOTE_OFF, ORDER_SLIDE_NOTE_ON,
};
use super::note::{midi_note_number, velocity_for_step};
use super::options::{MidiExportOptions, MidiSlideMode};
use super::timing::{has_slide_connection, step_ticks};

#[derive(Debug, Clone, Copy)]
struct SoundingNote {
    note: u8,
}

pub(crate) fn build_timeline(
    pattern: &Pattern,
    address: &str,
    options: &MidiExportOptions,
) -> Result<Vec<TimedMidiEvent>, Td3Error> {
    let step_ticks = step_ticks(pattern.triplet, options.ppqn)?;
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
                            group_end_tick + step_ticks / 2
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
