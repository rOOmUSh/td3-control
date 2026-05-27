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
//! The note schedule is produced by reusing the `.mid` export timeline
//! ([`crate::formats::mid::build_timeline`]) so ties, rests, accent
//! velocity, gate length, and slide-overlap match the exporter and the
//! keyboard preview byte-for-byte.

mod commands;
mod handle;
mod midi_events;
mod playback;
mod schedule;
mod updates;

pub use handle::AuditionRunner;
pub use schedule::{prepare_schedule, AuditionSchedule, ScheduledMidi};

#[cfg(test)]
pub(crate) use midi_events::{send_due_events_until_update_boundary, DueEventsResult};
