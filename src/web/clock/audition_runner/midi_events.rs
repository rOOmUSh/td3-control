use std::collections::BTreeSet;

use crate::error::Td3Error;

use super::schedule::AuditionSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DueEventsResult {
    Complete,
    ApplyPendingUpdate,
}

pub(crate) fn send_due_events_until_update_boundary<F>(
    schedule: &AuditionSchedule,
    next_event: &mut usize,
    sounding: &mut BTreeSet<u8>,
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
        send(&ev.bytes)?;
        track_sounding(sounding, &ev.bytes);
        *next_event += 1;
        if pending_update && sounding.is_empty() {
            return Ok(DueEventsResult::ApplyPendingUpdate);
        }
    }

    Ok(DueEventsResult::Complete)
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

/// Silence every pitch left sounding, then send All Notes Off as a
/// belt-and-suspenders guard. Channel 1 (`0x80`/`0xB0` status) matches
/// the audition output channel.
pub(super) fn silence_all(out: &mut midir::MidiOutputConnection, sounding: &BTreeSet<u8>) {
    for &note in sounding {
        let _ = out.send(&[0x80, note, 64]);
    }
    // CC 123 (All Notes Off) on channel 1.
    let _ = out.send(&[0xB0, 0x7B, 0x00]);
}
