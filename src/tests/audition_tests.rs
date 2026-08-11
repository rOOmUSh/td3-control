//! Unit tests for the host-sequenced audition schedule builder
//! (`crate::web::clock::prepare_schedule`). These cover tick-to-microsecond
//! conversion, event filtering, gate length, accent velocity, rest handling,
//! triplet timing, and consistency with the single-note keyboard preview.

use std::collections::BTreeSet;

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};
use crate::web::api_types::{NotePreviewRequest, PatternAuditionResponse};
use crate::web::clock::{
    prepare_schedule, prepare_schedule_with_gate, scale_cycle_phase, schedule_update_timing,
    send_due_events_until_update_boundary, AuditionApplyMode, AuditionSchedule, DueEventsResult,
    ScheduledMidi, DEFAULT_AUDITION_CHANNEL,
};

const CENTIBPM_120: u32 = 12_000;

#[test]
fn tempo_update_preserves_normalized_cycle_phase() {
    assert_eq!(scale_cycle_phase(500_000, 2_000_000, 1_000_000), 250_000);
    assert_eq!(scale_cycle_phase(250_000, 1_000_000, 2_000_000), 500_000);
    assert_eq!(scale_cycle_phase(2_500_000, 2_000_000, 1_000_000), 250_000);
}

#[test]
fn tempo_update_timing_reports_effective_cycle_epoch() {
    let timing = schedule_update_timing(
        24_000,
        7,
        10_250_000,
        250_000,
        1_000_000,
        AuditionApplyMode::CurrentCycleFuture,
    );

    assert_eq!(timing.centibpm, 24_000);
    assert_eq!(timing.schedule_generation, 7);
    assert_eq!(timing.effective_at_epoch_micros, 10_250_000);
    assert_eq!(timing.cycle_epoch_micros, 10_000_000);
    assert_eq!(timing.cycle_period_micros, 1_000_000);
    assert_eq!(timing.phase_micros, 250_000);
}

#[test]
fn audition_response_serializes_applied_timing_as_camel_case() {
    let response = PatternAuditionResponse {
        ok: true,
        bpm: 240,
        centibpm: 24_000,
        looping: true,
        schedule_generation: Some(7),
        effective_at_epoch_micros: Some(10_250_000),
        cycle_epoch_micros: Some(10_000_000),
        cycle_period_micros: Some(1_000_000),
        phase_micros: Some(250_000),
        triplet_morph: None,
    };

    let json = serde_json::to_value(response).unwrap();
    assert_eq!(json["effectiveAtEpochMicros"], 10_250_000);
    assert_eq!(json["cycleEpochMicros"], 10_000_000);
    assert_eq!(json["cyclePeriodMicros"], 1_000_000);
    assert_eq!(json["phaseMicros"], 250_000);
    assert_eq!(json["scheduleGeneration"], 7);
    assert!(json.get("effective_at_epoch_micros").is_none());
    assert!(json.get("gatePercent").is_none());
    assert!(
        json.get("tripletMorphPercent").is_none(),
        "legacy responses omit morph diagnostics"
    );
}

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
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();

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
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
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
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
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
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
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
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
    let first_on = &schedule.events[0];

    let preview = NotePreviewRequest {
        note: "C".to_string(),
        transpose: "NORMAL".to_string(),
        accent: false,
        midi_channel: None,
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
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
    let first_on = &schedule.events[0];
    assert_eq!(first_on.bytes[2], 110, "accent velocity");
}

#[test]
fn schedule_rest_steps_produce_no_notes() {
    let rest = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);
    let pattern = uniform_pattern(rest, 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
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
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
    assert_eq!(schedule.cycle_period_us, 2_666_666);
}

#[test]
fn schedule_active_steps_shortens_cycle() {
    // Only the active steps are sequenced; 8 active steps at 120 BPM span
    // 2 beats = 1.0 s.
    let pattern = uniform_pattern(normal_c(), 8, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
    assert_eq!(schedule.cycle_period_us, 1_000_000);
    assert_eq!(schedule.events.len(), 16, "8 on + 8 off");
}

#[test]
fn schedule_events_sorted_by_offset() {
    // Events must be in non-decreasing offset order so the runner can play
    // them sequentially.
    let pattern = uniform_pattern(normal_c(), 16, false);
    let schedule = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap();
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
    let schedule = prepare_schedule(&pattern, 12_050, DEFAULT_AUDITION_CHANNEL).unwrap();
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
                event_id: None,
            },
            ScheduledMidi {
                offset_us: 100,
                bytes: vec![0x90, 38, 78],
                event_id: None,
            },
        ],
        cycle_period_us: 1_000,
        channel: DEFAULT_AUDITION_CHANNEL,
    };
    let mut next_event = 0usize;
    let mut sounding = BTreeSet::from([36u8]);
    let mut ledger = BTreeSet::new();
    let mut sent: Vec<Vec<u8>> = Vec::new();

    let result = send_due_events_until_update_boundary(
        &schedule,
        &mut next_event,
        &mut sounding,
        &mut ledger,
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

#[test]
fn schedule_gate_offsets_follow_straight_and_triplet_steps() {
    let cases = [
        (false, [(25, 31_250), (50, 62_500), (100, 125_000)]),
        (true, [(25, 41_666), (50, 83_333), (100, 166_666)]),
    ];

    for (triplet, expected_offsets) in cases {
        let pattern = uniform_pattern(normal_c(), 2, triplet);
        let mut cycle_period_us = None;
        for (gate_percent, expected_offset_us) in expected_offsets {
            let schedule = prepare_schedule_with_gate(
                &pattern,
                CENTIBPM_120,
                gate_percent,
                DEFAULT_AUDITION_CHANNEL,
            )
            .unwrap();
            let first_off = schedule
                .events
                .iter()
                .find(|event| event.bytes[0] & 0xF0 == 0x80)
                .unwrap();

            assert_eq!(first_off.offset_us, expected_offset_us);
            if let Some(expected_cycle_period_us) = cycle_period_us {
                assert_eq!(schedule.cycle_period_us, expected_cycle_period_us);
            } else {
                cycle_period_us = Some(schedule.cycle_period_us);
            }
        }
    }
}

#[test]
fn schedule_gate_rounds_to_nearest_midi_tick() {
    const CENTIBPM_125: u32 = 12_500;
    let cases = [(false, 40_000), (true, 53_000)];

    for (triplet, expected_offset_us) in cases {
        let pattern = uniform_pattern(normal_c(), 2, triplet);
        let schedule =
            prepare_schedule_with_gate(&pattern, CENTIBPM_125, 33, DEFAULT_AUDITION_CHANNEL)
                .unwrap();
        let first_off = schedule
            .events
            .iter()
            .find(|event| event.bytes[0] & 0xF0 == 0x80)
            .unwrap();
        assert_eq!(first_off.offset_us, expected_offset_us);
    }
}

#[test]
fn schedule_gate_applies_to_final_tied_step_tail() {
    let rest = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);
    let mut steps = [rest; 16];
    steps[0] = normal_c();
    steps[1] = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Tie);
    let pattern = Pattern::new(false, 2, steps).unwrap();

    for (gate_percent, expected_offset_us) in [(25, 156_250), (50, 187_500), (100, 250_000)] {
        let schedule = prepare_schedule_with_gate(
            &pattern,
            CENTIBPM_120,
            gate_percent,
            DEFAULT_AUDITION_CHANNEL,
        )
        .unwrap();
        let note_offs: Vec<_> = schedule
            .events
            .iter()
            .filter(|event| event.bytes[0] & 0xF0 == 0x80)
            .collect();

        assert_eq!(note_offs.len(), 1);
        assert_eq!(note_offs[0].offset_us, expected_offset_us);
        assert_eq!(schedule.cycle_period_us, 250_000);
    }
}

#[test]
fn default_schedule_retains_legacy_50_percent_gate() {
    let pattern = uniform_pattern(normal_c(), 16, false);
    assert_eq!(
        prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL).unwrap(),
        prepare_schedule_with_gate(&pattern, CENTIBPM_120, 50, DEFAULT_AUDITION_CHANNEL).unwrap()
    );
}

#[test]
fn schedule_gate_preserves_rests_and_accent_velocity() {
    let rest = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);
    let rest_pattern = uniform_pattern(rest, 4, false);
    let accented = Step::new(0, Transpose::Normal, Accent::On, Slide::Off, Time::Normal);
    let accented_pattern = uniform_pattern(accented, 4, false);

    for gate_percent in [25, 50, 100] {
        let rest_schedule = prepare_schedule_with_gate(
            &rest_pattern,
            CENTIBPM_120,
            gate_percent,
            DEFAULT_AUDITION_CHANNEL,
        )
        .unwrap();
        assert!(rest_schedule.events.is_empty());
        assert_eq!(rest_schedule.cycle_period_us, 500_000);

        let accented_schedule = prepare_schedule_with_gate(
            &accented_pattern,
            CENTIBPM_120,
            gate_percent,
            DEFAULT_AUDITION_CHANNEL,
        )
        .unwrap();
        let first_on = accented_schedule
            .events
            .iter()
            .find(|event| event.bytes[0] & 0xF0 == 0x90 && event.bytes[2] > 0)
            .unwrap();
        assert_eq!(first_on.bytes[2], 110);
    }
}

#[test]
fn schedule_gate_preserves_terminal_and_connected_slide_timing() {
    let rest = Step::new(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);

    let mut terminal_steps = [rest; 16];
    terminal_steps[0] = Step::new(0, Transpose::Normal, Accent::Off, Slide::On, Time::Normal);
    let terminal_pattern = Pattern::new(false, 1, terminal_steps).unwrap();

    let mut connected_steps = [rest; 16];
    connected_steps[0] = Step::new(0, Transpose::Normal, Accent::Off, Slide::On, Time::Normal);
    connected_steps[1] = Step::new(2, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    let connected_pattern = Pattern::new(false, 2, connected_steps).unwrap();

    for gate_percent in [25, 50, 100] {
        let terminal_schedule = prepare_schedule_with_gate(
            &terminal_pattern,
            CENTIBPM_120,
            gate_percent,
            DEFAULT_AUDITION_CHANNEL,
        )
        .unwrap();
        let terminal_off = terminal_schedule
            .events
            .iter()
            .find(|event| event.bytes[0] & 0xF0 == 0x80)
            .unwrap();
        assert_eq!(terminal_off.offset_us, 125_000);

        let connected_schedule = prepare_schedule_with_gate(
            &connected_pattern,
            CENTIBPM_120,
            gate_percent,
            DEFAULT_AUDITION_CHANNEL,
        )
        .unwrap();
        let old_note_off = connected_schedule
            .events
            .iter()
            .find(|event| event.bytes[0] & 0xF0 == 0x80 && event.bytes[1] == 36)
            .unwrap();
        assert_eq!(old_note_off.offset_us, 140_625);
    }
}

#[test]
fn full_step_note_off_sorts_before_following_note_on() {
    let pattern = uniform_pattern(normal_c(), 2, false);
    let schedule =
        prepare_schedule_with_gate(&pattern, CENTIBPM_120, 100, DEFAULT_AUDITION_CHANNEL).unwrap();
    let boundary_statuses: Vec<_> = schedule
        .events
        .iter()
        .filter(|event| event.offset_us == 125_000)
        .map(|event| event.bytes[0] & 0xF0)
        .collect();

    assert_eq!(boundary_statuses, vec![0x80, 0x90]);
}
