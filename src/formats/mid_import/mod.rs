//! MIDI (.mid) → TD-3 Pattern import.
//!
//! Reverses what `formats::mid::export` produces and is permissive enough to
//! accept arbitrary DAW output. Strategy:
//!
//! 1. Parse the raw SMF bytes into a flat list of (tick, event) pairs.
//! 2. Pair note-on/note-off into `NoteEvent { start_tick, end_tick, pitch,
//!    velocity }` entries.
//! 3. Auto-detect whether the grid is quarter-subdivisions (ppqn/4) or triplet
//!    (ppqn/3) by checking which quantisation best aligns the note-on ticks.
//! 4. Snap each note-on to the nearest step. If multiple pitches land on the
//!    same step, consult the `PolyphonyResolver`.
//! 5. Derive per-step fields:
//!       - Normal: a note starts at this step.
//!       - Tie:    no note starts but a prior note is still ringing.
//!       - Rest:   silence at this step.
//!       - Slide:  Normal step whose gate length covers most of its group
//!         (≥ ~75% of the span to the next note-on or group end).
//!       - Accent: note-on velocity ≥ `accent_threshold`.
//!       - Transpose + note: reverse of the exporter pitch mapping.
//!         Pitches outside the 3-octave range are clamped by octave.

use crate::error::Td3Error;
use crate::formats::mid::{
    DEFAULT_MIDI_ACCENT_VELOCITY, DEFAULT_MIDI_NORMAL_VELOCITY, DEFAULT_MIDI_OCTAVE_OFFSET,
};
use crate::pattern::Pattern;
use crate::step;

mod mapping;
mod parse;
mod quantize;

use mapping::derive_pattern_steps;
use parse::parse_smf;
use quantize::{
    bin_to_steps, collect_note_events, derive_active_steps, detect_grid, detect_grid_for_empty,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Options controlling .mid → Pattern decoding.
#[derive(Debug, Clone)]
pub struct MidiImportOptions {
    /// Offset subtracted from each MIDI pitch to recover the TD-3 pitch.
    /// Must match the exporter's `octave_offset` for lossless round-trip
    /// (default 0, with MIDI 36 as TD-3 Normal-C).
    pub octave_offset: i8,
    /// Note-on velocity at or above this threshold is interpreted as an
    /// accent. The default sits halfway between the exporter's normal and
    /// accent velocities so both ends of the round-trip agree.
    pub accent_threshold: u8,
}

impl Default for MidiImportOptions {
    fn default() -> Self {
        let midpoint =
            ((DEFAULT_MIDI_NORMAL_VELOCITY as u16 + DEFAULT_MIDI_ACCENT_VELOCITY as u16) / 2) as u8;
        Self {
            octave_offset: DEFAULT_MIDI_OCTAVE_OFFSET,
            accent_threshold: midpoint,
        }
    }
}

impl MidiImportOptions {
    /// Build import options from a fully-resolved `AppEnv`. Reuses the
    /// `MIDI_EXPORT_*` keys so `.mid` round-trip stays lossless when the
    /// operator changes velocity or octave_offset in the env file.
    pub fn from_env(env: &crate::app_env::AppEnv) -> Self {
        let midpoint = ((env.midi_export_normal_velocity as u16
            + env.midi_export_accent_velocity as u16)
            / 2) as u8;
        Self {
            octave_offset: env.midi_export_octave_offset,
            accent_threshold: midpoint,
        }
    }
}

/// A single candidate note-on at a step boundary - shown to the resolver
/// when two or more pitches start at the same step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolyphonyCandidate {
    pub midi_pitch: u8,
    pub velocity: u8,
}

/// Strategy for reducing polyphonic starts to a single monophonic step.
///
/// The TD-3 is monophonic, so when the imported MIDI has two note-ons at the
/// same tick the caller must pick one. CLI wires this to a stdin menu; tests
/// wire scripted picks; a web UI can supply its own implementation later.
pub trait PolyphonyResolver {
    /// Called once per polyphonic step. `step_index` is 0-based; `candidates`
    /// is already sorted by ascending pitch. The implementation must return a
    /// valid index into `candidates`.
    fn choose(
        &mut self,
        step_index: usize,
        candidates: &[PolyphonyCandidate],
    ) -> Result<usize, Td3Error>;
}

/// Always return the lowest-pitch candidate. Useful for tests and for
/// deterministic fallback paths where no user input is available.
pub struct LowestPitchResolver;

impl PolyphonyResolver for LowestPitchResolver {
    fn choose(
        &mut self,
        _step_index: usize,
        _candidates: &[PolyphonyCandidate],
    ) -> Result<usize, Td3Error> {
        Ok(0)
    }
}

/// Reject any polyphony as an error. Useful when the caller has no way to
/// prompt for a choice and would rather fail loudly.
#[allow(dead_code)]
pub struct RejectPolyphonyResolver;

impl PolyphonyResolver for RejectPolyphonyResolver {
    fn choose(
        &mut self,
        step_index: usize,
        candidates: &[PolyphonyCandidate],
    ) -> Result<usize, Td3Error> {
        Err(Td3Error::FormatError(format!(
            "polyphony at step {}: {} simultaneous notes, but no resolver is configured",
            step_index + 1,
            candidates.len()
        )))
    }
}

/// Decode raw SMF bytes into a validated TD-3 Pattern.
pub fn import(
    bytes: &[u8],
    options: &MidiImportOptions,
    resolver: &mut dyn PolyphonyResolver,
) -> Result<Pattern, Td3Error> {
    let parsed = parse_smf(bytes)?;
    let note_events = collect_note_events(&parsed.events)?;
    if note_events.is_empty() {
        return import_empty_pattern(parsed.ppqn, parsed.last_tick);
    }

    let (step_ticks, triplet) = detect_grid(parsed.ppqn, &note_events)?;
    let steps_by_index = bin_to_steps(&note_events, step_ticks);
    let active_steps = derive_active_steps(&steps_by_index, parsed.last_tick, step_ticks);

    let steps = derive_pattern_steps(
        &steps_by_index,
        &note_events,
        step_ticks,
        active_steps,
        options,
        resolver,
    )?;

    Pattern::new(triplet, active_steps, steps)
}

/// Build an empty Pattern for a .mid that contains no note-on events.
/// Real TD-3 banks frequently include empty patterns used as recording
/// markers, so rejecting them on import would break the .mid round-trip
/// for legitimate user data. Grid and length are recovered from the
/// End-of-Track tick when possible; otherwise the device default (16-step
/// straight) is used as a safe fallback so the resulting Pattern is still
/// valid.
///
/// Step time is set to `TieRest` because the .mid exporter collapses
/// `Tie`, `Rest`, and `TieRest` into the same byte stream (none of them
/// emit MIDI events), so the source representation is unrecoverable from
/// the .mid alone. Empirically, TD-3 hardware stores cleared/marker
/// patterns with both the tie and rest bits set, so choosing `TieRest`
/// matches the device's own convention on the round-trip.
fn import_empty_pattern(ppqn: u32, last_tick: u32) -> Result<Pattern, Td3Error> {
    let (step_ticks, triplet) = detect_grid_for_empty(ppqn, last_tick).unwrap_or_else(|| {
        let fallback_step = if ppqn.is_multiple_of(4) {
            ppqn / 4
        } else if ppqn.is_multiple_of(3) {
            ppqn / 3
        } else {
            ppqn.max(1)
        };
        (fallback_step, false)
    });
    let empty_buckets: Vec<Vec<usize>> = (0..16).map(|_| Vec::new()).collect();
    let active_steps = derive_active_steps(&empty_buckets, last_tick, step_ticks);
    let mut steps = [step::Step::default(); 16];
    for s in steps.iter_mut() {
        s.time = step::Time::TieRest;
    }
    Pattern::new(triplet, active_steps, steps)
}
