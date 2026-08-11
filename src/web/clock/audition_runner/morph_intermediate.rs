//! Rational-warp audition builder for morph amounts 1 through 99.
//!
//! Source attacks keep warped boundary offsets. Each event offset is
//! computed from its exact rational global bar position and centi-BPM in
//! one truncating division, so no per-cell rounding can accumulate
//! drift. The cycle spans the source's own beats. The losing attack
//! keeps sounding, and its gate is bounded by its warped terminal cell,
//! which converges on the forward collision destination.
//!
//! Collision retirement: a losing attack is dropped at
//! [`COLLISION_RETIREMENT_AMOUNT_PERCENT`], or earlier if its onset
//! comes within [`COLLISION_RETIREMENT_FLOOR_US`] of the next attack.
//!
//! The amount threshold is a listening result. Past roughly the halfway
//! point the loser reads as a stumble against the triplet pulse the
//! survivors have established rather than as part of the phrase, so it
//! leaves at the halfway mark where the ear is already expecting the
//! grid to change.
//!
//! The separation floor still applies below that point, because at fast
//! tempos the two retriggers fuse before the amount ever reaches the
//! threshold. Below that separation a monophonic TD-3 cannot resolve two
//! retriggers, so the pair reads as a flam or a click rather than two
//! notes, and the losing note's own gate has shrunk below the length its
//! amplifier envelope needs.

use crate::error::Td3Error;
use crate::formats::mid::{
    note_off_event, note_on_event, ORDER_NOTE_OFF, ORDER_NOTE_ON, ORDER_SLIDE_NOTE_OFF,
    ORDER_SLIDE_NOTE_ON, TD3_MIDI_BASE_PITCH,
};
use crate::triplet_morph::{
    MorphAmount, MorphEventId, MorphEventRole, MorphPlanError, Rat, SourcePhrase, TripletMorphPlan,
};

use super::schedule::{AuditionSchedule, ScheduledMidi, ACCENT_VELOCITY, NORMAL_VELOCITY};

/// Connected-slide release overlap in beats: the old pitch is released
/// this long after the new attack, matching the legacy timeline's
/// one-eighth-step overlap (a step is 1/4 beat).
const SLIDE_OVERLAP_NUM: i128 = 1;
const SLIDE_OVERLAP_DEN: i128 = 32;

/// Minimum separation, in microseconds, between a losing attack and the
/// next attack. Below this the two retriggers fuse on the device, so the
/// losing attack is retired early instead of being emitted as a click.
pub(crate) const COLLISION_RETIREMENT_FLOOR_US: u64 = 20_000;

/// Morph amount at which a losing attack leaves regardless of how much
/// separation it still has. Below this it keeps sounding.
pub(crate) const COLLISION_RETIREMENT_AMOUNT_PERCENT: u32 = 80;

/// Fraction of the remaining silence removed at the peak of the gate
/// compensation curve, as a rational.
pub(crate) const GATE_COMPENSATION_PEAK_NUM: i128 = 80;
pub(crate) const GATE_COMPENSATION_PEAK_DEN: i128 = 100;

/// Morph amount where the gate compensation peaks. It ramps up to this
/// point and back down to nothing at the endpoint.
pub(crate) const GATE_COMPENSATION_PEAK_PERCENT: i128 = 70;

/// Gate fraction the intermediate builder releases at, with the sweep's
/// gate compensation folded in.
///
/// A gate is a fraction of its cell, so the scheduled duty cycle is
/// scale free: widening cells alone would keep it constant. The device
/// does not behave that way. Its amplifier envelope rings for roughly a
/// fixed time past every Note Off, so a bar with fewer, wider cells
/// collects that bonus fewer times and each individual silence grows in
/// absolute terms. Measured on a straight sixteen at 120 BPM with the
/// gate at 50: silences of 42 ms between sixteen attacks, 53 ms between
/// the twelve that remain once the losers retire.
///
/// The compensation removes a share of the remaining silence, so it
/// scales with the gate the user set instead of displacing it: a short
/// staccato gate stays short and a legato gate is left alone. It is zero
/// at both ends of the sweep and peaks at
/// [`GATE_COMPENSATION_PEAK_PERCENT`], which sits just under the
/// retirement point: the surviving cells have widened but the losers
/// have not left yet, so that is where the phrase is at its thinnest.
/// The two sides of the triangle are different widths.
///
/// The user's gate setting is unchanged. This is the audible gate only.
fn compensated_gate(gate_percent: u32, amount: MorphAmount) -> Option<Rat> {
    let gate = Rat::new(i128::from(gate_percent.clamp(1, 100)), 100)?;
    let value = i128::from(amount.value());
    let ramp = if value <= GATE_COMPENSATION_PEAK_PERCENT {
        Rat::new(value, GATE_COMPENSATION_PEAK_PERCENT)?
    } else {
        Rat::new(100 - value, 100 - GATE_COMPENSATION_PEAK_PERCENT)?
    };
    let shrink = Rat::new(GATE_COMPENSATION_PEAK_NUM, GATE_COMPENSATION_PEAK_DEN)?.mul(ramp);
    let one = Rat::int(1);
    let silence = one.sub(gate);
    Some(one.sub(silence.mul(one.sub(shrink))))
}

struct PendingEvent {
    offset_us: u64,
    order: u8,
    source_step: u8,
    bytes: Vec<u8>,
    id: MorphEventId,
}

pub(super) fn build_intermediate_schedule(
    phrase: &SourcePhrase,
    plan: &TripletMorphPlan,
    centibpm: u32,
    gate_percent: u32,
    amount: MorphAmount,
    channel: u8,
) -> Result<AuditionSchedule, Td3Error> {
    let cycle_period_us = pos_to_us(Rat::int(plan.beat_count() as i128), centibpm)?;
    let gate = compensated_gate(gate_percent, amount).ok_or(MorphPlanError::TimingOverflow)?;
    let slide_overlap =
        Rat::new(SLIDE_OVERLAP_NUM, SLIDE_OVERLAP_DEN).ok_or(MorphPlanError::TimingOverflow)?;

    let retired = collision_retirements(phrase, plan, centibpm, amount, cycle_period_us)?;
    let losers = loser_steps(plan);
    let absorbed = absorbed_onsets(phrase, plan, centibpm, amount, &retired)?;
    let adopted_onset = |step: usize| {
        absorbed
            .iter()
            .find(|(winner, _)| *winner == step)
            .map(|(_, onset)| *onset)
    };
    let next_onsets = next_surviving_onsets(phrase, plan, centibpm, amount, &retired, &absorbed)?;

    let mut events: Vec<PendingEvent> = Vec::new();
    // Sounding pitch and owning attack step while a connected slide
    // chain is open. The owner stays the chain's first attack so Note
    // Off identity matches the endpoint adapter's attribution.
    let mut sounding: Option<(u8, u8)> = None;

    for (index, attack) in phrase.attacks.iter().enumerate() {
        if retired[index] {
            // A retired attack is never slide connected, so no chain
            // state is open across it and nothing needs closing here.
            continue;
        }
        let midi_pitch = TD3_MIDI_BASE_PITCH + attack.pitch;
        let pitch = u8::try_from(midi_pitch).map_err(|_| {
            Td3Error::FormatError(format!("midi note out of range: {}", midi_pitch))
        })?;
        if pitch > 127 {
            return Err(Td3Error::FormatError(format!(
                "midi note out of range: {}",
                midi_pitch
            )));
        }
        let velocity = if attack.accent {
            ACCENT_VELOCITY
        } else {
            NORMAL_VELOCITY
        };
        let source_step = attack.step as u8;
        let onset_pos = warp(plan, attack.step, amount)?;
        // A winner whose loser has been retired starts in the hole the
        // loser left. The slide overlap still measures from the attack's
        // own warped position, so `onset_pos` stays the glide reference.
        let onset_us = match adopted_onset(attack.step) {
            Some(adopted) => adopted,
            None => pos_to_us(onset_pos, centibpm)?,
        };
        if onset_us >= cycle_period_us {
            return Err(MorphPlanError::EventOutsideCycle.into());
        }

        let (sound_pitch, owner, own_onset_us) = match sounding.take() {
            Some((prev_pitch, prev_owner)) if prev_pitch == pitch => {
                // Equal-pitch connected slide: the note continues with
                // no new events; ownership stays with the chain start.
                (prev_pitch, prev_owner, None)
            }
            Some((prev_pitch, prev_owner)) => {
                events.push(PendingEvent {
                    offset_us: onset_us,
                    order: ORDER_SLIDE_NOTE_ON,
                    source_step,
                    bytes: note_on_event(channel, pitch, velocity),
                    id: MorphEventId {
                        source_step,
                        role: MorphEventRole::NoteOn,
                    },
                });
                let tail_us =
                    pos_to_us(onset_pos.add(slide_overlap), centibpm)?.min(cycle_period_us);
                events.push(PendingEvent {
                    offset_us: tail_us,
                    order: ORDER_SLIDE_NOTE_OFF,
                    source_step: prev_owner,
                    bytes: note_off_event(channel, prev_pitch),
                    id: MorphEventId {
                        source_step: prev_owner,
                        role: MorphEventRole::SlideTailNoteOff,
                    },
                });
                (pitch, source_step, Some(onset_us))
            }
            None => {
                events.push(PendingEvent {
                    offset_us: onset_us,
                    order: ORDER_NOTE_ON,
                    source_step,
                    bytes: note_on_event(channel, pitch, velocity),
                    id: MorphEventId {
                        source_step,
                        role: MorphEventRole::NoteOn,
                    },
                });
                (pitch, source_step, Some(onset_us))
            }
        };

        let connects_onward = phrase
            .slide_edges
            .iter()
            .any(|(from, _)| *from == attack.step);
        if connects_onward {
            sounding = Some((sound_pitch, owner));
            continue;
        }

        // Release inside the warped terminal cell of the attack's tie
        // chain. Slide-on unconnected notes hold the full warped cell;
        // ordinary notes release at the gate fraction through it. The
        // cell end equals the forward collision destination for a losing
        // attack, so the gate approaches zero without crossing it.
        //
        // A losing attack is the exception: its release is drawn from
        // the gate point towards the next surviving attack in
        // proportion to how far it is through its journey to
        // retirement, so the note fills the gap it is vacating rather
        // than becoming an ever shorter blip followed by an ever longer
        // silence. ORDER_NOTE_OFF sorts ahead of ORDER_NOTE_ON at the
        // same offset, so a fully held release lands immediately before
        // the attack it hands over to.
        let cell_start = warp(plan, attack.group_end, amount)?;
        let cell_end = warp(plan, attack.group_end + 1, amount)?;
        let holds_to_next_attack = !attack.slide && losers.contains(&attack.step);
        let off_pos = if attack.slide {
            cell_end
        } else {
            cell_start.add(gate.mul(cell_end.sub(cell_start)))
        };
        let release_us = pos_to_us(off_pos, centibpm)?;
        let mut off_us = if holds_to_next_attack {
            held_release_us(release_us, next_onsets[index], amount)
        } else {
            release_us
        }
        .min(cycle_period_us);
        if let Some(onset) = own_onset_us {
            if off_us <= onset {
                off_us = (onset + 1).min(cycle_period_us);
            }
        }
        events.push(PendingEvent {
            offset_us: off_us,
            order: ORDER_NOTE_OFF,
            source_step: owner,
            bytes: note_off_event(channel, sound_pitch),
            id: MorphEventId {
                source_step: owner,
                role: MorphEventRole::NoteOff,
            },
        });
        sounding = None;
    }

    // The final attack can never hold an open connection because a slide
    // edge requires a following Normal cell, but release defensively so
    // no Note On could leak past the cycle boundary.
    if let Some((pitch, owner)) = sounding.take() {
        events.push(PendingEvent {
            offset_us: cycle_period_us,
            order: ORDER_NOTE_OFF,
            source_step: owner,
            bytes: note_off_event(channel, pitch),
            id: MorphEventId {
                source_step: owner,
                role: MorphEventRole::NoteOff,
            },
        });
    }

    events.sort_by_key(|event| (event.offset_us, event.order, event.source_step));

    Ok(AuditionSchedule {
        events: events
            .into_iter()
            .map(|event| ScheduledMidi {
                offset_us: event.offset_us,
                bytes: event.bytes,
                event_id: Some(event.id),
            })
            .collect(),
        cycle_period_us,
        channel,
    })
}

/// Release of a losing attack: its own gate release drawn towards the
/// next surviving attack by the morph amount.
///
/// A loser's cell collapses as the amount rises, so a plain gate
/// fraction of it turns the note into an ever shorter blip followed by
/// an ever longer silence. Holding to the next attack removes that, but
/// taking the whole hold from amount 1 doubles every losing note the
/// instant the knob leaves zero: measured at 120 BPM with the gate at
/// 50, the silence in a cycle fell from 572 ms to 327 ms between amount
/// 0 and amount 1, and three of the twelve gaps closed outright.
///
/// The hold is therefore proportional. It is nothing at amount 1 and
/// complete at [`COLLISION_RETIREMENT_AMOUNT_PERCENT`], which is where
/// the loser is retired and the winner adopts its onset, so the two
/// behaviours meet at the same release and the handover is continuous.
///
/// The separation floor can retire a loser earlier than the threshold at
/// fast tempos. The hold is short of complete there, but the floor is 20
/// ms of separation, so what remains is bounded by that.
fn held_release_us(gate_release_us: u64, next_onset_us: u64, amount: MorphAmount) -> u64 {
    let Some(span) = next_onset_us.checked_sub(gate_release_us) else {
        return gate_release_us;
    };
    let threshold = u64::from(COLLISION_RETIREMENT_AMOUNT_PERCENT);
    let progress = u64::from(amount.value()).min(threshold);
    gate_release_us.saturating_add(span.saturating_mul(progress) / threshold)
}

/// Source steps the plan marks as the losing attack of their beat.
fn loser_steps(plan: &TripletMorphPlan) -> Vec<usize> {
    plan.beats.iter().map(|beat| beat.loser).collect()
}

/// The surviving step a loser collides into: the selected step of the
/// same beat that shares its target offset.
fn collision_winner(plan: &TripletMorphPlan, loser: usize) -> Option<usize> {
    let beat = plan.beats.iter().find(|beat| beat.loser == loser)?;
    let loser_target = plan.assignments.get(loser)?.target_offset;
    beat.selected
        .iter()
        .copied()
        .find(|&step| plan.assignments.get(step).map(|a| a.target_offset) == Some(loser_target))
}

/// Onset a surviving attack adopts because the loser that collided into
/// it has been retired, keyed by the surviving source step.
///
/// The retired loser leaves a hole where its attack used to be, and the
/// winner is the note that hole belongs to once the two have merged. The
/// winner therefore starts where the loser started and keeps its own
/// release, so the attack grid is unchanged across the retirement point
/// and only the note count drops.
///
/// The adopted onset is the loser's own warped position, which converges
/// on the shared target as the amount rises. The winner slides forward
/// out of the hole and its body shrinks until, at the endpoint, it sits
/// exactly on the triplet grid.
///
/// A slide-connected winner is left alone: its onset is bound to the
/// glide it belongs to.
fn absorbed_onsets(
    phrase: &SourcePhrase,
    plan: &TripletMorphPlan,
    centibpm: u32,
    amount: MorphAmount,
    retired: &[bool],
) -> Result<Vec<(usize, u64)>, Td3Error> {
    let mut adopted = Vec::new();
    for (index, attack) in phrase.attacks.iter().enumerate() {
        if !retired[index] {
            continue;
        }
        let Some(winner) = collision_winner(plan, attack.step) else {
            continue;
        };
        if phrase
            .slide_edges
            .iter()
            .any(|(from, to)| *from == winner || *to == winner)
        {
            continue;
        }
        let loser_onset = pos_to_us(warp(plan, attack.step, amount)?, centibpm)?;
        let winner_onset = pos_to_us(warp(plan, winner, amount)?, centibpm)?;
        // Only ever pull an attack earlier, into the hole ahead of it.
        if loser_onset < winner_onset {
            adopted.push((winner, loser_onset));
        }
    }
    Ok(adopted)
}

/// Warped onset of the next attack that is still emitted, per attack
/// index. A losing attack releases on this value so it sounds through
/// the gap it is vacating. The last attack has no successor inside the
/// cycle and holds to the cycle end.
fn next_surviving_onsets(
    phrase: &SourcePhrase,
    plan: &TripletMorphPlan,
    centibpm: u32,
    amount: MorphAmount,
    retired: &[bool],
    absorbed: &[(usize, u64)],
) -> Result<Vec<u64>, Td3Error> {
    let count = phrase.attacks.len();
    let cycle_period_us = pos_to_us(Rat::int(plan.beat_count() as i128), centibpm)?;
    let mut onsets: Vec<u64> = Vec::with_capacity(count);
    for attack in &phrase.attacks {
        // An absorbed winner is measured where it actually sounds, so a
        // loser that holds to it releases on the adopted onset.
        let adopted = absorbed
            .iter()
            .find(|(winner, _)| *winner == attack.step)
            .map(|(_, onset)| *onset);
        match adopted {
            Some(onset) => onsets.push(onset),
            None => onsets.push(pos_to_us(warp(plan, attack.step, amount)?, centibpm)?),
        }
    }
    let mut next = vec![cycle_period_us; count];
    let mut following = cycle_period_us;
    for index in (0..count).rev() {
        next[index] = following;
        if !retired[index] {
            following = onsets[index];
        }
    }
    Ok(next)
}

/// Decide which losing attacks are retired at this amount.
///
/// A losing attack is retired when its onset comes within
/// [`COLLISION_RETIREMENT_FLOOR_US`] of the next attack, measured
/// against the unretired onset sequence so the decision does not depend
/// on evaluation order. The last attack measures against the first
/// attack of the next loop.
///
/// Slide-connected attacks are never retired. A slide target glides
/// without retriggering the amplifier, so it produces no flam to begin
/// with, and dropping either end of a chain would break a continuous
/// glide rather than remove a redundant retrigger.
fn collision_retirements(
    phrase: &SourcePhrase,
    plan: &TripletMorphPlan,
    centibpm: u32,
    amount: MorphAmount,
    cycle_period_us: u64,
) -> Result<Vec<bool>, Td3Error> {
    let count = phrase.attacks.len();
    let mut retired = vec![false; count];
    if count == 0 {
        return Ok(retired);
    }

    let mut onsets: Vec<u64> = Vec::with_capacity(count);
    for attack in &phrase.attacks {
        onsets.push(pos_to_us(warp(plan, attack.step, amount)?, centibpm)?);
    }

    let losers: Vec<usize> = plan.beats.iter().map(|beat| beat.loser).collect();
    let slide_connected = |step: usize| {
        phrase
            .slide_edges
            .iter()
            .any(|(from, to)| *from == step || *to == step)
    };

    let wrap_onset = cycle_period_us.saturating_add(onsets[0]);
    for index in 0..count {
        let step = phrase.attacks[index].step;
        if !losers.contains(&step) || slide_connected(step) {
            continue;
        }
        if u32::from(amount.value()) >= COLLISION_RETIREMENT_AMOUNT_PERCENT {
            retired[index] = true;
            continue;
        }
        let next = onsets.get(index + 1).copied().unwrap_or(wrap_onset);
        if next.saturating_sub(onsets[index]) < COLLISION_RETIREMENT_FLOOR_US {
            retired[index] = true;
        }
    }
    Ok(retired)
}

fn warp(plan: &TripletMorphPlan, step: usize, amount: MorphAmount) -> Result<Rat, Td3Error> {
    plan.warp_boundary(step, amount)
        .ok_or_else(|| Td3Error::Midi(format!("morph warp: source step {} out of range", step)))
}

/// Convert an exact rational bar position, in beats, to a microsecond
/// offset at `centibpm`. One beat is 6_000_000_000 / centibpm
/// microseconds; the single truncating division here matches the
/// legacy tick conversion's rounding style.
pub(super) fn pos_to_us(pos: Rat, centibpm: u32) -> Result<u64, Td3Error> {
    if pos.is_negative() {
        return Err(MorphPlanError::EventOutsideCycle.into());
    }
    let numerator = pos
        .num()
        .checked_mul(6_000_000_000)
        .ok_or(MorphPlanError::TimingOverflow)?;
    let denominator = pos
        .den()
        .checked_mul(i128::from(centibpm.max(1)))
        .ok_or(MorphPlanError::TimingOverflow)?;
    if denominator <= 0 {
        return Err(MorphPlanError::TimingOverflow.into());
    }
    u64::try_from(numerator / denominator).map_err(|_| MorphPlanError::TimingOverflow.into())
}
