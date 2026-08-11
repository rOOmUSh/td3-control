use thiserror::Error;

/// Typed failures produced by triplet morph planning and projection.
///
/// A planning error never writes device memory, never changes BPM, and
/// never reports audition success.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MorphPlanError {
    #[error("triplet morph amount out of range: {0} (must be 0..=100)")]
    AmountOutOfRange(u32),

    #[error("triplet morph requires 4, 8, 12, or 16 active steps, got {0}")]
    UnsupportedActiveSteps(u8),

    #[error("triplet morph requires a straight pattern, native triplet is enabled")]
    NativeTripletEnabled,

    #[error("triplet morph source pattern invalid: {0}")]
    InvalidSource(String),

    #[error("triplet morph found no valid whole-pattern endpoint candidate")]
    NoValidCandidate,

    #[error("triplet morph endpoint projection invalid: {0}")]
    InvalidEndpointProjection(String),

    #[error("triplet morph timing overflow")]
    TimingOverflow,

    #[error("triplet morph event rounded outside the cycle")]
    EventOutsideCycle,
}
