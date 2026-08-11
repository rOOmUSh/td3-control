use std::collections::BTreeSet;

use crate::error::Td3Error;
use crate::triplet_morph::MorphEventId;
use crate::web::midi_channel::channel_status;

use super::schedule::{AuditionSchedule, ScheduledMidi};

/// Stable event identities dispatched in the current cycle. An event
/// whose identity is recorded here is consumed for the rest of the
/// cycle: a schedule replacement can never replay it even if its new
/// offset moved into the future. Cleared at every cycle rollover after
/// boundary Note Off handling. Events without identities (legacy
/// schedules) are never recorded or skipped.
pub(crate) type DispatchLedger = BTreeSet<MorphEventId>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DueEventsResult {
    Complete,
    ApplyPendingUpdate,
}

fn already_dispatched(ledger: &DispatchLedger, event: &ScheduledMidi) -> bool {
    event.event_id.is_some_and(|id| ledger.contains(&id))
}

fn record_dispatch(ledger: &mut DispatchLedger, event: &ScheduledMidi) {
    if let Some(id) = event.event_id {
        ledger.insert(id);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn send_due_events_until_update_boundary<F>(
    schedule: &AuditionSchedule,
    next_event: &mut usize,
    sounding: &mut BTreeSet<u8>,
    ledger: &mut DispatchLedger,
    due_offset: u64,
    pending_update: bool,
    mut send: F,
) -> Result<DueEventsResult, Td3Error>
where
    F: FnMut(&[u8]) -> Result<(), Td3Error>,
{
    while *next_event < schedule.events.len()
        && schedule.events[*next_event].offset_us <= due_offset
    {
        let ev = &schedule.events[*next_event];
        if already_dispatched(ledger, ev) {
            *next_event += 1;
            continue;
        }
        send(&ev.bytes)?;
        track_sounding(sounding, &ev.bytes);
        record_dispatch(ledger, ev);
        *next_event += 1;
        if pending_update && sounding.is_empty() {
            return Ok(DueEventsResult::ApplyPendingUpdate);
        }
    }

    Ok(DueEventsResult::Complete)
}

pub(crate) fn send_events_at_exact_offset<F>(
    schedule: &AuditionSchedule,
    next_event: &mut usize,
    sounding: &mut BTreeSet<u8>,
    ledger: &mut DispatchLedger,
    offset_us: u64,
    mut send: F,
) -> Result<(), Td3Error>
where
    F: FnMut(&[u8]) -> Result<(), Td3Error>,
{
    while *next_event < schedule.events.len() && schedule.events[*next_event].offset_us < offset_us
    {
        *next_event += 1;
    }

    while *next_event < schedule.events.len() && schedule.events[*next_event].offset_us == offset_us
    {
        let event = &schedule.events[*next_event];
        if already_dispatched(ledger, event) {
            *next_event += 1;
            continue;
        }
        send(&event.bytes)?;
        track_sounding(sounding, &event.bytes);
        record_dispatch(ledger, event);
        *next_event += 1;
    }

    Ok(())
}

pub(crate) fn flush_note_offs_through_offset<F>(
    schedule: &AuditionSchedule,
    next_event: &mut usize,
    sounding: &mut BTreeSet<u8>,
    ledger: &mut DispatchLedger,
    offset_us: u64,
    mut send: F,
) -> Result<(), Td3Error>
where
    F: FnMut(&[u8]) -> Result<(), Td3Error>,
{
    while *next_event < schedule.events.len() && schedule.events[*next_event].offset_us <= offset_us
    {
        let event = &schedule.events[*next_event];
        if is_note_off(&event.bytes) && !already_dispatched(ledger, event) {
            send(&event.bytes)?;
            track_sounding(sounding, &event.bytes);
            record_dispatch(ledger, event);
        }
        *next_event += 1;
    }

    Ok(())
}

pub(super) fn next_event_index(schedule: &AuditionSchedule, phase_us: u64) -> usize {
    schedule
        .events
        .partition_point(|event| event.offset_us < phase_us)
}

/// Update the sounding-note set from an outbound MIDI message. A Note On
/// with non-zero velocity adds the pitch; a Note Off, or a Note On with
/// zero velocity (running-status note off), removes it.
fn track_sounding(sounding: &mut BTreeSet<u8>, bytes: &[u8]) {
    let (Some(&status), Some(&note)) = (bytes.first(), bytes.get(1)) else {
        return;
    };
    match status & 0xF0 {
        0x90 if bytes.get(2).copied().unwrap_or(0) > 0 => {
            sounding.insert(note);
        }
        0x80 => {
            sounding.remove(&note);
        }
        0x90 => {
            sounding.remove(&note);
        }
        _ => {}
    }
}

fn is_note_off(bytes: &[u8]) -> bool {
    match bytes.first().map(|status| status & 0xF0) {
        Some(0x80) => true,
        Some(0x90) => bytes.get(2).copied().unwrap_or(0) == 0,
        _ => false,
    }
}

/// Silence every pitch left sounding, then send All Notes Off as a
/// belt-and-suspenders guard.
///
/// `channel` is 1 through 16 and must be the channel the schedule
/// sounded on: a device configured for channel N discards a Note Off
/// addressed to any other channel, so a mismatch here leaves the note
/// ringing until the device is power-cycled.
pub(super) fn silence_all(
    out: &mut midir::MidiOutputConnection,
    sounding: &BTreeSet<u8>,
    channel: u8,
) {
    for &note in sounding {
        let _ = out.send(&[channel_status(0x80, channel), note, 64]);
    }
    // CC 123 (All Notes Off) on the audition channel.
    let _ = out.send(&[channel_status(0xB0, channel), 0x7B, 0x00]);
}
