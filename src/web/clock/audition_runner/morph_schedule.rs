//! Morph-aware host-audition schedule builder.
//!
//! Amount 0 rebuilds the straight schedule byte-and-offset equivalent
//! to `prepare_schedule_with_gate` and attaches stable event
//! identities. Amounts 1 through 99 use the rational warp builder in
//! `morph_intermediate`. Amount 100 projects the endpoint, converts it
//! to an ephemeral 12-active-step triplet pattern, and delegates MIDI
//! construction to the existing gate-aware triplet timeline builder.

use std::collections::BTreeMap;

use crate::error::Td3Error;
use crate::formats::mid::{build_timeline_with_gate, ORDER_SLIDE_NOTE_OFF};
use crate::pattern::Pattern;
use crate::triplet_morph::{
    endpoint_as_ephemeral_pattern, normalize_source, plan_triplet_morph, MorphAmount, MorphEventId,
    MorphEventRole, TripletMorphPlan,
};

use super::morph_intermediate::build_intermediate_schedule;
use super::schedule::{
    audition_options, tick_offset_us, AuditionSchedule, ScheduledMidi, AUDITION_PPQN,
};

/// Build the audition schedule for `pattern` at a morph `amount`.
///
/// The canonical pattern is never mutated: its `triplet` flag and BPM
/// are unchanged, and only the isolated ephemeral endpoint pattern has
/// `triplet = true`. Cycle period is exactly four beats for every
/// amount. Returns the schedule plus the deterministic plan used to
/// derive it.
pub(crate) fn prepare_morph_schedule(
    pattern: &Pattern,
    centibpm: u32,
    gate_percent: u32,
    amount: MorphAmount,
    channel: u8,
) -> Result<(AuditionSchedule, TripletMorphPlan), Td3Error> {
    let phrase = normalize_source(pattern)?;
    let plan = plan_triplet_morph(pattern)?;

    if amount.is_zero() {
        let mut provenance = [None; 16];
        for (cell, slot) in provenance.iter_mut().enumerate().take(phrase.active_steps) {
            *slot = Some(cell as u8);
        }
        let schedule =
            schedule_with_provenance_ids(pattern, centibpm, gate_percent, channel, &provenance)?;
        return Ok((schedule, plan));
    }

    if amount.is_endpoint() {
        let derived = crate::triplet_morph::project_endpoint(pattern, &phrase, &plan)?;
        let ephemeral = endpoint_as_ephemeral_pattern(&derived)?;
        let mut provenance = [None; 16];
        for (slot, cell) in provenance.iter_mut().zip(derived.cells.iter()) {
            *slot = Some(cell.source_step as u8);
        }
        let schedule =
            schedule_with_provenance_ids(&ephemeral, centibpm, gate_percent, channel, &provenance)?;
        return Ok((schedule, plan));
    }

    let schedule =
        build_intermediate_schedule(&phrase, &plan, centibpm, gate_percent, amount, channel)?;
    Ok((schedule, plan))
}

/// Rebuild the legacy timeline pipeline for `pattern` and attach a
/// stable `MorphEventId` to every channel-voice event. The event bytes,
/// offsets, and ordering are identical to
/// `prepare_schedule_with_gate` because the same timeline builder,
/// filter, stable sort key, and offset conversion are used.
///
/// `provenance` maps a cell index of `pattern` to the owning canonical
/// source step. Note Off identity follows the sounding pitch back to
/// the attack that started it, so equal-pitch connected slides resolve
/// to the first attack of the chain.
fn schedule_with_provenance_ids(
    pattern: &Pattern,
    centibpm: u32,
    gate_percent: u32,
    channel: u8,
    provenance: &[Option<u8>; 16],
) -> Result<AuditionSchedule, Td3Error> {
    let options = audition_options(centibpm, channel);
    let timeline = build_timeline_with_gate(pattern, "audition", &options, gate_percent)?;

    let mut events: Vec<(u32, u8, Vec<u8>)> = timeline
        .into_iter()
        .filter(|ev| matches!(ev.data.first().map(|b| b & 0xF0), Some(0x80) | Some(0x90)))
        .map(|ev| (ev.tick, ev.order, ev.data))
        .collect();
    events.sort_by_key(|(tick, order, _)| (*tick, *order));

    let divisor: u32 = if pattern.triplet { 3 } else { 4 };
    let step_ticks = AUDITION_PPQN as u32 / divisor;
    let pattern_ticks = (pattern.active_steps as u32).max(1) * step_ticks;
    let cycle_period_us = tick_offset_us(pattern_ticks, centibpm, AUDITION_PPQN);

    let mut sounding: BTreeMap<u8, u8> = BTreeMap::new();
    let mut scheduled: Vec<ScheduledMidi> = Vec::with_capacity(events.len());
    for (tick, order, bytes) in events {
        let event_id = event_identity(tick, order, &bytes, step_ticks, provenance, &mut sounding)?;
        scheduled.push(ScheduledMidi {
            offset_us: tick_offset_us(tick, centibpm, AUDITION_PPQN),
            bytes,
            event_id: Some(event_id),
        });
    }

    Ok(AuditionSchedule {
        events: scheduled,
        cycle_period_us,
        channel,
    })
}

fn event_identity(
    tick: u32,
    order: u8,
    bytes: &[u8],
    step_ticks: u32,
    provenance: &[Option<u8>; 16],
    sounding: &mut BTreeMap<u8, u8>,
) -> Result<MorphEventId, Td3Error> {
    let (Some(&status), Some(&note)) = (bytes.first(), bytes.get(1)) else {
        return Err(Td3Error::Midi(
            "morph identity: truncated channel-voice event".to_string(),
        ));
    };
    let velocity = bytes.get(2).copied().unwrap_or(0);
    let is_note_on = (status & 0xF0) == 0x90 && velocity > 0;

    if is_note_on {
        let cell = (tick / step_ticks.max(1)) as usize;
        let source_step = provenance.get(cell).copied().flatten().ok_or_else(|| {
            Td3Error::Midi(format!("morph identity: note on in unmapped cell {}", cell))
        })?;
        sounding.insert(note, source_step);
        return Ok(MorphEventId {
            source_step,
            role: MorphEventRole::NoteOn,
        });
    }

    let source_step = sounding.remove(&note).ok_or_else(|| {
        Td3Error::Midi(format!(
            "morph identity: note off for pitch {} with no sounding owner",
            note
        ))
    })?;
    let role = if order == ORDER_SLIDE_NOTE_OFF {
        MorphEventRole::SlideTailNoteOff
    } else {
        MorphEventRole::NoteOff
    };
    Ok(MorphEventId { source_step, role })
}
