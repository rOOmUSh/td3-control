//! Unit tests for the host-sequenced audition schedule builder
//! (`crate::web::clock::prepare_schedule`). These cover tick-to-microsecond
//! conversion, event filtering, gate length, accent velocity, rest handling,
//! triplet timing, and consistency with the single-note keyboard preview.

use std::collections::BTreeSet;

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};
use crate::web::api_types::NotePreviewRequest;
use crate::web::clock::{
    prepare_schedule, send_due_events_until_update_boundary, AuditionSchedule, DueEventsResult,
    ScheduledMidi,
};

const CENTIBPM_120: u32 = 12_000;

/// All 16 steps share `step`; `active_steps` and `triplet` configurable.
fn uniform_pattern(step: Step, active_steps: u8, triplet: bool) -> Pattern {
    Pattern::new(triplet, active_steps, [step; 16]).expect("valid test pattern")
}

fn normal_c() -> Step {
    Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal)
}

#[test]
fn schedule_emits_note_on_off_per_active_step() {
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();

    // 16 normal steps -> 16 Note On + 16 Note Off.
    assert_eq!(schedule.events.len(), 32, "16 on + 16 off");

    let note_ons = schedule
        .events
        .iter()
        .filter(|e| e.bytes[0] & 0xF0 == 0x90 && e.bytes[2] > 0)
        .count();
    let note_offs = schedule
        .events
        .iter()
        .filter(|e| e.bytes[0] & 0xF0 == 0x80)
        .count();
    assert_eq!(note_ons, 16);
    assert_eq!(note_offs, 16);
}

#[test]
fn schedule_drops_meta_events() {
    // Every retained event must be a channel-voice Note On/Off; no 0xFF meta
    // (track name, tempo, time signature, end-of-track) survives the filter.
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    for ev in &schedule.events {
        let status = ev.bytes[0] & 0xF0;
        assert!(
            status == 0x80 || status == 0x90,
            "non-note event leaked: {:02X?}",
            ev.bytes
        );
    }
}

#[test]
fn schedule_timing_matches_tempo_at_120bpm() {
    // 16 sixteenth notes at 120 BPM span 4 beats = 2.0 s.
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    assert_eq!(schedule.cycle_period_us, 2_000_000);

    // First Note On fires at offset 0; step interval is 125 ms.
    let first = &schedule.events[0];
    assert_eq!(first.offset_us, 0);
    assert_eq!(first.bytes[0] & 0xF0, 0x90);

    // The second Note On is one step (125 ms) later.
    let second_on = schedule
        .events
        .iter()
        .filter(|e| e.bytes[0] & 0xF0 == 0x90 && e.bytes[2] > 0)
        .nth(1)
        .unwrap();
    assert_eq!(second_on.offset_us, 125_000);
}

#[test]
fn schedule_half_step_gate_for_normal_notes() {
    // A normal (non-slide) note releases half a step after onset: 62.5 ms at
    // 120 BPM sixteenths.
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    let first_off = schedule
        .events
        .iter()
        .find(|e| e.bytes[0] & 0xF0 == 0x80)
        .unwrap();
    assert_eq!(first_off.offset_us, 62_500);
}

#[test]
fn schedule_note_byte_matches_keyboard_preview() {
    // The audition note byte for C/NORMAL must equal the single-note keyboard
    // preview's midi_note(), so sequenced audition and keyboard preview agree.
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    let first_on = &schedule.events[0];

    let preview = NotePreviewRequest {
        note: "C".to_string(),
        transpose: "NORMAL".to_string(),
        accent: false,
    };
    assert_eq!(first_on.bytes[1], preview.midi_note().unwrap());
    assert_eq!(first_on.bytes[1], 36);
    assert_eq!(first_on.bytes[2], 78, "normal velocity");
    assert_eq!(first_on.bytes[0], 0x90, "channel 1 note on");
}

#[test]
fn schedule_accent_uses_high_velocity() {
    let accented = Step::new(0, Transpose::Normal, Accent::On, Slide::Off, Time::Normal);
    let pattern = uniform_pattern(accented, 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    let first_on = &schedule.events[0];
    assert_eq!(first_on.bytes[2], 110, "accent velocity");
}

#[test]
fn schedule_rest_steps_produce_no_notes() {
    let rest = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);
    let pattern = uniform_pattern(rest, 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    assert!(
        schedule.events.is_empty(),
        "all-rest pattern emits no note events"
    );
    // The cycle still spans the full active-step duration so a looping
    // audition keeps tempo through the silence.
    assert_eq!(schedule.cycle_period_us, 2_000_000);
}

#[test]
fn schedule_triplet_timing_shortens_cycle() {
    // Triplet steps are 1/3-of-a-beat wide instead of 1/4, so 16 of them span
    // 16/3 beats = 2.6667 s at 120 BPM.
    let pattern = uniform_pattern(normal_c(), 16, true);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    assert_eq!(schedule.cycle_period_us, 2_666_666);
}

#[test]
fn schedule_active_steps_shortens_cycle() {
    // Only the active steps are sequenced; 8 active steps at 120 BPM span
    // 2 beats = 1.0 s.
    let pattern = uniform_pattern(normal_c(), 8, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    assert_eq!(schedule.cycle_period_us, 1_000_000);
    assert_eq!(schedule.events.len(), 16, "8 on + 8 off");
}

#[test]
fn schedule_events_sorted_by_offset() {
    // Events must be in non-decreasing offset order so the runner can play
    // them sequentially.
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120).unwrap();
    let mut prev = 0u64;
    for ev in &schedule.events {
        assert!(ev.offset_us >= prev, "offsets must be non-decreasing");
        prev = ev.offset_us;
    }
}

#[test]
fn schedule_fractional_bpm_resolves() {
    // 120.50 BPM (centi-BPM 12050) must not divide by zero or panic, and the
    // cycle must be slightly shorter than the 120.00 BPM cycle.
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, 12_050).unwrap();
    assert!(schedule.cycle_period_us > 0);
    assert!(schedule.cycle_period_us < 2_000_000);
}

#[test]
fn pending_update_waits_for_due_note_off_before_replacing_boundary() {
    let schedule = AuditionSchedule {
        events: vec![
            ScheduledMidi {
                offset_us: 100,
                bytes: vec![0x80, 36, 64],
            },
            ScheduledMidi {
                offset_us: 100,
                bytes: vec![0x90, 38, 78],
            },
        ],
        cycle_period_us: 1_000,
    };
    let mut next_event = 0usize;
    let mut sounding = BTreeSet::from([36u8]);
    let mut sent: Vec<Vec<u8>> = Vec::new();

    let result = send_due_events_until_update_boundary(
        &schedule,
        &mut next_event,
        &mut sounding,
        100,
        true,
        |bytes| {
            sent.push(bytes.to_vec());
            Ok::<(), Td3Error>(())
        },
    )
    .unwrap();

    assert_eq!(result, DueEventsResult::ApplyPendingUpdate);
    assert_eq!(sent, vec![vec![0x80, 36, 64]]);
    assert_eq!(next_event, 1);
    assert!(sounding.is_empty());
}
