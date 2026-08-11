//! Device MIDI channel handling for host-originated channel-voice
//! messages.
//!
//! A TD-3 accepts Note On/Off only on the channel it is configured for.
//! Host audition and the keyboard note preview are the only paths that
//! address the device with channel-voice messages, so both must encode
//! the configured channel. LIVE playback drives the device sequencer
//! with SysEx and MIDI realtime, neither of which carries a channel, so
//! a device on a non-default channel plays in LIVE mode and stays silent
//! in NO-LIVE unless the channel is honored here.

use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};
use crate::triplet_morph::MorphAmount;
use crate::web::clock::{
    prepare_morph_schedule, prepare_schedule, prepare_schedule_with_gate, DEFAULT_AUDITION_CHANNEL,
};
use crate::web::midi_channel::channel_status;

use super::fixtures::straight_sixteen;

const CENTIBPM_120: u32 = 12_000;

/// The channel the TD-3-MO on the reference rig listens on. Any value
/// other than 1 exercises the encoding; 3 is used because it is the one
/// that reproduced the silent-audition report.
const NON_DEFAULT_CHANNEL: u8 = 3;

/// Channel-voice status bytes present in a schedule, deduplicated.
fn status_bytes(events: &[crate::web::clock::ScheduledMidi]) -> Vec<u8> {
    let mut seen: Vec<u8> = events
        .iter()
        .filter_map(|event| event.bytes.first().copied())
        .filter(|status| matches!(status & 0xF0, 0x80 | 0x90))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen
}

/// A pattern with an accent and a slide so the timeline emits every
/// channel-voice shape the builders produce, not just plain notes.
fn expressive_pattern() -> Pattern {
    let mut steps = [Step::default(); 16];
    steps[0] = Step::new(0, Transpose::Normal, Accent::On, Slide::Off, Time::Normal);
    steps[1] = Step::new(7, Transpose::Up, Accent::Off, Slide::On, Time::Normal);
    steps[2] = Step::new(3, Transpose::Normal, Accent::Off, Slide::Off, Time::Tie);
    steps[3] = Step::new(5, Transpose::Down, Accent::On, Slide::Off, Time::Rest);
    Pattern::new(false, 16, steps).expect("expressive 16-step pattern is valid")
}

// ── channel_status ──────────────────────────────────────────────────

#[test]
fn channel_status_encodes_the_channel_in_the_low_nibble() {
    // MIDI numbers channels 1-16 on the wire as 0-15.
    assert_eq!(channel_status(0x90, 1), 0x90);
    assert_eq!(channel_status(0x90, 3), 0x92);
    assert_eq!(channel_status(0x90, 16), 0x9F);
    assert_eq!(channel_status(0x80, 3), 0x82);
    assert_eq!(channel_status(0xB0, 16), 0xBF);
}

#[test]
fn channel_status_clamps_out_of_range_channels_instead_of_corrupting_status() {
    // 0 and 17+ must not carry into the status nibble. A Note Off that
    // became some other message type would leave the note sounding with
    // no way to stop it.
    assert_eq!(channel_status(0x80, 0), 0x80);
    assert_eq!(channel_status(0x80, 17), 0x8F);
    assert_eq!(channel_status(0x80, 255), 0x8F);
    for channel in 0..=255u8 {
        assert_eq!(
            channel_status(0x80, channel) & 0xF0,
            0x80,
            "channel {channel} corrupted the status nibble"
        );
    }
}

// ── prepare_schedule / prepare_schedule_with_gate ───────────────────

#[test]
fn schedule_encodes_the_requested_channel_on_every_event() {
    let schedule = prepare_schedule(&expressive_pattern(), CENTIBPM_120, NON_DEFAULT_CHANNEL)
        .expect("schedule builds");

    assert!(!schedule.events.is_empty(), "pattern produced no events");
    assert_eq!(status_bytes(&schedule.events), vec![0x82, 0x92]);
    assert_eq!(schedule.channel, NON_DEFAULT_CHANNEL);
}

#[test]
fn default_channel_still_emits_the_historical_status_bytes() {
    // Regression guard for the shipped default: a config that does not
    // set MIDI_DEVICE_CHANNEL must produce exactly the byte stream
    // earlier releases sent.
    let schedule = prepare_schedule(
        &expressive_pattern(),
        CENTIBPM_120,
        DEFAULT_AUDITION_CHANNEL,
    )
    .expect("schedule builds");

    assert_eq!(status_bytes(&schedule.events), vec![0x80, 0x90]);
    assert_eq!(schedule.channel, 1);
}

#[test]
fn channel_changes_only_the_status_nibble_not_timing_or_payload() {
    // The channel must not perturb offsets, note numbers, or velocities.
    // If it did, changing it would silently retune or reshuffle playback.
    let pattern = expressive_pattern();
    let default = prepare_schedule(&pattern, CENTIBPM_120, DEFAULT_AUDITION_CHANNEL)
        .expect("default schedule builds");
    let shifted =
        prepare_schedule(&pattern, CENTIBPM_120, NON_DEFAULT_CHANNEL).expect("schedule builds");

    assert_eq!(default.events.len(), shifted.events.len());
    assert_eq!(default.cycle_period_us, shifted.cycle_period_us);
    for (a, b) in default.events.iter().zip(shifted.events.iter()) {
        assert_eq!(a.offset_us, b.offset_us);
        assert_eq!(a.bytes.len(), b.bytes.len());
        assert_eq!(a.bytes[0] & 0xF0, b.bytes[0] & 0xF0, "message type changed");
        assert_eq!(b.bytes[0] & 0x0F, NON_DEFAULT_CHANNEL - 1);
        assert_eq!(a.bytes[1..], b.bytes[1..], "payload changed");
    }
}

#[test]
fn gated_schedule_encodes_the_requested_channel() {
    // A gate other than 50 takes the second builder branch, which uses a
    // different timeline entry point.
    let schedule =
        prepare_schedule_with_gate(&expressive_pattern(), CENTIBPM_120, 90, NON_DEFAULT_CHANNEL)
            .expect("gated schedule builds");

    assert_eq!(status_bytes(&schedule.events), vec![0x82, 0x92]);
    assert_eq!(schedule.channel, NON_DEFAULT_CHANNEL);
}

// ── prepare_morph_schedule ──────────────────────────────────────────

/// Amount 0, an intermediate amount, and amount 100 are built by three
/// different code paths (provenance rebuild, rational warp, and endpoint
/// projection). Each has its own MIDI construction and each must carry
/// the channel.
#[test]
fn every_morph_builder_path_encodes_the_requested_channel() {
    for amount in [0u32, 1, 50, 99, 100] {
        let (schedule, _plan) = prepare_morph_schedule(
            &straight_sixteen(),
            CENTIBPM_120,
            50,
            MorphAmount::new(amount).expect("amount in range"),
            NON_DEFAULT_CHANNEL,
        )
        .unwrap_or_else(|err| panic!("morph schedule at {amount} failed: {err}"));

        assert!(
            !schedule.events.is_empty(),
            "morph amount {amount} produced no events"
        );
        assert_eq!(
            status_bytes(&schedule.events),
            vec![0x82, 0x92],
            "morph amount {amount} emitted a wrong-channel status byte"
        );
        assert_eq!(schedule.channel, NON_DEFAULT_CHANNEL);
    }
}

#[test]
fn morph_default_channel_still_emits_the_historical_status_bytes() {
    for amount in [0u32, 50, 100] {
        let (schedule, _plan) = prepare_morph_schedule(
            &straight_sixteen(),
            CENTIBPM_120,
            50,
            MorphAmount::new(amount).expect("amount in range"),
            DEFAULT_AUDITION_CHANNEL,
        )
        .unwrap_or_else(|err| panic!("morph schedule at {amount} failed: {err}"));

        assert_eq!(
            status_bytes(&schedule.events),
            vec![0x80, 0x90],
            "morph amount {amount} changed the default-channel bytes"
        );
    }
}

// ── silencing ───────────────────────────────────────────────────────

#[test]
fn schedule_carries_the_channel_the_runner_silences_on() {
    // The runner reads `schedule.channel` to build its shutdown Note Off
    // and All Notes Off. If the schedule reported a channel its events
    // were not encoded on, shutdown would leave notes ringing.
    let schedule = prepare_schedule(&expressive_pattern(), CENTIBPM_120, NON_DEFAULT_CHANNEL)
        .expect("schedule builds");

    for event in &schedule.events {
        assert_eq!(
            event.bytes[0] & 0x0F,
            schedule.channel - 1,
            "event channel does not match the schedule channel used for silencing"
        );
    }
    assert_eq!(
        channel_status(0x80, schedule.channel),
        0x82,
        "shutdown Note Off would address the wrong channel"
    );
    assert_eq!(
        channel_status(0xB0, schedule.channel),
        0xB2,
        "All Notes Off would address the wrong channel"
    );
}
