use serde::{Deserialize, Serialize};

use crate::triplet_morph::{DerivedCellRole, DerivedTripletPattern, TripletMorphPlan};

use super::{WebPattern, WebStep};

// ---------------------------------------------------------------------------
// Triplet morph plan (MIDI-independent planning endpoint)
// ---------------------------------------------------------------------------
//
// POST /api/pattern/triplet-morph/plan returns the deterministic morph
// plan for one canonical WebPattern so the browser can render derived
// views by pure visual interpolation. Rust remains the only normative
// planner; JavaScript never duplicates priority logic.

#[derive(Deserialize)]
pub struct TripletMorphPlanRequest {
    pub pattern: WebPattern,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TripletMorphPlanResponse {
    pub eligible: bool,
    /// Typed reason text when the source is ineligible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<TripletMorphPlanBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TripletMorphPlanBody {
    pub plan_version: u16,
    pub beats: Vec<MorphBeatPlanDto>,
    pub assignments: Vec<MorphAssignmentDto>,
    pub endpoint_cells: Vec<MorphEndpointCellDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MorphBeatPlanDto {
    pub beat: usize,
    pub pair_rank: u8,
    pub selected: [usize; 2],
    pub loser: usize,
}

/// Exact rational value as integer numerator and denominator.
#[derive(Serialize)]
pub struct MorphRationalDto {
    pub num: i64,
    pub den: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MorphAssignmentDto {
    pub step: usize,
    pub survivor: bool,
    /// Offset inside the owning beat at amount 0, in beats.
    pub source_offset: MorphRationalDto,
    /// Offset inside the owning beat at amount 100, in beats.
    pub target_offset: MorphRationalDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MorphEndpointCellDto {
    pub target_index: usize,
    pub source_step: usize,
    pub role: &'static str,
    pub step: WebStep,
}

fn rational_dto(value: crate::triplet_morph::Rat) -> MorphRationalDto {
    // Planner rationals are tiny reduced fractions; the saturating
    // fallback can never trigger for valid plans but avoids panicking
    // conversions.
    MorphRationalDto {
        num: i64::try_from(value.num()).unwrap_or(i64::MAX),
        den: i64::try_from(value.den()).unwrap_or(i64::MAX),
    }
}

fn role_name(role: DerivedCellRole) -> &'static str {
    match role {
        DerivedCellRole::Attack => "attack",
        DerivedCellRole::Continuation => "continuation",
        DerivedCellRole::Silence => "silence",
        DerivedCellRole::OrphanSilence => "orphanSilence",
    }
}

impl TripletMorphPlanBody {
    pub fn from_plan(plan: &TripletMorphPlan, endpoint: &DerivedTripletPattern) -> Self {
        Self {
            plan_version: plan.version,
            beats: plan
                .beats
                .iter()
                .map(|beat| MorphBeatPlanDto {
                    beat: beat.beat,
                    pair_rank: beat.pair_rank,
                    selected: beat.selected,
                    loser: beat.loser,
                })
                .collect(),
            assignments: plan
                .assignments
                .iter()
                .map(|assignment| MorphAssignmentDto {
                    step: assignment.step,
                    survivor: assignment.survivor,
                    source_offset: rational_dto(assignment.source_offset),
                    target_offset: rational_dto(assignment.target_offset),
                })
                .collect(),
            endpoint_cells: endpoint
                .cells
                .iter()
                .map(|cell| MorphEndpointCellDto {
                    target_index: cell.target_index,
                    source_step: cell.source_step,
                    role: role_name(cell.role),
                    step: WebStep::from_step(&cell.step),
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Audition response diagnostics
// ---------------------------------------------------------------------------

/// Optional response diagnostics for a morph-aware audition request.
/// Legacy requests omit the morph field and receive none of these.
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct TripletMorphDiagnostics {
    pub triplet_morph_percent: u32,
    pub triplet_morph_plan_version: u16,
    /// "currentCycleFuture" or "nextCycle".
    pub triplet_morph_apply_mode: &'static str,
    /// Wall-clock epoch of the first cycle fully governed by the amount.
    pub triplet_morph_fully_applied_epoch_micros: u64,
}
