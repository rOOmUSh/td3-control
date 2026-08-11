//! NO-LIVE triplet morph planning.
//!
//! Pure, deterministic planning for the reversible 16-to-12 audition
//! transform. The canonical pattern is never mutated: the plan, warped
//! boundary positions, and the endpoint projection are all derived from
//! an immutable source snapshot. This module is independent of Axum,
//! DOM concepts, wall-clock APIs, and MIDI output connections.

mod endpoint;
mod error;
mod loss;
mod phrase;
mod plan;
mod rational;

pub use endpoint::{
    endpoint_as_ephemeral_pattern, project_endpoint, DerivedCellRole, DerivedTripletPattern,
};
pub use error::MorphPlanError;
pub use phrase::{normalize_source, SourcePhrase};
pub use plan::{plan_triplet_morph, TripletMorphPlan};
pub use rational::Rat;

/// Validated morph knob amount, an integer from 0 through 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MorphAmount(u8);

impl MorphAmount {
    pub fn new(value: u32) -> Result<Self, MorphPlanError> {
        if value > 100 {
            return Err(MorphPlanError::AmountOutOfRange(value));
        }
        Ok(Self(value as u8))
    }

    pub fn value(self) -> u8 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn is_endpoint(self) -> bool {
        self.0 == 100
    }
}

/// MIDI event role used by stable morph event identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MorphEventRole {
    NoteOn,
    NoteOff,
    SlideTailNoteOff,
}

/// Stable scheduler identity of one scheduled MIDI event: the owning
/// source attack plus the event role. Scheduler metadata only, never a
/// MIDI byte and never part of device output. Unique within one cycle
/// because each attack emits at most one Note On and one release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MorphEventId {
    pub source_step: u8,
    pub role: MorphEventRole,
}
