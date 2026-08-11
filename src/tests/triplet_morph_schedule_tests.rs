//! Exact schedule tests for the morph-aware audition builder.

use crate::error::Td3Error;
use crate::step::{Accent, Slide, Time};
use crate::triplet_morph::{MorphAmount, MorphEventId, MorphEventRole};
use crate::web::clock::{
    prepare_morph_schedule, prepare_schedule_with_gate, ScheduledMidi,
    COLLISION_RETIREMENT_AMOUNT_PERCENT, DEFAULT_AUDITION_CHANNEL, GATE_COMPENSATION_PEAK_DEN,
    GATE_COMPENSATION_PEAK_NUM, GATE_COMPENSATION_PEAK_PERCENT,
};

use super::fixtures::straight_sixteen;

const CENTIBPM_120: u32 = 12_000;
const CYCLE_120_US: u64 = 2_000_000;

fn amount(value: u32) -> MorphAmount {
    MorphAmount::new(value).expect("amount in range")
}

fn is_note_on(event: &ScheduledMidi) -> bool {
    matches!(event.bytes.first().map(|b| b & 0xF0), Some(0x90))
        && event.bytes.get(2).copied().unwrap_or(0) > 0
}

fn is_note_off(event: &ScheduledMidi) -> bool {
    match event.bytes.first().map(|b| b & 0xF0) {
        Some(0x80) => true,
        Some(0x90) => event.bytes.get(2).copied().unwrap_or(0) == 0,
        _ => false,
    }
}

fn note_on_offset(events: &[ScheduledMidi], source_step: u8) -> Option<u64> {
    events
        .iter()
        .find(|event| {
            is_note_on(event)
                && event.event_id
                    == Some(MorphEventId {
                        source_step,
                        role: MorphEventRole::NoteOn,
                    })
        })
        .map(|event| event.offset_us)
}

fn note_off_offset(events: &[ScheduledMidi], source_step: u8) -> Option<u64> {
    events
        .iter()
        .find(|event| {
            event.event_id
                == Some(MorphEventId {
                    source_step,
                    role: MorphEventRole::NoteOff,
                })
        })
        .map(|event| event.offset_us)
}

// ---------------------------------------------------------------------------
// Cycle and anchor timing
// ---------------------------------------------------------------------------

#[test]
fn every_amount_keeps_a_two_second_cycle_at_120_bpm() {
    let pattern = straight_sixteen();
    for raw in 0..=100u32 {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        assert_eq!(schedule.cycle_period_us, CYCLE_120_US, "amount {}", raw);
        for event in &schedule.events {
            assert!(
                event.offset_us <= schedule.cycle_period_us,
                "event past cycle at amount {}",
                raw
            );
            if is_note_on(event) {
                assert!(
                    event.offset_us < schedule.cycle_period_us,
                    "note on leaked to the boundary at amount {}",
                    raw
                );
            }
        }
    }
}

#[test]
fn beat_anchors_never_move() {
    let pattern = straight_sixteen();
    for raw in [0u32, 25, 50, 75, 99, 100] {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        for (beat, anchor_step) in [(0u64, 0u8), (1, 4), (2, 8), (3, 12)] {
            assert_eq!(
                note_on_offset(&schedule.events, anchor_step),
                Some(beat * 500_000),
                "anchor step {} at amount {}",
                anchor_step,
                raw
            );
        }
    }
}

#[test]
fn default_fifty_percent_first_beat_positions_match_the_specification() {
    let pattern = straight_sixteen();
    let (schedule, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(50),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    // 7/24, 7/12 and 17/24 beat at 500 ms per beat, truncated. Halfway
    // is below the retirement threshold of
    // COLLISION_RETIREMENT_AMOUNT_PERCENT, so all four cells still sound
    // at their own warped positions with nothing absorbed.
    assert_eq!(note_on_offset(&schedule.events, 1), Some(145_833));
    assert_eq!(note_on_offset(&schedule.events, 2), Some(291_666));
    assert_eq!(note_on_offset(&schedule.events, 3), Some(354_166));
}

#[test]
fn endpoint_has_at_most_three_attacks_per_beat() {
    let pattern = straight_sixteen();
    let (schedule, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(100),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    for beat in 0..4u64 {
        let beat_start = beat * 500_000;
        let beat_end = beat_start + 500_000;
        let attacks = schedule
            .events
            .iter()
            .filter(|event| {
                is_note_on(event) && event.offset_us >= beat_start && event.offset_us < beat_end
            })
            .count();
        assert!(attacks <= 3, "beat {} has {} attacks", beat, attacks);
    }
    let total_ons = schedule.events.iter().filter(|e| is_note_on(e)).count();
    assert_eq!(
        total_ons, 12,
        "default endpoint keeps three attacks per beat"
    );
}

// ---------------------------------------------------------------------------
// Zero-amount equivalence with the legacy straight path
// ---------------------------------------------------------------------------

#[test]
fn zero_amount_matches_the_legacy_straight_schedule_bytes_and_offsets() {
    let mut pattern = straight_sixteen();
    pattern.step[2].accent = Accent::On;
    pattern.step[5].time = Time::Tie;
    pattern.step[9].slide = Slide::On;
    pattern.step[11].time = Time::Rest;

    for gate in [1u32, 50, 100] {
        let legacy =
            prepare_schedule_with_gate(&pattern, CENTIBPM_120, gate, DEFAULT_AUDITION_CHANNEL)
                .expect("legacy");
        let (morph, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            gate,
            amount(0),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("morph");
        assert_eq!(
            morph.cycle_period_us, legacy.cycle_period_us,
            "cycle at gate {}",
            gate
        );
        assert_eq!(morph.events.len(), legacy.events.len(), "gate {}", gate);
        for (morph_event, legacy_event) in morph.events.iter().zip(legacy.events.iter()) {
            assert_eq!(morph_event.offset_us, legacy_event.offset_us);
            assert_eq!(morph_event.bytes, legacy_event.bytes);
            assert!(morph_event.event_id.is_some(), "morph events carry ids");
        }
    }
}

// ---------------------------------------------------------------------------
// Velocity, gate, and semantic behavior
// ---------------------------------------------------------------------------

#[test]
fn accent_velocity_stays_on_its_own_surviving_event() {
    let mut pattern = straight_sixteen();
    pattern.step[1].accent = Accent::On;
    let (schedule, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(40),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    for event in schedule.events.iter().filter(|e| is_note_on(e)) {
        let expected = if event.event_id.map(|id| id.source_step) == Some(1) {
            110
        } else {
            78
        };
        assert_eq!(event.bytes[2], expected, "event {:?}", event.event_id);
    }
}

#[test]
fn losing_attack_gate_approaches_collision_without_crossing() {
    let pattern = straight_sixteen();
    let mut previous_gate: Option<u64> = None;
    // Amounts below the collision retirement point, where the losing
    // attack is still emitted and its gate is still shrinking.
    for raw in [1u32, 10, 25, 40, 49] {
        let (schedule, plan) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            100,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        let loser = plan.beats[0].loser as u8;
        let survivor = plan.beats[0].selected[1] as u8;
        let loser_on = note_on_offset(&schedule.events, loser).expect("loser attacks");
        let loser_off = note_off_offset(&schedule.events, loser).expect("loser releases");
        let survivor_on = note_on_offset(&schedule.events, survivor).expect("survivor");
        assert!(loser_on < loser_off, "gate must be positive at {}", raw);
        assert!(
            loser_off <= survivor_on,
            "loser release crossed its collision destination at {}",
            raw
        );
        let gate = loser_off - loser_on;
        if let Some(previous) = previous_gate {
            assert!(
                gate < previous,
                "gate must shrink: {} -> {}",
                previous,
                gate
            );
        }
        previous_gate = Some(gate);
    }
}

#[test]
fn endpoint_emits_no_zero_length_attack() {
    let pattern = straight_sixteen();
    for gate in [1u32, 50, 100] {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            gate,
            amount(100),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        for event in schedule.events.iter().filter(|e| is_note_on(e)) {
            let id = event.event_id.expect("id");
            let off = schedule
                .events
                .iter()
                .find(|candidate| {
                    candidate.event_id.map(|c| c.source_step) == Some(id.source_step)
                        && is_note_off(candidate)
                })
                .expect("matching release");
            assert!(off.offset_us > event.offset_us, "zero-length attack");
        }
    }
}

#[test]
fn rests_emit_no_attacks_and_ties_extend_without_duplicates() {
    let mut pattern = straight_sixteen();
    pattern.step[1].time = Time::Tie;
    pattern.step[2].time = Time::Rest;
    pattern.step[6].time = Time::TieRest;
    for raw in [0u32, 30, 70, 100] {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        let ons: Vec<u8> = schedule
            .events
            .iter()
            .filter(|e| is_note_on(e))
            .filter_map(|e| e.event_id.map(|id| id.source_step))
            .collect();
        let mut deduped = ons.clone();
        deduped.dedup();
        assert_eq!(ons, deduped, "duplicate note on at amount {}", raw);
        assert!(
            !ons.contains(&1) && !ons.contains(&2) && !ons.contains(&6),
            "tie or rest cells must not attack at amount {}",
            raw
        );
    }
}

#[test]
fn full_gate_note_off_sorts_before_the_next_note_on() {
    let pattern = straight_sixteen();
    let (schedule, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        100,
        amount(50),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    // With 100 percent gate each release lands exactly on the next
    // warped boundary; the release must be dispatched first.
    for (index, event) in schedule.events.iter().enumerate() {
        if !is_note_on(event) {
            continue;
        }
        for later in &schedule.events[index + 1..] {
            assert!(
                !(is_note_off(later)
                    && later.offset_us == event.offset_us
                    && later.event_id.map(|id| id.role) == Some(MorphEventRole::NoteOff)
                    && later.event_id.map(|id| id.source_step)
                        != event.event_id.map(|id| id.source_step)),
                "foreign note off sorted after a note on at equal offset"
            );
        }
    }
}

#[test]
fn connected_slide_overlap_is_bounded_and_ordered() {
    let mut pattern = straight_sixteen();
    pattern.step[1].slide = Slide::On;
    pattern.step[2].note = 4;
    for raw in [1u32, 50, 99] {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        let new_on = note_on_offset(&schedule.events, 2).expect("slide target on");
        let tail = schedule
            .events
            .iter()
            .find(|event| {
                event.event_id
                    == Some(MorphEventId {
                        source_step: 1,
                        role: MorphEventRole::SlideTailNoteOff,
                    })
            })
            .expect("slide tail release");
        assert!(tail.offset_us >= new_on, "tail before the new attack");
        assert!(tail.offset_us <= schedule.cycle_period_us);
        let overlap = tail.offset_us - new_on;
        // One thirty-second of a beat at 120 BPM is 15625 us.
        assert!(overlap <= 15_625, "overlap too long: {}", overlap);
    }
}

#[test]
fn final_wrap_leaves_no_sounding_note() {
    let mut pattern = straight_sixteen();
    pattern.step[14].slide = Slide::On;
    pattern.step[15].time = Time::Tie;
    for raw in [0u32, 40, 99, 100] {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            100,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        let mut sounding: Vec<u8> = Vec::new();
        for event in &schedule.events {
            let Some(&note) = event.bytes.get(1) else {
                continue;
            };
            if is_note_on(event) {
                sounding.push(note);
            } else if is_note_off(event) {
                sounding.retain(|&candidate| candidate != note);
            }
        }
        assert!(
            sounding.is_empty(),
            "notes left sounding after the wrap at amount {}: {:?}",
            raw,
            sounding
        );
    }
}

// ---------------------------------------------------------------------------
// BPM boundaries and typed failures
// ---------------------------------------------------------------------------

#[test]
fn bpm_boundaries_use_checked_nonzero_timing() {
    let pattern = straight_sixteen();
    for (centibpm, expected_cycle) in [(2_000u32, 12_000_000u64), (30_000, 800_000)] {
        for raw in [0u32, 50, 100] {
            let (schedule, _) = prepare_morph_schedule(
                &pattern,
                centibpm,
                50,
                amount(raw),
                DEFAULT_AUDITION_CHANNEL,
            )
            .expect("schedule");
            assert_eq!(schedule.cycle_period_us, expected_cycle);
            assert!(!schedule.events.is_empty());
            for event in &schedule.events {
                assert!(event.offset_us <= expected_cycle);
            }
        }
    }
}

#[test]
fn malformed_amount_and_ineligible_sources_return_typed_errors() {
    assert!(MorphAmount::new(101).is_err());

    let mut native_triplet = straight_sixteen();
    native_triplet.triplet = true;
    let result = prepare_morph_schedule(
        &native_triplet,
        CENTIBPM_120,
        50,
        amount(10),
        DEFAULT_AUDITION_CHANNEL,
    );
    assert!(matches!(result, Err(Td3Error::TripletMorph(_))));

    let mut short = straight_sixteen();
    short.active_steps = 7;
    let result = prepare_morph_schedule(
        &short,
        CENTIBPM_120,
        50,
        amount(10),
        DEFAULT_AUDITION_CHANNEL,
    );
    assert!(matches!(result, Err(Td3Error::TripletMorph(_))));
}

// ---------------------------------------------------------------------------
// Variable pattern length: 4, 8, 12, and 16 active steps
// ---------------------------------------------------------------------------

fn straight_of_length(active_steps: u8) -> crate::pattern::Pattern {
    let mut pattern = straight_sixteen();
    pattern.active_steps = active_steps;
    pattern
}

#[test]
fn every_supported_length_keeps_its_own_beat_count_and_cycle() {
    for (active_steps, beats) in [(4u8, 1u64), (8, 2), (12, 3), (16, 4)] {
        let pattern = straight_of_length(active_steps);
        for raw in [0u32, 50, 100] {
            let (schedule, plan) = prepare_morph_schedule(
                &pattern,
                CENTIBPM_120,
                50,
                amount(raw),
                DEFAULT_AUDITION_CHANNEL,
            )
            .expect("schedule");
            assert_eq!(
                plan.beats.len(),
                beats as usize,
                "{} steps plan beats at {}%",
                active_steps,
                raw
            );
            assert_eq!(plan.assignments.len(), active_steps as usize);
            assert_eq!(
                schedule.cycle_period_us,
                beats * 500_000,
                "{} steps cycle at {}%",
                active_steps,
                raw
            );
            for event in &schedule.events {
                assert!(event.offset_us <= schedule.cycle_period_us);
            }
        }
    }
}

#[test]
fn every_supported_length_keeps_fixed_anchors_and_three_endpoint_cells_per_beat() {
    for active_steps in [4u8, 8, 12, 16] {
        let pattern = straight_of_length(active_steps);
        let beats = active_steps as usize / 4;

        let (endpoint, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(100),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("endpoint schedule");
        let attacks = endpoint.events.iter().filter(|e| is_note_on(e)).count();
        assert_eq!(
            attacks,
            beats * 3,
            "{} steps endpoint keeps three attacks per beat",
            active_steps
        );

        for raw in [0u32, 25, 50, 75, 100] {
            let (schedule, _) = prepare_morph_schedule(
                &pattern,
                CENTIBPM_120,
                50,
                amount(raw),
                DEFAULT_AUDITION_CHANNEL,
            )
            .expect("schedule");
            for beat in 0..beats {
                assert_eq!(
                    note_on_offset(&schedule.events, (beat * 4) as u8),
                    Some(beat as u64 * 500_000),
                    "{} steps anchor beat {} at {}%",
                    active_steps,
                    beat,
                    raw
                );
            }
        }
    }
}

#[test]
fn every_supported_length_matches_the_legacy_straight_schedule_at_zero() {
    for active_steps in [4u8, 8, 12, 16] {
        let mut pattern = straight_of_length(active_steps);
        pattern.step[1].accent = Accent::On;
        pattern.step[2].time = Time::Rest;
        let legacy =
            prepare_schedule_with_gate(&pattern, CENTIBPM_120, 50, DEFAULT_AUDITION_CHANNEL)
                .expect("legacy");
        let (morph, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(0),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("morph");
        assert_eq!(morph.cycle_period_us, legacy.cycle_period_us);
        assert_eq!(morph.events.len(), legacy.events.len(), "{}", active_steps);
        for (a, b) in morph.events.iter().zip(legacy.events.iter()) {
            assert_eq!(a.offset_us, b.offset_us);
            assert_eq!(a.bytes, b.bytes);
        }
    }
}

#[test]
fn a_single_beat_source_enumerates_only_three_candidates() {
    // One beat: S2+S4 by default for an all-equal source.
    let plan =
        crate::triplet_morph::plan_triplet_morph(&straight_of_length(4)).expect("single beat plan");
    assert_eq!(plan.beats.len(), 1);
    assert_eq!(plan.beats[0].selected, [1, 3]);
    assert_eq!(plan.beats[0].loser, 2);
    assert_eq!(plan.assignments.len(), 4);
}

// ---------------------------------------------------------------------------
// Collision retirement
// ---------------------------------------------------------------------------

/// Smallest interval between consecutive Note On events in a schedule.
fn min_attack_gap_us(schedule: &crate::web::clock::AuditionSchedule) -> Option<u64> {
    let ons: Vec<u64> = schedule
        .events
        .iter()
        .filter(|e| is_note_on(e))
        .map(|e| e.offset_us)
        .collect();
    ons.windows(2).map(|w| w[1] - w[0]).min()
}

#[test]
fn a_losing_attack_is_retired_once_it_closes_on_the_next_attack() {
    let pattern = straight_sixteen();
    // At 120 BPM the amount threshold is reached long before the 20 ms
    // separation floor, so the threshold is where the loser leaves.
    let threshold = COLLISION_RETIREMENT_AMOUNT_PERCENT;
    let (below, plan) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(threshold - 1),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    let loser = plan.beats[0].loser as u8;
    assert!(
        note_on_offset(&below.events, loser).is_some(),
        "one below the threshold the losing attack still sounds"
    );

    let (above, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(threshold),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    assert!(
        note_on_offset(&above.events, loser).is_none(),
        "at the threshold the losing attack is retired"
    );
    // A retired attack leaves no release behind either.
    assert!(
        !above
            .events
            .iter()
            .any(|e| e.event_id.map(|id| id.source_step) == Some(loser)),
        "a retired attack emits no events at all"
    );
}

#[test]
fn retirement_keeps_every_schedule_above_the_floor_at_all_amounts_and_tempos() {
    let mut pattern = straight_sixteen();
    pattern.step[5].accent = Accent::On;
    pattern.step[9].note = 7;
    for centibpm in [2_000u32, 12_000, 13_600, 24_000, 30_000] {
        for raw in 0..=100u32 {
            let (schedule, _) = prepare_morph_schedule(
                &pattern,
                centibpm,
                50,
                amount(raw),
                DEFAULT_AUDITION_CHANNEL,
            )
            .expect("schedule");
            if let Some(gap) = min_attack_gap_us(&schedule) {
                assert!(
                    gap >= 20_000,
                    "centibpm {} amount {} left a {} us attack gap",
                    centibpm,
                    raw,
                    gap
                );
            }
        }
    }
}

#[test]
fn the_earlier_of_the_threshold_and_the_floor_retires_the_loser() {
    let pattern = straight_sixteen();
    let loser = crate::triplet_morph::plan_triplet_morph(&pattern)
        .expect("plan")
        .beats[0]
        .loser as u8;
    let sounds = |centibpm: u32, raw: u32| {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            centibpm,
            50,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        note_on_offset(&schedule.events, loser).is_some()
    };
    // Two rules retire the loser and the earlier one wins. The
    // separation floor is crossed at 1 - BPM/750 of the sweep, so it
    // governs above 750 * (1 - threshold/100) BPM and the amount
    // threshold governs below that.
    let threshold = COLLISION_RETIREMENT_AMOUNT_PERCENT;
    let crossover_bpm = 750.0 * (1.0 - f64::from(threshold) / 100.0);

    let mut below = 0;
    let mut above = 0;
    for centibpm in [2_000u32, 6_000, 12_000, 14_000, 20_000, 30_000] {
        let bpm = f64::from(centibpm) / 100.0;
        if bpm < crossover_bpm {
            below += 1;
            assert!(
                sounds(centibpm, threshold - 1),
                "{centibpm} centi-BPM is threshold governed and should still sound \
                 one below the threshold",
            );
            assert!(
                !sounds(centibpm, threshold),
                "{centibpm} centi-BPM should retire at the threshold",
            );
        } else {
            // The floor gets there first, so the loser is already gone
            // before the threshold, but it still sounds early on.
            above += 1;
            assert!(
                !sounds(centibpm, threshold - 1),
                "{centibpm} centi-BPM crosses the separation floor before the threshold",
            );
            assert!(
                sounds(centibpm, 20),
                "{centibpm} centi-BPM should still sound early in the sweep",
            );
        }
    }
    assert!(
        below > 0 && above > 0,
        "the tempo set must straddle the crossover"
    );
}

#[test]
fn a_slide_connected_loser_is_never_retired() {
    let mut pattern = straight_sixteen();
    // One uninterrupted S2 -> S3 -> S4 chain: dropping the middle is the
    // legal contraction, so the losing cell is itself slide connected.
    pattern.step[1].slide = Slide::On;
    pattern.step[2].slide = Slide::On;
    pattern.step[2].note = 4;
    let (schedule, plan) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(99),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    let loser = plan.beats[0].loser as u8;
    assert_eq!(loser, 2, "the chain middle loses in beat 0");
    assert!(
        note_on_offset(&schedule.events, loser).is_some(),
        "a glide target retriggers nothing, so it is not retired"
    );
}

#[test]
fn zero_amount_retires_nothing() {
    let pattern = straight_sixteen();
    let (schedule, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(0),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    let ons = schedule.events.iter().filter(|e| is_note_on(e)).count();
    assert_eq!(ons, 16, "every straight attack survives at zero");
}

// ---------------------------------------------------------------------------
// Losing attack gate
// ---------------------------------------------------------------------------

/// A losing attack never overruns the attack it hands over to, and the
/// surviving attacks keep the gate fraction. How much of the gap it
/// closes on the way is covered by
/// [`the_losing_gap_closes_in_proportion_to_the_amount`].
#[test]
fn a_losing_attack_never_overruns_its_successor() {
    let source = straight_sixteen();
    // S3 of each beat is the default loser: source steps 2, 6, 10, 14.
    let losers = [2u8, 6, 10, 14];
    let threshold = COLLISION_RETIREMENT_AMOUNT_PERCENT;

    for value in [10u32, 25, 40, 49, threshold - 1] {
        for gate in [1u32, 25, 50, 90, 100] {
            let (schedule, _plan) = prepare_morph_schedule(
                &source,
                CENTIBPM_120,
                gate,
                amount(value),
                DEFAULT_AUDITION_CHANNEL,
            )
            .expect("intermediate schedule");
            let events = &schedule.events;

            for loser in losers {
                let onset = note_on_offset(events, loser).expect("loser attack present");
                let release = note_off_offset(events, loser).expect("loser release present");
                assert!(
                    release > onset,
                    "amount {value}, gate {gate}: loser {loser} releases at {release} \
                     which is not after its onset {onset}",
                );

                // The next surviving attack is the following source step,
                // or the cycle end for the last one.
                let next_onset =
                    note_on_offset(events, loser + 1).unwrap_or(schedule.cycle_period_us);
                assert!(
                    release <= next_onset,
                    "amount {value}, gate {gate}: loser {loser} releases at {release}, \
                     past the attack it hands over to at {next_onset}",
                );

                // The release must be dispatched ahead of the attack it
                // hands over to, not after it.
                let release_index = events
                    .iter()
                    .position(|event| event.offset_us == release && is_note_off(event))
                    .expect("release ordered");
                let next_index = events
                    .iter()
                    .position(|event| event.offset_us == next_onset && is_note_on(event));
                if let Some(next_index) = next_index {
                    assert!(
                        release_index < next_index,
                        "amount {value}, gate {gate}: loser {loser} releases after its successor",
                    );
                }
            }

            // A surviving attack still takes the gate fraction, so its
            // gate stays below the distance to the next attack whenever
            // the gate is under 100 percent.
            if gate < 100 {
                let onset = note_on_offset(events, 1).expect("survivor attack present");
                let release = note_off_offset(events, 1).expect("survivor release present");
                let next_onset = note_on_offset(events, 2).expect("next attack present");
                assert!(
                    release < next_onset,
                    "amount {value}, gate {gate}: survivor 1 should not reach {next_onset}",
                );
                assert!(
                    release > onset,
                    "amount {value}, gate {gate}: survivor 1 gate is empty"
                );
            }
        }
    }
}

/// The silence a losing attack leaves before the attack it collides
/// into closes in step with the amount: untouched where the knob has
/// barely left zero, gone by the retirement threshold, and shrinking
/// throughout.
///
/// Taking the whole hold from amount 1 was audible on the device as an
/// abrupt jump. Recorded at 120 BPM with the gate at 50, one cycle held
/// 572 ms of silence at amount 0 and 327 ms at amount 1, with three of
/// its twelve gaps closed outright.
#[test]
fn the_losing_gap_closes_in_proportion_to_the_amount() {
    let source = straight_sixteen();
    let losers = [2u8, 6, 10, 14];
    let threshold = COLLISION_RETIREMENT_AMOUNT_PERCENT;
    // Half a cell at 120 BPM: what the canonical amount-0 schedule
    // leaves after every step at gate 50.
    let canonical_silence_us = 62_500i64;

    for loser in losers {
        let mut previous: Option<u64> = None;
        for value in [1u32, 10, 25, 40, 55, threshold - 1] {
            let (schedule, _plan) = prepare_morph_schedule(
                &source,
                CENTIBPM_120,
                50,
                amount(value),
                DEFAULT_AUDITION_CHANNEL,
            )
            .expect("intermediate schedule");
            let events = &schedule.events;
            let onset = note_on_offset(events, loser).expect("loser attack present");
            let release = note_off_offset(events, loser).expect("loser release present");
            let next_onset = note_on_offset(events, loser + 1).unwrap_or(schedule.cycle_period_us);
            assert!(release > onset, "amount {value}: loser {loser} has no gate");
            assert!(
                release <= next_onset,
                "amount {value}: loser {loser} overruns its successor",
            );

            let silence = next_onset - release;
            if value == 1 {
                // One amount step off zero may shave a few percent off
                // the silence through the warp, the proportional hold
                // and the first step of the gate compensation ramp. The
                // bug this guards against took it to zero outright.
                let drift = i64::try_from(silence).expect("silence fits") - canonical_silence_us;
                assert!(
                    drift.abs() * 100 < canonical_silence_us * 8,
                    "amount 1: loser {loser} leaves {silence} us of silence, \
                     {drift} us from the {canonical_silence_us} us it leaves at amount 0",
                );
            }
            if value == threshold - 1 {
                assert!(
                    silence * 100 < next_onset - onset,
                    "one below the threshold: loser {loser} still leaves {silence} us \
                     of silence before the handover",
                );
            }
            if let Some(previous) = previous {
                assert!(
                    silence < previous,
                    "amount {value}: loser {loser} silence grew, {previous} -> {silence}",
                );
            }
            previous = Some(silence);
        }
    }
}

/// At amount 0 the schedule is the canonical straight one, so a losing
/// step is an ordinary step and keeps the gate fraction like the rest.
#[test]
fn amount_zero_keeps_the_gate_fraction_on_every_step() {
    let source = straight_sixteen();
    let (schedule, _plan) = prepare_morph_schedule(
        &source,
        CENTIBPM_120,
        50,
        amount(0),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("zero schedule");
    let events = &schedule.events;
    for step in [1u8, 2, 6, 10] {
        let onset = note_on_offset(events, step).expect("attack present");
        let release = note_off_offset(events, step).expect("release present");
        assert_eq!(
            release - onset,
            62_500,
            "step {step} should hold the canonical half cell at amount 0",
        );
    }
}

/// The winner a retired loser collided into starts in the hole the
/// loser left, so the attack grid is unchanged across the retirement
/// point and only the note count drops. From there the winner slides
/// forward and its body shrinks until it sits on the triplet grid.
#[test]
fn a_retired_losers_winner_absorbs_its_slot_then_settles_on_the_grid() {
    let pattern = straight_sixteen();
    let plan = crate::triplet_morph::plan_triplet_morph(&pattern).expect("plan");
    let loser = plan.beats[0].loser as u8;

    // The last amount before retirement fixes the attack grid that the
    // retirement point has to preserve.
    let threshold = COLLISION_RETIREMENT_AMOUNT_PERCENT;
    let (before, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(threshold - 1),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    let loser_slot =
        note_on_offset(&before.events, loser).expect("loser sounds below the threshold");

    let (at, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(threshold),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    assert!(
        note_on_offset(&at.events, loser).is_none(),
        "the loser is retired at the threshold"
    );
    let winner = (0..16u8)
        .find(|&step| {
            step != loser
                && note_on_offset(&at.events, step)
                    .is_some_and(|onset| onset.abs_diff(loser_slot) < 2_000)
        })
        .expect("some attack took over the loser's slot");

    // The slot is held within one warp step of where the loser was, so
    // the ear hears the same pulse with one fewer note in it.
    let absorbed_onset = note_on_offset(&at.events, winner).expect("winner sounds");
    assert!(
        absorbed_onset.abs_diff(loser_slot) < 2_000,
        "winner {winner} starts at {absorbed_onset}, not in the loser's slot at {loser_slot}",
    );

    // Sweeping on, the winner slides forward and its body shrinks
    // monotonically toward the grid.
    let mut previous: Option<(u64, u64)> = None;
    // Evenly spaced from the retirement point to the endpoint, so the
    // list stays strictly increasing wherever the threshold is set.
    let steps: Vec<u32> = (0..5)
        .map(|i| threshold + (99 - threshold) * i / 4)
        .collect();
    for raw in steps {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        let onset = note_on_offset(&schedule.events, winner).expect("winner sounds");
        let release = note_off_offset(&schedule.events, winner).expect("winner releases");
        assert!(release > onset, "amount {raw}: winner gate is empty");
        let body = release - onset;
        if let Some((previous_onset, previous_body)) = previous {
            assert!(
                onset > previous_onset,
                "amount {raw}: winner onset {onset} did not advance past {previous_onset}",
            );
            assert!(
                body < previous_body,
                "amount {raw}: winner body {body} did not shrink below {previous_body}",
            );
        }
        previous = Some((onset, body));
    }

    // At 99 the winner has nearly reached the endpoint cell it occupies
    // at 100, which for the default choice is 2/3 of the beat.
    let (near_end, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(99),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    let onset = note_on_offset(&near_end.events, winner).expect("winner sounds");
    assert!(
        onset.abs_diff(333_333) < 1_500,
        "winner should converge on 2/3 beat, got {onset}",
    );
}

/// A slide-connected winner keeps its own onset: its position belongs
/// to the glide, not to the hole beside it. S4 slides onward to the next
/// beat's anchor, which leaves S3 a plain loser that still retires.
#[test]
fn a_slide_connected_winner_is_not_pulled_into_the_hole() {
    let mut pattern = straight_sixteen();
    pattern.step[3].slide = Slide::On;
    pattern.step[4].note = 5;
    // Guard against the assertion passing because absorption never ran.
    let plan = crate::triplet_morph::plan_triplet_morph(&pattern).expect("plan");
    assert_eq!(plan.beats[0].loser, 2, "S3 is still the loser");

    // Comfortably above COLLISION_RETIREMENT_AMOUNT_PERCENT.
    let (schedule, _) = prepare_morph_schedule(
        &pattern,
        CENTIBPM_120,
        50,
        amount(80),
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule");
    assert!(
        note_on_offset(&schedule.events, 2).is_none(),
        "the plain loser still retires above the threshold"
    );
    // S4 warps to 3/4 - 0.8 * (3/4 - 2/3) = 41/60 beat. The hole S3 left
    // is at 1/2 + 0.8 * (2/3 - 1/2) = 19/30 beat. S4 must stay on its own.
    assert_eq!(
        note_on_offset(&schedule.events, 3),
        Some(341_666),
        "a slide-connected winner keeps its own warped onset"
    );
}

// ---------------------------------------------------------------------------
// Sweep gate compensation
// ---------------------------------------------------------------------------

/// Effective gate of the beat-0 anchor as a percentage of its cell. The
/// anchor is never retired and never absorbed, so its cell runs from 0
/// to the next attack at every amount.
fn anchor_gate_percent(events: &[ScheduledMidi]) -> f64 {
    let release = note_off_offset(events, 0).expect("anchor releases");
    let cell = events
        .iter()
        .filter(|event| is_note_on(event))
        .map(|event| event.offset_us)
        .find(|&offset| offset > 0)
        .expect("a second attack bounds the anchor cell");
    release as f64 / cell as f64 * 100.0
}

/// The audible gate is widened across the sweep and returns to the set
/// value at both ends. A gate is a fraction of its cell, so widening
/// cells alone would hold the duty cycle constant, but the device's
/// envelope ring is a fixed time per note: fewer, wider cells collect it
/// fewer times and each silence grows in absolute terms.
#[test]
fn gate_compensation_peaks_at_its_amount_and_vanishes_at_both_ends() {
    let pattern = straight_sixteen();
    let measured = |raw: u32| {
        let (schedule, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            50,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("schedule");
        anchor_gate_percent(&schedule.events)
    };

    let peak_at = GATE_COMPENSATION_PEAK_PERCENT as u32;
    let strength = GATE_COMPENSATION_PEAK_NUM as f64 / GATE_COMPENSATION_PEAK_DEN as f64;

    // At the peak the compensation removes `strength` of the silence a
    // gate of 50 would otherwise leave.
    let expected_peak = 100.0 * (1.0 - 0.5 * (1.0 - strength));
    assert!(
        (measured(peak_at) - expected_peak).abs() < 0.05,
        "peak gate should be {expected_peak} percent, got {}",
        measured(peak_at)
    );

    // Both ends return the gate the user set, give or take one amount
    // step of ramp.
    assert!(
        measured(1) > 50.0 && measured(1) < 52.0,
        "start, got {}",
        measured(1)
    );
    assert!(
        measured(99) > 50.0 && measured(99) < 53.0,
        "end, got {}",
        measured(99)
    );

    // The curve is a triangle in the amount: it rises across the first
    // side, falls across the second, and the two sides have different
    // widths whenever the peak is off centre. Checked against the shape
    // directly rather than by pairing amounts either side of the peak,
    // which only lines up exactly when both sides divide evenly.
    let ramp = |raw: u32| {
        if raw <= peak_at {
            f64::from(raw) / f64::from(peak_at)
        } else {
            f64::from(100 - raw) / f64::from(100 - peak_at)
        }
    };
    let expected = |raw: u32| 100.0 * (1.0 - 0.5 * (1.0 - strength * ramp(raw)));
    for raw in [1u32, 10, 25, 40, 55, peak_at, 75, 85, 92, 99] {
        assert!(
            (measured(raw) - expected(raw)).abs() < 0.05,
            "amount {raw}: gate {} does not match the curve at {}",
            measured(raw),
            expected(raw),
        );
    }

    // Monotonic up to the peak and back down after it.
    let mut walk = vec![1u32, peak_at / 4, peak_at / 2, peak_at];
    walk.extend([1, 2, 3].map(|i| peak_at + (99 - peak_at) * i / 4));
    walk.push(99);
    for pair in walk.windows(2) {
        let rising_side = pair[1] <= peak_at;
        if rising_side {
            assert!(
                measured(pair[1]) > measured(pair[0]),
                "{} should exceed {}",
                pair[1],
                pair[0]
            );
        } else {
            assert!(
                measured(pair[1]) < measured(pair[0]),
                "{} should fall below {}",
                pair[1],
                pair[0]
            );
        }
    }
}

/// The compensation removes a share of the remaining silence rather than
/// displacing the gate, so it scales with what the user set: a legato
/// gate is untouched and a staccato gate stays staccato.
#[test]
fn gate_compensation_scales_with_the_set_gate_and_never_overruns() {
    let pattern = straight_sixteen();
    for raw in [1u32, 25, 50, 75, 99] {
        for gate in [1u32, 10, 25, 50, 75, 90, 100] {
            let (schedule, _) = prepare_morph_schedule(
                &pattern,
                CENTIBPM_120,
                gate,
                amount(raw),
                DEFAULT_AUDITION_CHANNEL,
            )
            .expect("build");
            let measured = anchor_gate_percent(&schedule.events);
            assert!(
                measured >= f64::from(gate) - 0.05,
                "gate {gate} at amount {raw} shrank to {measured}",
            );
            assert!(
                measured <= 100.0,
                "gate {gate} at amount {raw} overran the cell at {measured}",
            );
            // The release still lands strictly inside the cell for any
            // gate below full legato.
            if gate < 100 {
                assert!(
                    measured < 100.0,
                    "gate {gate} at amount {raw} became legato at {measured}",
                );
            }
        }
        // A full legato gate has no silence to reclaim, so it is
        // untouched at every amount.
        let (legato, _) = prepare_morph_schedule(
            &pattern,
            CENTIBPM_120,
            100,
            amount(raw),
            DEFAULT_AUDITION_CHANNEL,
        )
        .expect("build");
        assert!((anchor_gate_percent(&legato.events) - 100.0).abs() < 0.05);
    }
}
