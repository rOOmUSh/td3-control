//! Exhaustive local-state invariants for the triplet morph planner.
//!
//! Enumerates every four-cell Time combination for beat 0 in several
//! articulation contexts and asserts warp invariants for all 101 knob
//! amounts: offsets stay inside the beat, anchors are exact, source
//! order is strict below 100 and nondecreasing at 100, selected targets
//! land exactly on 1/3 and 2/3, and no selected tie is orphaned.

use crate::step::{Accent, Slide, Time};
use crate::triplet_morph::{
    normalize_source, plan_triplet_morph, MorphAmount, Rat, TripletMorphPlan,
};

use super::fixtures::straight_sixteen;

const TIMES: [Time; 4] = [Time::Normal, Time::Tie, Time::Rest, Time::TieRest];

#[derive(Clone, Copy)]
enum Context {
    Plain,
    AccentedS3,
    SlideS2,
    NextBeatOwnerTie,
}

const CONTEXTS: [Context; 4] = [
    Context::Plain,
    Context::AccentedS3,
    Context::SlideS2,
    Context::NextBeatOwnerTie,
];

fn beat_zero_pattern(times: [Time; 4], context: Context) -> crate::pattern::Pattern {
    let mut pattern = straight_sixteen();
    for (local, time) in times.iter().enumerate() {
        pattern.step[local].time = *time;
    }
    match context {
        Context::Plain => {}
        Context::AccentedS3 => pattern.step[2].accent = Accent::On,
        Context::SlideS2 => pattern.step[1].slide = Slide::On,
        Context::NextBeatOwnerTie => pattern.step[4].time = Time::Tie,
    }
    pattern
}

fn assert_beat_zero_invariants(pattern: &crate::pattern::Pattern, plan: &TripletMorphPlan) {
    let phrase = normalize_source(pattern).expect("eligible source");
    let one_third = Rat::new(1, 3).expect("nonzero denominator");
    let two_thirds = Rat::new(2, 3).expect("nonzero denominator");

    // No selected tie is orphaned by the plan.
    let losers: Vec<usize> = plan.beats.iter().map(|beat| beat.loser).collect();
    for boundary in &phrase.boundaries {
        if losers.contains(&boundary.step) {
            continue;
        }
        if let Some(owner) = boundary.continues_owner {
            assert!(
                !losers.contains(&owner),
                "surviving tie at step {} lost its owner {}",
                boundary.step,
                owner
            );
        }
    }

    for raw_amount in 0..=100u32 {
        let amount = MorphAmount::new(raw_amount).expect("amount in range");
        let mut previous: Option<Rat> = None;
        for step in 0..16usize {
            let position = plan
                .warp_boundary(step, amount)
                .expect("warp for every source step");
            let beat = Rat::int((step / 4) as i128);
            let beat_end = Rat::int((step / 4) as i128 + 1);
            assert!(
                position >= beat && position <= beat_end,
                "step {} amount {} left its beat: {:?}",
                step,
                raw_amount,
                position
            );
            if step % 4 == 0 {
                assert_eq!(position, beat, "anchor moved at step {}", step);
            }
            if let Some(previous) = previous {
                if raw_amount < 100 {
                    assert!(
                        previous < position,
                        "order not strict at step {} amount {}",
                        step,
                        raw_amount
                    );
                } else {
                    assert!(
                        previous <= position,
                        "order decreased at step {} amount 100",
                        step
                    );
                }
            }
            previous = Some(position);
        }
        assert_eq!(
            plan.warp_boundary(16, amount),
            Some(Rat::int(4)),
            "bar end must stay exact"
        );

        if raw_amount == 100 {
            for beat_plan in &plan.beats {
                let beat = Rat::int(beat_plan.beat as i128);
                let early = plan
                    .warp_boundary(beat_plan.selected[0], amount)
                    .expect("early survivor");
                let late = plan
                    .warp_boundary(beat_plan.selected[1], amount)
                    .expect("late survivor");
                assert_eq!(early.sub(beat), one_third);
                assert_eq!(late.sub(beat), two_thirds);
            }
        }
    }
}

#[test]
fn every_time_combination_plans_without_panic_and_warps_inside_the_beat() {
    for combination in 0..256usize {
        let times = [
            TIMES[combination % 4],
            TIMES[(combination / 4) % 4],
            TIMES[(combination / 16) % 4],
            TIMES[(combination / 64) % 4],
        ];
        for context in CONTEXTS {
            let pattern = beat_zero_pattern(times, context);
            let plan = plan_triplet_morph(&pattern)
                .expect("every four-cell time combination must stay plannable");
            assert_beat_zero_invariants(&pattern, &plan);
        }
    }
}
