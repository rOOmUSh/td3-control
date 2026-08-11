use crate::step::{Accent, Slide, Time, Transpose};
use crate::triplet_morph::{
    endpoint_as_ephemeral_pattern, normalize_source, plan_triplet_morph, project_endpoint,
    DerivedCellRole, MorphAmount, MorphPlanError, Rat, TripletMorphPlan,
};

use super::fixtures::straight_sixteen;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn plan_of(pattern: &crate::pattern::Pattern) -> TripletMorphPlan {
    plan_triplet_morph(pattern).expect("planning must succeed for eligible fixture")
}

fn rat(num: i128, den: i128) -> Rat {
    Rat::new(num, den).expect("nonzero denominator")
}

/// Set beats 1..=3 to a quiet shape (S1 attack, three redundant-ish
/// rests) so table tests can isolate beat 0 loss.
fn quiet_tail(pattern: &mut crate::pattern::Pattern) {
    for beat in 1..4 {
        let base = beat * 4;
        pattern.step[base].time = Time::Normal;
        for local in 1..4 {
            pattern.step[base + local].time = Time::Rest;
        }
    }
}

// ---------------------------------------------------------------------------
// Eligibility and amount validation
// ---------------------------------------------------------------------------

#[test]
fn amount_accepts_zero_through_one_hundred() {
    assert_eq!(MorphAmount::new(0).map(|a| a.value()), Ok(0));
    assert_eq!(MorphAmount::new(100).map(|a| a.value()), Ok(100));
    assert_eq!(
        MorphAmount::new(101),
        Err(MorphPlanError::AmountOutOfRange(101))
    );
}

#[test]
fn normalize_rejects_native_triplet() {
    let mut pattern = straight_sixteen();
    pattern.triplet = true;
    assert_eq!(
        normalize_source(&pattern).err(),
        Some(MorphPlanError::NativeTripletEnabled)
    );
}

#[test]
fn normalize_rejects_unsupported_active_steps() {
    // Whole four-step beats only: 4, 8, 12, and 16 are supported.
    for value in [1u8, 2, 3, 5, 6, 7, 9, 10, 11, 13, 14, 15] {
        let mut pattern = straight_sixteen();
        pattern.active_steps = value;
        assert_eq!(
            normalize_source(&pattern).err(),
            Some(MorphPlanError::UnsupportedActiveSteps(value)),
            "active_steps {} must be rejected",
            value
        );
    }
    for value in [4u8, 8, 12, 16] {
        let mut pattern = straight_sixteen();
        pattern.active_steps = value;
        let phrase = normalize_source(&pattern).expect("supported length");
        assert_eq!(phrase.active_steps, value as usize);
        assert_eq!(phrase.beat_count, value as usize / 4);
        assert_eq!(phrase.boundaries.len(), value as usize);
    }
}

// ---------------------------------------------------------------------------
// 6.1 planner table tests
// ---------------------------------------------------------------------------

#[test]
fn all_equal_offbeats_select_s2_and_s4_in_every_beat() {
    let plan = plan_of(&straight_sixteen());
    for beat in 0..4 {
        let beat_plan = &plan.beats[beat];
        assert_eq!(beat_plan.pair_rank, 0, "beat {}", beat);
        assert_eq!(beat_plan.selected, [beat * 4 + 1, beat * 4 + 3]);
        assert_eq!(beat_plan.loser, beat * 4 + 2);
    }
    assert_eq!(plan.loss.lost_attacks, 4);
    assert_eq!(plan.loss.total_survivor_displacement, rat(4, 6));
    assert_eq!(plan.loss.maximum_survivor_displacement, rat(1, 12));
}

#[test]
fn s3_accent_forces_a_pair_containing_s3() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    pattern.step[2].accent = Accent::On;
    let plan = plan_of(&pattern);
    assert!(plan.beats[0].selected.contains(&2));
    assert_eq!(plan.beats[0].pair_rank, 1, "S2+S3 beats S3+S4 by fallback");
    assert_eq!(plan.loss.lost_accented_attacks, 0);
}

#[test]
fn all_accented_offbeats_fall_back_to_minimum_displacement() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    for local in 1..4 {
        pattern.step[local].accent = Accent::On;
    }
    let plan = plan_of(&pattern);
    assert_eq!(plan.beats[0].pair_rank, 0);
    assert_eq!(plan.beats[0].loser, 2);
    assert_eq!(plan.loss.lost_accented_attacks, 1);
}

#[test]
fn connected_slide_chain_contracts_across_the_losing_middle() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    // S2 -> S3 -> S4 one uninterrupted slide chain.
    pattern.step[1].slide = Slide::On;
    pattern.step[2].slide = Slide::On;
    let plan = plan_of(&pattern);
    assert_eq!(plan.beats[0].loser, 2, "middle of the chain loses");
    assert_eq!(plan.loss.lost_slide_connectivity, 0);

    let phrase = normalize_source(&pattern).expect("eligible");
    let derived = project_endpoint(&pattern, &phrase, &plan).expect("projection");
    let s2 = derived
        .cells
        .iter()
        .find(|cell| cell.source_step == 1)
        .expect("S2 survives");
    assert_eq!(s2.step.slide, Slide::On, "contracted slide keeps the flag");
}

#[test]
fn a_rest_breaks_slide_contraction() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    // S2 slides into S3; S4 is a rest that cuts S3.
    pattern.step[1].slide = Slide::On;
    pattern.step[3].time = Time::Rest;
    let plan = plan_of(&pattern);
    assert_eq!(
        plan.beats[0].loser, 3,
        "the cutting rest loses, not the chain"
    );
    assert_eq!(plan.loss.lost_slide_connectivity, 0);
    assert_eq!(plan.loss.lost_articulation_cuts, 1);
}

#[test]
fn articulation_cut_rest_beats_redundant_silence() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    // S3 cuts the S2 note; S4 is silence after silence.
    pattern.step[2].time = Time::Rest;
    pattern.step[3].time = Time::Rest;
    let plan = plan_of(&pattern);
    assert_eq!(plan.beats[0].loser, 3, "the redundant rest loses");
    assert_eq!(plan.loss.lost_articulation_cuts, 0);
}

#[test]
fn local_pitch_extremum_is_retained_over_plain_repeats() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    pattern.step[2].note = 4;
    let plan = plan_of(&pattern);
    assert!(plan.beats[0].selected.contains(&2), "extremum survives");
    assert_eq!(plan.loss.lost_contour_critical_attacks, 0);
}

#[test]
fn a_tie_is_not_counted_as_an_attack() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    pattern.step[2].time = Time::Tie;
    let plan = plan_of(&pattern);
    assert_eq!(plan.beats[0].loser, 2, "the tie loses");
    assert_eq!(plan.loss.lost_attacks, 0, "beat 0 keeps both attacks");
    assert_eq!(plan.loss.lost_continuation_quanta, 1);
}

#[test]
fn a_selected_tie_never_loses_its_owner_attack() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    // S4 is a tie owned by S3. Losing S3 would orphan a selected S4.
    pattern.step[3].time = Time::Tie;
    let plan = plan_of(&pattern);
    assert_ne!(plan.beats[0].loser, 2, "S3 owns the surviving S4 tie");
    assert_eq!(plan.beats[0].pair_rank, 1, "the owned tie itself loses");
}

#[test]
fn owner_required_by_next_beat_s1_tie_is_retained() {
    let mut pattern = straight_sixteen();
    // Beat 1 S1 continues beat 0 S4. An S3 accent pushes toward pairs
    // containing S3; losing S4 is invalid, so S2 must lose.
    pattern.step[4].time = Time::Tie;
    pattern.step[2].accent = Accent::On;
    let plan = plan_of(&pattern);
    assert_eq!(
        plan.beats[0].loser, 1,
        "S2 loses; S4 is required cross-beat"
    );
    assert!(plan.beats[0].selected.contains(&3));
}

#[test]
fn cross_beat_slide_chain_contracts_through_the_beat_boundary() {
    let mut pattern = straight_sixteen();
    // S3 -> S4 -> next-beat S1 chain: losing S4 contracts to S3 -> S1'.
    pattern.step[2].slide = Slide::On;
    pattern.step[3].slide = Slide::On;
    pattern.step[2].accent = Accent::On;
    let plan = plan_of(&pattern);
    assert_eq!(plan.beats[0].loser, 3, "S4 loses via valid contraction");
    assert_eq!(plan.loss.lost_slide_connectivity, 0);
}

#[test]
fn empty_s1_remains_the_target_beat_cell() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    pattern.step[0].time = Time::Rest;
    pattern.step[1].accent = Accent::On;
    let plan = plan_of(&pattern);
    let phrase = normalize_source(&pattern).expect("eligible");
    let derived = project_endpoint(&pattern, &phrase, &plan).expect("projection");
    assert_eq!(derived.cells[0].source_step, 0);
    assert_eq!(derived.cells[0].role, DerivedCellRole::Silence);
    assert!(plan.assignments[0].survivor);
    assert_eq!(plan.assignments[0].target_offset, Rat::ZERO);
}

#[test]
fn leading_orphan_tie_stays_silent_and_plannable() {
    let mut pattern = straight_sixteen();
    quiet_tail(&mut pattern);
    pattern.step[0].time = Time::Tie;
    let plan = plan_of(&pattern);
    let phrase = normalize_source(&pattern).expect("eligible");
    assert!(phrase.boundaries[0].orphan_tie);
    let derived = project_endpoint(&pattern, &phrase, &plan).expect("projection");
    assert_eq!(derived.cells[0].role, DerivedCellRole::OrphanSilence);
    assert_eq!(derived.cells[0].step.time, Time::Rest);
}

#[test]
fn flags_on_silent_cells_do_not_affect_selection() {
    let mut base = straight_sixteen();
    quiet_tail(&mut base);
    base.step[2].time = Time::Rest;
    let mut flagged = straight_sixteen();
    quiet_tail(&mut flagged);
    flagged.step[2].time = Time::Rest;
    flagged.step[2].accent = Accent::On;
    flagged.step[2].slide = Slide::On;
    flagged.step[2].transpose = Transpose::Up;

    let base_plan = plan_of(&base);
    let flagged_plan = plan_of(&flagged);
    assert_eq!(base_plan.beats, flagged_plan.beats);
    assert_eq!(base_plan.loss, flagged_plan.loss);
}

#[test]
fn redundant_rest_and_tie_cases_use_the_deterministic_fallback() {
    let mut pattern = straight_sixteen();
    for step in pattern.step.iter_mut() {
        step.time = Time::Rest;
    }
    let plan = plan_of(&pattern);
    for beat in 0..4 {
        assert_eq!(plan.beats[beat].pair_rank, 0, "beat {}", beat);
    }
}

#[test]
fn uniform_transposition_does_not_change_survivors() {
    let mut base = straight_sixteen();
    quiet_tail(&mut base);
    base.step[1].note = 2;
    base.step[2].note = 7;
    base.step[3].note = 4;
    let mut shifted = straight_sixteen();
    quiet_tail(&mut shifted);
    shifted.step[1].note = 2;
    shifted.step[2].note = 7;
    shifted.step[3].note = 4;
    for step in shifted.step.iter_mut() {
        step.transpose = Transpose::Up;
    }
    assert_eq!(plan_of(&base).beats, plan_of(&shifted).beats);
}

#[test]
fn repeated_planning_is_structurally_equal() {
    let mut pattern = straight_sixteen();
    pattern.step[2].accent = Accent::On;
    pattern.step[5].time = Time::Tie;
    pattern.step[9].slide = Slide::On;
    pattern.step[11].time = Time::Rest;
    assert_eq!(plan_of(&pattern), plan_of(&pattern));
}

// ---------------------------------------------------------------------------
// Warp positions (worked default example)
// ---------------------------------------------------------------------------

#[test]
fn default_warp_matches_the_worked_example_positions() {
    let plan = plan_of(&straight_sixteen());
    let half = MorphAmount::new(50).expect("valid amount");
    // 50%: S2 = 7/24, S3 = 7/12, S4 = 17/24 beat.
    assert_eq!(plan.warp_boundary(1, half), Some(rat(7, 24)));
    assert_eq!(plan.warp_boundary(2, half), Some(rat(7, 12)));
    assert_eq!(plan.warp_boundary(3, half), Some(rat(17, 24)));
    // Beat anchors never move.
    for beat in 0..4 {
        assert_eq!(
            plan.warp_boundary(beat * 4, half),
            Some(Rat::int(beat as i128))
        );
    }
    // Endpoint: survivors at exactly 1/3 and 2/3; the loser collides at 2/3.
    let full = MorphAmount::new(100).expect("valid amount");
    assert_eq!(plan.warp_boundary(1, full), Some(rat(1, 3)));
    assert_eq!(plan.warp_boundary(2, full), Some(rat(2, 3)));
    assert_eq!(plan.warp_boundary(3, full), Some(rat(2, 3)));
    assert_eq!(plan.warp_boundary(16, full), Some(Rat::int(4)));
}

// ---------------------------------------------------------------------------
// Endpoint projection
// ---------------------------------------------------------------------------

#[test]
fn default_endpoint_projects_twelve_cells_with_provenance() {
    let pattern = straight_sixteen();
    let plan = plan_of(&pattern);
    let phrase = normalize_source(&pattern).expect("eligible");
    let derived = project_endpoint(&pattern, &phrase, &plan).expect("projection");
    let sources: Vec<usize> = derived.cells.iter().map(|cell| cell.source_step).collect();
    assert_eq!(sources, vec![0, 1, 3, 4, 5, 7, 8, 9, 11, 12, 13, 15]);
    for cell in &derived.cells {
        assert_eq!(cell.role, DerivedCellRole::Attack);
    }
}

#[test]
fn ephemeral_endpoint_pattern_is_a_valid_twelve_step_triplet() {
    let pattern = straight_sixteen();
    let plan = plan_of(&pattern);
    let phrase = normalize_source(&pattern).expect("eligible");
    let derived = project_endpoint(&pattern, &phrase, &plan).expect("projection");
    let ephemeral = endpoint_as_ephemeral_pattern(&derived).expect("valid ephemeral");
    assert!(ephemeral.triplet);
    assert_eq!(ephemeral.active_steps, 12);
    assert!(ephemeral.validate().is_ok());
}

#[test]
fn broken_slide_edge_is_cleared_in_the_endpoint() {
    let mut pattern = straight_sixteen();
    // S2 slides into S3 with no onward chain. Losing S4 is invalid
    // because next-beat S1 continues it, and losing accented S2 costs
    // both the edge and an accent, so S3 loses and the S2 -> S3 edge
    // breaks without a valid contraction.
    pattern.step[1].slide = Slide::On;
    pattern.step[1].accent = Accent::On;
    pattern.step[4].time = Time::Tie;
    let plan = plan_of(&pattern);
    assert_eq!(plan.beats[0].loser, 2);
    assert_eq!(plan.loss.lost_slide_connectivity, 1);

    let phrase = normalize_source(&pattern).expect("eligible");
    let derived = project_endpoint(&pattern, &phrase, &plan).expect("projection");
    let s2 = derived
        .cells
        .iter()
        .find(|cell| cell.source_step == 1)
        .expect("S2 survives");
    assert_eq!(
        s2.step.slide,
        Slide::Off,
        "no invented slide onto the newly adjacent attack"
    );
}
