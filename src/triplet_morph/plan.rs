use crate::pattern::Pattern;

use super::error::MorphPlanError;
use super::loss::{candidate_loss, LossVector};
use super::phrase::{normalize_source, SourcePhrase, STEPS_PER_BEAT};
use super::rational::Rat;
use super::MorphAmount;

/// Version 2 added collision retirement: a losing attack is dropped once
/// it comes within the retirement floor of the next attack, instead of
/// sounding as an unresolvable retrigger down to exactly 100%.
pub const MORPH_PLAN_VERSION: u16 = 2;

const PAIR_RANK_COUNT: usize = 3;

/// One beat's collision decision: the two surviving offbeat source steps
/// and the one losing step, identified by absolute source step index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatPlan {
    pub beat: usize,
    /// Index into the fixed fallback pair order:
    /// 0 = S2+S4, 1 = S2+S3, 2 = S3+S4.
    pub pair_rank: u8,
    /// Earlier and later surviving offbeat steps, mapped to 1/3 and 2/3.
    pub selected: [usize; 2],
    pub loser: usize,
}

/// Source and target offsets of one source boundary inside its beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryAssignment {
    pub step: usize,
    pub survivor: bool,
    /// Offset inside the owning beat, in beats, at amount 0.
    pub source_offset: Rat,
    /// Offset inside the owning beat, in beats, at amount 100.
    pub target_offset: Rat,
}

/// Deterministic whole-gesture morph plan computed once from an
/// immutable source snapshot. Winners never change with knob amount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TripletMorphPlan {
    pub version: u16,
    /// One entry per beat, 1 through 4 of them.
    pub beats: Vec<BeatPlan>,
    /// One entry per live source step, four per beat.
    pub assignments: Vec<BoundaryAssignment>,
    pub loss: LossVector,
}

impl TripletMorphPlan {
    /// Beats spanned by this plan, 1 through 4.
    pub fn beat_count(&self) -> usize {
        self.beats.len()
    }

    /// Global bar position, in beats, of a source boundary warped by
    /// `amount`: W(m, s, t) = s + m * (t - s), plus the beat index.
    pub fn warp_boundary(&self, source_step: usize, amount: MorphAmount) -> Option<Rat> {
        if source_step == self.assignments.len() {
            return Some(Rat::int(self.beat_count() as i128));
        }
        let assignment = self.assignments.get(source_step)?;
        let beat = Rat::int((source_step / STEPS_PER_BEAT) as i128);
        let m = Rat::new(i128::from(amount.value()), 100)?;
        let local = assignment
            .source_offset
            .add(m.mul(assignment.target_offset.sub(assignment.source_offset)));
        Some(beat.add(local))
    }
}

/// Local source offset of one of the four cells inside a beat.
fn source_offset(local: usize) -> Rat {
    Rat::new(local as i128, 4).unwrap_or(Rat::ZERO)
}

struct PairChoice {
    /// Local cell indices (1..=3) of the earlier and later survivors.
    survivors: [usize; 2],
    /// Local cell index of the loser.
    loser: usize,
    /// Loser forward-collision destination offset inside the beat.
    loser_target: Rat,
}

fn pair_choice(rank: u8) -> PairChoice {
    match rank {
        // S2 + S4 survive; S3 collides forward with S4 at 2/3.
        0 => PairChoice {
            survivors: [1, 3],
            loser: 2,
            loser_target: Rat::new(2, 3).unwrap_or(Rat::ONE),
        },
        // S2 + S3 survive; S4 collides forward with the beat boundary.
        1 => PairChoice {
            survivors: [1, 2],
            loser: 3,
            loser_target: Rat::ONE,
        },
        // S3 + S4 survive; S2 collides forward with S3 at 1/3.
        _ => PairChoice {
            survivors: [2, 3],
            loser: 1,
            loser_target: Rat::new(1, 3).unwrap_or(Rat::ZERO),
        },
    }
}

fn survivor_targets() -> [Rat; 2] {
    [
        Rat::new(1, 3).unwrap_or(Rat::ZERO),
        Rat::new(2, 3).unwrap_or(Rat::ONE),
    ]
}

/// Total and maximum survivor displacement contributed by one beat's
/// pair choice, in beats.
fn pair_displacement(rank: u8) -> (Rat, Rat) {
    let choice = pair_choice(rank);
    let targets = survivor_targets();
    let early = source_offset(choice.survivors[0]).sub(targets[0]).abs();
    let late = source_offset(choice.survivors[1]).sub(targets[1]).abs();
    (early.add(late), early.max(late))
}

/// Compute the deterministic morph plan for an eligible source pattern.
///
/// Evaluates every 3^beats strict-anchor whole-pattern candidate (3, 9,
/// 27, or 81) so ties and slides crossing beat boundaries influence
/// selection, then picks the lexicographically smallest loss vector.
pub fn plan_triplet_morph(pattern: &Pattern) -> Result<TripletMorphPlan, MorphPlanError> {
    let phrase = normalize_source(pattern)?;
    plan_from_phrase(&phrase)
}

pub(super) fn plan_from_phrase(phrase: &SourcePhrase) -> Result<TripletMorphPlan, MorphPlanError> {
    let beat_count = phrase.beat_count;
    let mut best: Option<(Vec<u8>, LossVector)> = None;

    // 3^beat_count complete candidates: 3, 9, 27, or 81.
    for candidate in 0..PAIR_RANK_COUNT.pow(beat_count as u32) {
        let ranks = candidate_ranks(candidate, beat_count);
        let losers = candidate_losers(&ranks);
        if !candidate_is_valid(phrase, &losers) {
            continue;
        }
        let loss = candidate_loss(phrase, &losers, &ranks, pair_displacement);
        let better = match &best {
            Some((_, best_loss)) => loss < *best_loss,
            None => true,
        };
        if better {
            best = Some((ranks, loss));
        }
    }

    let (ranks, loss) = best.ok_or(MorphPlanError::NoValidCandidate)?;
    Ok(build_plan(&ranks, loss))
}

fn candidate_ranks(candidate: usize, beat_count: usize) -> Vec<u8> {
    let mut ranks = vec![0u8; beat_count];
    let mut remaining = candidate;
    for rank in ranks.iter_mut() {
        *rank = (remaining % PAIR_RANK_COUNT) as u8;
        remaining /= PAIR_RANK_COUNT;
    }
    ranks
}

fn candidate_losers(ranks: &[u8]) -> Vec<usize> {
    ranks
        .iter()
        .enumerate()
        .map(|(beat, &rank)| beat * STEPS_PER_BEAT + pair_choice(rank).loser)
        .collect()
}

/// A candidate is invalid when any surviving sounding tie would lose its
/// owner attack, including ties crossing a beat boundary onto a fixed S1.
fn candidate_is_valid(phrase: &SourcePhrase, losers: &[usize]) -> bool {
    let lost = |step: usize| losers.contains(&step);
    for boundary in &phrase.boundaries {
        if let Some(owner) = boundary.continues_owner {
            if !lost(boundary.step) && lost(owner) {
                return false;
            }
        }
    }
    true
}

fn build_plan(ranks: &[u8], loss: LossVector) -> TripletMorphPlan {
    let targets = survivor_targets();
    let mut beats: Vec<BeatPlan> = Vec::with_capacity(ranks.len());
    let mut assignments: Vec<BoundaryAssignment> = vec![
        BoundaryAssignment {
            step: 0,
            survivor: true,
            source_offset: Rat::ZERO,
            target_offset: Rat::ZERO,
        };
        ranks.len() * STEPS_PER_BEAT
    ];

    for (beat, &rank) in ranks.iter().enumerate() {
        let choice = pair_choice(rank);
        let base = beat * STEPS_PER_BEAT;
        beats.push(BeatPlan {
            beat,
            pair_rank: rank,
            selected: [base + choice.survivors[0], base + choice.survivors[1]],
            loser: base + choice.loser,
        });

        assignments[base] = BoundaryAssignment {
            step: base,
            survivor: true,
            source_offset: Rat::ZERO,
            target_offset: Rat::ZERO,
        };
        for (survivor_index, &local) in choice.survivors.iter().enumerate() {
            assignments[base + local] = BoundaryAssignment {
                step: base + local,
                survivor: true,
                source_offset: source_offset(local),
                target_offset: targets[survivor_index],
            };
        }
        assignments[base + choice.loser] = BoundaryAssignment {
            step: base + choice.loser,
            survivor: false,
            source_offset: source_offset(choice.loser),
            target_offset: choice.loser_target,
        };
    }

    TripletMorphPlan {
        version: MORPH_PLAN_VERSION,
        beats,
        assignments,
        loss,
    }
}
