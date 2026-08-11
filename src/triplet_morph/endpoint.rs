use crate::pattern::Pattern;
use crate::step::{Slide, Step, Time};

use super::error::MorphPlanError;
use super::phrase::{SourcePhrase, PATTERN_CELL_COUNT, STEPS_PER_BEAT};
use super::plan::TripletMorphPlan;

/// Derived target cells per beat.
pub const TARGET_CELLS_PER_BEAT: usize = 3;

/// Semantic role of one derived endpoint cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivedCellRole {
    /// A Normal cell that starts a note attack.
    Attack,
    /// A tie continuing a surviving attack.
    Continuation,
    /// A rest boundary.
    Silence,
    /// A source-orphan tie canonicalized to silence.
    OrphanSilence,
}

/// One derived target cell with source provenance. Read-only projection
/// data, not canonical editor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedCell {
    pub target_index: usize,
    pub source_step: usize,
    pub role: DerivedCellRole,
    pub step: Step,
}

/// The 100% endpoint projection: three cells per source beat, with
/// source provenance and semantic roles. Never persisted, exported, or
/// sent to device memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedTripletPattern {
    pub cells: Vec<DerivedCell>,
}

/// Project the endpoint target-cell view from the immutable source and
/// plan.
///
/// Surviving cells keep their own pitch, transpose, accent, and slide
/// semantics. A slide flag is cleared when its source connection is lost
/// without a valid chain contraction, so no invented slide relationship
/// can appear between newly adjacent attacks. Orphan ties canonicalize
/// to explicit rests.
pub fn project_endpoint(
    pattern: &Pattern,
    phrase: &SourcePhrase,
    plan: &TripletMorphPlan,
) -> Result<DerivedTripletPattern, MorphPlanError> {
    let target_cells = plan.beat_count() * TARGET_CELLS_PER_BEAT;
    let mut survivors: Vec<usize> = Vec::with_capacity(target_cells);
    for (beat, beat_plan) in plan.beats.iter().enumerate() {
        survivors.push(beat * STEPS_PER_BEAT);
        survivors.push(beat_plan.selected[0]);
        survivors.push(beat_plan.selected[1]);
    }
    if survivors.len() != target_cells {
        return Err(MorphPlanError::InvalidEndpointProjection(format!(
            "expected {} surviving cells, got {}",
            target_cells,
            survivors.len()
        )));
    }

    let losers: Vec<usize> = plan.beats.iter().map(|beat| beat.loser).collect();
    let lost = |step: usize| losers.contains(&step);

    let mut cells: Vec<DerivedCell> = Vec::with_capacity(target_cells);
    for (target_index, &source_step) in survivors.iter().enumerate() {
        if source_step >= phrase.active_steps {
            return Err(MorphPlanError::InvalidEndpointProjection(format!(
                "surviving source step {} out of range",
                source_step
            )));
        }
        let boundary = &phrase.boundaries[source_step];
        let mut step = pattern.step[source_step];

        let role = match boundary.time {
            Time::Normal => DerivedCellRole::Attack,
            Time::Tie if boundary.orphan_tie => {
                step.time = Time::Rest;
                DerivedCellRole::OrphanSilence
            }
            Time::Tie => DerivedCellRole::Continuation,
            Time::Rest | Time::TieRest => DerivedCellRole::Silence,
        };

        cells.push(DerivedCell {
            target_index,
            source_step,
            role,
            step,
        });
    }

    // Slide flags: keep only connections traceable to a preserved source
    // edge or a valid contraction. A slide with no source edge keeps its
    // full-gate meaning only while its derived successor is not an attack.
    for index in 0..cells.len() {
        let cell = cells[index];
        if cell.role != DerivedCellRole::Attack || cell.step.slide != Slide::On {
            continue;
        }
        let edge = phrase
            .slide_edges
            .iter()
            .find(|(from, _)| *from == cell.source_step);
        let keep = match edge {
            Some(&(_, to)) => {
                if lost(to) {
                    let onward = phrase.slide_edges.iter().find(|(from, _)| *from == to);
                    match onward {
                        Some(&(_, chain_end)) => !lost(chain_end),
                        None => false,
                    }
                } else {
                    true
                }
            }
            None => {
                let next_is_attack = cells
                    .get(index + 1)
                    .is_some_and(|next| next.role == DerivedCellRole::Attack);
                !next_is_attack
            }
        };
        if !keep {
            cells[index].step.slide = Slide::Off;
        }
    }

    Ok(DerivedTripletPattern { cells })
}

/// Convert the endpoint projection to an ephemeral valid Pattern with
/// `triplet = true` and three active steps per source beat, so the
/// established triplet MIDI
/// builder defines the audible endpoint. The returned pattern is never
/// exposed as canonical state, persisted, exported, or written to the
/// device.
pub fn endpoint_as_ephemeral_pattern(
    derived: &DerivedTripletPattern,
) -> Result<Pattern, MorphPlanError> {
    if derived.cells.is_empty() || derived.cells.len() > PATTERN_CELL_COUNT {
        return Err(MorphPlanError::InvalidEndpointProjection(format!(
            "derived cell count {} outside 1..={}",
            derived.cells.len(),
            PATTERN_CELL_COUNT
        )));
    }
    let mut steps: [Step; PATTERN_CELL_COUNT] = Default::default();
    for (index, cell) in derived.cells.iter().enumerate() {
        steps[index] = cell.step;
    }
    Pattern::new(true, derived.cells.len() as u8, steps)
        .map_err(|err| MorphPlanError::InvalidEndpointProjection(err.to_string()))
}
