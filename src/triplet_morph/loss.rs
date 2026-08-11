use std::cmp::Ordering;

use crate::step::Time;

use super::phrase::{SourceAttack, SourcePhrase};
use super::rational::Rat;

/// Lexicographically ordered musical loss for one whole-pattern
/// candidate. The first differing field decides the winner. No fields
/// are summed and no tunable weights exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossVector {
    pub lost_slide_connectivity: u32,
    pub lost_accented_attacks: u32,
    pub lost_articulation_cuts: u32,
    pub lost_contour_critical_attacks: u32,
    pub lost_attacks: u32,
    pub lost_continuation_quanta: u32,
    pub total_survivor_displacement: Rat,
    pub maximum_survivor_displacement: Rat,
    /// Per-beat pair rank in beat order. Final deterministic tie break.
    pub fallback_plan_rank: Vec<u8>,
}

impl Ord for LossVector {
    fn cmp(&self, other: &Self) -> Ordering {
        self.lost_slide_connectivity
            .cmp(&other.lost_slide_connectivity)
            .then_with(|| self.lost_accented_attacks.cmp(&other.lost_accented_attacks))
            .then_with(|| {
                self.lost_articulation_cuts
                    .cmp(&other.lost_articulation_cuts)
            })
            .then_with(|| {
                self.lost_contour_critical_attacks
                    .cmp(&other.lost_contour_critical_attacks)
            })
            .then_with(|| self.lost_attacks.cmp(&other.lost_attacks))
            .then_with(|| {
                self.lost_continuation_quanta
                    .cmp(&other.lost_continuation_quanta)
            })
            .then_with(|| {
                self.total_survivor_displacement
                    .cmp(&other.total_survivor_displacement)
            })
            .then_with(|| {
                self.maximum_survivor_displacement
                    .cmp(&other.maximum_survivor_displacement)
            })
            .then_with(|| self.fallback_plan_rank.cmp(&other.fallback_plan_rank))
    }
}

impl PartialOrd for LossVector {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A losing attack is contour-critical when its pitch differs from both
/// nearest attack neighbors, is a strict local extremum, or its removal
/// changes the nonzero direction sequence of the local contour. Pitch
/// comparison uses effective pitch after transpose.
pub(super) fn is_contour_critical(phrase: &SourcePhrase, attack: &SourceAttack) -> bool {
    let prev = attack
        .prev_attack
        .and_then(|step| phrase.attack(step))
        .map(|neighbor| neighbor.pitch);
    let next = attack
        .next_attack
        .and_then(|step| phrase.attack(step))
        .map(|neighbor| neighbor.pitch);
    let (Some(prev), Some(next)) = (prev, next) else {
        return false;
    };
    let pitch = attack.pitch;

    let differs_from_both = pitch != prev && pitch != next;
    let strict_extremum = (pitch > prev && pitch > next) || (pitch < prev && pitch < next);
    let with_removed = nonzero_directions(&[pitch - prev, next - pitch]);
    let without = nonzero_directions(&[next - prev]);

    differs_from_both || strict_extremum || with_removed != without
}

fn nonzero_directions(deltas: &[i16]) -> Vec<i8> {
    deltas
        .iter()
        .filter(|delta| **delta != 0)
        .map(|delta| if *delta > 0 { 1i8 } else { -1i8 })
        .collect()
}

/// Count musical loss for one whole-pattern candidate described by its
/// four losing steps and per-beat pair ranks.
pub(super) fn candidate_loss(
    phrase: &SourcePhrase,
    losers: &[usize],
    ranks: &[u8],
    per_beat_displacement: impl Fn(u8) -> (Rat, Rat),
) -> LossVector {
    let lost = |step: usize| losers.contains(&step);

    let mut loss = LossVector {
        lost_slide_connectivity: 0,
        lost_accented_attacks: 0,
        lost_articulation_cuts: 0,
        lost_contour_critical_attacks: 0,
        lost_attacks: 0,
        lost_continuation_quanta: 0,
        total_survivor_displacement: Rat::ZERO,
        maximum_survivor_displacement: Rat::ZERO,
        fallback_plan_rank: ranks.to_vec(),
    };

    for &loser in losers {
        let boundary = &phrase.boundaries[loser];
        match boundary.time {
            Time::Normal => {
                loss.lost_attacks += 1;
                if let Some(attack) = phrase.attack(loser) {
                    if attack.accent {
                        loss.lost_accented_attacks += 1;
                    }
                    if is_contour_critical(phrase, attack) {
                        loss.lost_contour_critical_attacks += 1;
                    }
                }
            }
            Time::Tie => {
                if let Some(owner) = boundary.continues_owner {
                    if !lost(owner) {
                        loss.lost_continuation_quanta += 1;
                    }
                }
            }
            Time::Rest | Time::TieRest => {
                if boundary.cuts_sounding {
                    loss.lost_articulation_cuts += 1;
                }
            }
        }
    }

    for &(from, to) in &phrase.slide_edges {
        if !lost(from) && !lost(to) {
            continue;
        }
        if !contraction_covers(phrase, &lost, from, to) {
            loss.lost_slide_connectivity += 1;
        }
    }

    for &rank in ranks {
        let (total, maximum) = per_beat_displacement(rank);
        loss.total_survivor_displacement = loss.total_survivor_displacement.add(total);
        if maximum > loss.maximum_survivor_displacement {
            loss.maximum_survivor_displacement = maximum;
        }
    }

    loss
}

/// A broken edge is covered only by a valid A -> B -> C contraction:
/// the dropped endpoint is the middle of one uninterrupted source slide
/// chain, both original edges exist, and A and C survive. Edge existence
/// already guarantees no rest lies inside the chain.
fn contraction_covers(
    phrase: &SourcePhrase,
    lost: &impl Fn(usize) -> bool,
    from: usize,
    to: usize,
) -> bool {
    if lost(from) && lost(to) {
        return false;
    }
    let middle = if lost(from) { from } else { to };
    let incoming = phrase
        .slide_edges
        .iter()
        .find(|(_, edge_to)| *edge_to == middle);
    let outgoing = phrase
        .slide_edges
        .iter()
        .find(|(edge_from, _)| *edge_from == middle);
    match (incoming, outgoing) {
        (Some(&(chain_start, _)), Some(&(_, chain_end))) => !lost(chain_start) && !lost(chain_end),
        _ => false,
    }
}
