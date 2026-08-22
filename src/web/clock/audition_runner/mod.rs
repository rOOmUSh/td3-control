//! Host-sequenced pattern audition: a dedicated OS thread that emits
//! timed MIDI Note On/Off for a pattern's steps without engaging the
//! device's internal sequencer.
//!
//! Unlike [`ClockRunner`](super::ClockRunner) this thread never sends
//! MIDI Start (0xFA), Clock (0xF8), or Stop (0xFC). The TD-3 sounds
//! each note from its synth voice purely from inbound channel-voice
//! MIDI, exactly like the single-note keyboard preview. Nothing is
//! written to device pattern memory, so the audition is non-destructive.
//!
//! The note schedule reuses the MIDI timeline's pitch, tie, rest, accent,
//! and slide semantics. Host audition can override the ordinary-note gate
//! while MIDI export retains its legacy half-step gate.

mod commands;
mod handle;
mod midi_events;
mod morph_intermediate;
mod morph_schedule;
mod playback;
mod schedule;
mod updates;

pub use commands::{
    AuditionApplyMode, AuditionUpdateAck, AuditionUpdateError, AuditionUpdateResult,
};
pub use handle::AuditionRunner;
#[cfg(test)]
pub(crate) use morph_intermediate::{
    COLLISION_RETIREMENT_AMOUNT_PERCENT, GATE_COMPENSATION_PEAK_DEN, GATE_COMPENSATION_PEAK_NUM,
    GATE_COMPENSATION_PEAK_PERCENT,
};
#[cfg(test)]
pub(crate) use morph_schedule::prepare_morph_schedule;
pub(crate) use morph_schedule::prepare_morph_schedule_with_lanes;
#[cfg(test)]
pub(crate) use schedule::prepare_schedule_with_gate;
pub(crate) use schedule::prepare_schedule_with_lanes;
#[cfg(test)]
pub(crate) use schedule::DEFAULT_AUDITION_CHANNEL;
pub use schedule::{prepare_schedule, AuditionSchedule, ScheduledMidi};

#[cfg(test)]
pub(crate) use commands::{
    coalescing_rejects_stale_without_losing_valid, deadline_drain_observes_queued_update,
};
#[cfg(test)]
pub(crate) use handle::{reject_closed_command_for_test, reject_closed_raw_send_for_test};
#[cfg(test)]
pub(crate) use midi_events::{send_due_events_until_update_boundary, DueEventsResult};
#[cfg(test)]
pub(crate) use playback::{
    flush_raw_sends_for_test, remaining_start_delay, AuditionTransitionTestHarness,
};
#[cfg(test)]
pub(crate) use updates::{cycle_timing_at_now, scale_cycle_phase, schedule_update_timing};
