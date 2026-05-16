#![allow(clippy::field_reassign_with_default)]

use crate::config::{
    ArtifactPaths, BankJob, Config, ControlRuntime, MidiRuntime, Mode, RenderProfile,
};
use crate::formats::mid::{
    build_timeline, encode_vlq, export, MidiExportOptions, MidiSlideMode, DEFAULT_PPQN,
};
use crate::formats::Format;
use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};
use std::time::Duration;

fn make_step(note: u8, transpose: Transpose, accent: Accent, slide: Slide, time: Time) -> Step {
    Step {
        note,
        transpose,
        accent,
        slide,
        time,
    }
}

fn simple_pattern() -> Pattern {
    let mut pattern = Pattern::default();
    pattern.active_steps = 4;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(2, Transpose::Normal, Accent::On, Slide::Off, Time::Normal);
    pattern.step[2] = make_step(4, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[3] = make_step(5, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern
}

fn full_pattern() -> Pattern {
    let mut pattern = Pattern::default();
    pattern.active_steps = 16;
    for i in 0..16 {
        pattern.step[i] = make_step(
            (i % 12) as u8,
            Transpose::Normal,
            Accent::Off,
            Slide::Off,
            Time::Normal,
        );
    }
    pattern
}

#[test]
fn vlq_encodes_boundaries() {
    assert_eq!(encode_vlq(0), vec![0x00]);
    assert_eq!(encode_vlq(127), vec![0x7F]);
    assert_eq!(encode_vlq(128), vec![0x81, 0x00]);
    assert_eq!(encode_vlq(16_383), vec![0xFF, 0x7F]);
    assert_eq!(encode_vlq(16_384), vec![0x81, 0x80, 0x00]);
}

#[test]
fn timeline_adds_meta_and_end_of_track() {
    let pattern = simple_pattern();
    let options = MidiExportOptions::default();
    let timeline = build_timeline(&pattern, "G1-P1A", &options).unwrap();

    assert!(timeline
        .iter()
        .any(|event| event.data.starts_with(&[0xFF, 0x03])));
    assert!(timeline
        .iter()
        .any(|event| event.data.starts_with(&[0xFF, 0x51, 0x03])));
    assert!(timeline
        .iter()
        .any(|event| event.data == vec![0xFF, 0x2F, 0x00]));
}

#[test]
fn timeline_td3_slide_overlaps_next_note_by_eighth_step() {
    // Legato slide: new note starts at the step boundary, old note closes
    // 1/8 of a step later to create the overlap that Ableton needs to
    // render a proper glide.
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::On, Time::Normal);
    pattern.step[1] = make_step(2, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);

    let options = MidiExportOptions {
        slide_mode: MidiSlideMode::Td3,
        ..MidiExportOptions::default()
    };
    let timeline = build_timeline(&pattern, "G1-P1A", &options).unwrap();
    let step_ticks = (DEFAULT_PPQN / 4) as u32;
    let overlap = step_ticks / 8;

    // New note on at the boundary
    let boundary_ons: Vec<u8> = timeline
        .iter()
        .filter(|event| event.tick == step_ticks && event.data[0] & 0xF0 == 0x90)
        .map(|event| event.data[1])
        .collect();
    assert_eq!(boundary_ons, vec![2 + 36]);

    // Old note off sits 1/8 of a step later
    let overlap_offs: Vec<u8> = timeline
        .iter()
        .filter(|event| event.tick == step_ticks + overlap && event.data[0] & 0xF0 == 0x80)
        .map(|event| event.data[1])
        .collect();
    assert_eq!(overlap_offs, vec![36]);
}

#[test]
fn timeline_no_slide_note_releases_at_half_step() {
    // A Normal step without slide gets a half-length gate: the note-off
    // lands at step_ticks/2, not at the next step boundary.
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);

    let timeline = build_timeline(&pattern, "G1-P1A", &MidiExportOptions::default()).unwrap();
    let step_ticks = (DEFAULT_PPQN / 4) as u32;
    let half_tick = step_ticks / 2;

    let note_offs: Vec<u32> = timeline
        .iter()
        .filter(|event| event.data.first().map(|b| b & 0xF0) == Some(0x80))
        .map(|event| event.tick)
        .collect();
    assert_eq!(note_offs, vec![half_tick]);
}

#[test]
fn timeline_slide_note_holds_full_step() {
    // A Normal(slide=On) with no next Normal gets a full-length gate: the
    // note-off lands at the end of the step, not at step_ticks/2.
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::On, Time::Normal);
    pattern.step[1] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);

    let timeline = build_timeline(&pattern, "G1-P1A", &MidiExportOptions::default()).unwrap();
    let step_ticks = (DEFAULT_PPQN / 4) as u32;

    let note_offs: Vec<u32> = timeline
        .iter()
        .filter(|event| event.data.first().map(|b| b & 0xF0) == Some(0x80))
        .map(|event| event.tick)
        .collect();
    assert_eq!(note_offs, vec![step_ticks]);
}

#[test]
fn timeline_tie_extends_no_slide_note_to_final_step_half() {
    // Tie absorbs step 1 into the group. slide=Off → release at the midpoint
    // of the final step (step_ticks + step_ticks/2), not at the pattern end.
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(0, Transpose::Normal, Accent::On, Slide::Off, Time::Tie);

    let timeline = build_timeline(&pattern, "G1-P1A", &MidiExportOptions::default()).unwrap();
    let step_ticks = (DEFAULT_PPQN / 4) as u32;
    let release_tick = step_ticks + step_ticks / 2;

    assert!(!timeline
        .iter()
        .any(|event| event.tick == step_ticks && event.data[0] & 0xF0 == 0x80));
    assert!(timeline
        .iter()
        .any(|event| event.tick == release_tick && event.data[0] & 0xF0 == 0x80));
}

#[test]
fn timeline_tie_rest_holds_gate_through_no_slide_half_release() {
    // TieRest does not extend the group (only Tie does). slide=Off on step 0
    // still produces a half-step release at step_ticks/2 and no new note-on
    // at the TieRest's boundary.
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::TieRest);

    let timeline = build_timeline(&pattern, "G1-P1A", &MidiExportOptions::default()).unwrap();
    let step_ticks = (DEFAULT_PPQN / 4) as u32;
    let half_tick = step_ticks / 2;

    assert!(timeline
        .iter()
        .any(|event| event.tick == half_tick && event.data[0] & 0xF0 == 0x80));
    assert!(!timeline
        .iter()
        .any(|event| event.tick == step_ticks && event.data[0] & 0xF0 == 0x90));
}

#[test]
fn timeline_pitch_mapping_matches_td3_encoded_octaves() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 3;
    pattern.step[0] = make_step(0, Transpose::Down, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[2] = make_step(0, Transpose::Up, Accent::Off, Slide::Off, Time::Normal);

    let options = MidiExportOptions {
        octave_offset: 0,
        ..MidiExportOptions::default()
    };
    let timeline = build_timeline(&pattern, "G1-P1A", &options).unwrap();
    let note_on_notes: Vec<u8> = timeline
        .iter()
        .filter(|event| event.data.first().map(|b| b & 0xF0) == Some(0x90))
        .map(|event| event.data[1])
        .collect();

    assert_eq!(note_on_notes, vec![24, 36, 48]);
}

#[test]
fn timeline_exports_normal_g_sharp_at_daw_octave() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(8, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);

    let options = MidiExportOptions {
        octave_offset: 0,
        ..MidiExportOptions::default()
    };
    let timeline = build_timeline(&pattern, "G1-P1A", &options).unwrap();
    let note_on_notes: Vec<u8> = timeline
        .iter()
        .filter(|event| event.data.first().map(|b| b & 0xF0) == Some(0x90))
        .map(|event| event.data[1])
        .collect();

    assert_eq!(note_on_notes, vec![44]);
}

#[test]
fn export_starts_with_valid_smf_header() {
    let pattern = simple_pattern();
    let bytes = export(&pattern, "G1-P1A", &MidiExportOptions::default()).unwrap();

    assert_eq!(&bytes[0..4], b"MThd");
    assert_eq!(&bytes[4..8], &[0x00, 0x00, 0x00, 0x06]);
    assert_eq!(&bytes[8..10], &[0x00, 0x00]);
    assert_eq!(&bytes[10..12], &[0x00, 0x01]);
    assert_eq!(&bytes[12..14], &DEFAULT_PPQN.to_be_bytes());
    assert_eq!(&bytes[14..18], b"MTrk");
}

#[test]
fn export_sets_track_length_and_end_of_track() {
    let pattern = simple_pattern();
    let bytes = export(&pattern, "G1-P1A", &MidiExportOptions::default()).unwrap();
    let track_len = u32::from_be_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]) as usize;
    let track_data = &bytes[22..];

    assert_eq!(track_len, track_data.len());
    assert!(track_data.ends_with(&[0xFF, 0x2F, 0x00]));
}

#[test]
fn timeline_repeats_pattern_for_multiple_loops() {
    let pattern = simple_pattern();
    let options = MidiExportOptions {
        loop_count: 3,
        ..MidiExportOptions::default()
    };
    let timeline = build_timeline(&pattern, "G1-P1A", &options).unwrap();
    let note_on_ticks: Vec<u32> = timeline
        .iter()
        .filter(|event| event.data.first().map(|b| b & 0xF0) == Some(0x90))
        .map(|event| event.tick)
        .collect();

    assert_eq!(
        note_on_ticks,
        vec![0, 120, 240, 360, 480, 600, 720, 840, 960, 1080, 1200, 1320]
    );
    assert!(timeline
        .iter()
        .any(|event| event.tick == 1440 && event.data == vec![0xFF, 0x2F, 0x00]));
}

#[test]
fn export_options_reject_zero_loops() {
    let options = MidiExportOptions {
        loop_count: 0,
        ..MidiExportOptions::default()
    };
    assert!(options.validate().is_err());
}

#[test]
fn export_options_reject_generic_slide_mode() {
    let options = MidiExportOptions {
        slide_mode: MidiSlideMode::Generic,
        ..MidiExportOptions::default()
    };
    assert!(options.validate().is_err());
}

#[test]
fn timeline_rejects_non_divisible_triplet_ppqn() {
    let mut pattern = Pattern::default();
    pattern.triplet = true;
    let options = MidiExportOptions {
        ppqn: 100,
        ..MidiExportOptions::default()
    };
    assert!(build_timeline(&pattern, "G1-P1A", &options).is_err());
}

#[test]
fn export_rejects_out_of_range_midi_note_after_octave_offset() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(12, Transpose::Up, Accent::Off, Slide::Off, Time::Normal);

    let options = MidiExportOptions {
        octave_offset: 100,
        ..MidiExportOptions::default()
    };
    assert!(export(&pattern, "G1-P1A", &options).is_err());
}

/// Parse the MTrk data back into (tick, status_nibble) pairs so we can
/// verify the on-disk bytes carry the new timing rules.
fn parse_track_events(smf: &[u8]) -> Vec<(u32, u8)> {
    assert_eq!(&smf[0..4], b"MThd");
    assert_eq!(&smf[14..18], b"MTrk");
    let track_len = u32::from_be_bytes([smf[18], smf[19], smf[20], smf[21]]) as usize;
    let track = &smf[22..22 + track_len];

    let mut events = Vec::new();
    let mut i = 0;
    let mut tick = 0u32;
    while i < track.len() {
        let mut delta = 0u32;
        loop {
            let b = track[i];
            i += 1;
            delta = (delta << 7) | ((b & 0x7F) as u32);
            if b & 0x80 == 0 {
                break;
            }
        }
        tick += delta;
        let status = track[i];
        if status == 0xFF {
            let _meta = track[i + 1];
            let mut j = i + 2;
            let mut mlen = 0u32;
            loop {
                let b = track[j];
                j += 1;
                mlen = (mlen << 7) | ((b & 0x7F) as u32);
                if b & 0x80 == 0 {
                    break;
                }
            }
            i = j + mlen as usize;
            continue;
        }
        let status_nibble = status & 0xF0;
        events.push((tick, status_nibble));
        i += 3;
    }
    events
}

#[test]
fn export_bytes_contain_half_step_release_for_no_slide() {
    // End-to-end: serialize, parse the MTrk back, confirm note-off sits at
    // step_ticks/2 relative to its note-on.
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);

    let options = MidiExportOptions::default();
    let smf = export(&pattern, "G1-P1A", &options).unwrap();
    let events = parse_track_events(&smf);
    let step_ticks = (DEFAULT_PPQN / 4) as u32;

    let note_on = events.iter().find(|(_, s)| *s == 0x90).unwrap().0;
    let note_off = events.iter().find(|(_, s)| *s == 0x80).unwrap().0;
    assert_eq!(note_on, 0);
    assert_eq!(note_off, step_ticks / 2);
}

#[test]
fn export_bytes_contain_eighth_step_overlap_for_slide_to_different_pitch() {
    // End-to-end: slide=On + next Normal different pitch → note-on of new
    // note at boundary tick, note-off of old note at boundary + step/8.
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::On, Time::Normal);
    pattern.step[1] = make_step(2, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);

    let options = MidiExportOptions::default();
    let smf = export(&pattern, "G1-P1A", &options).unwrap();
    let events = parse_track_events(&smf);
    let step_ticks = (DEFAULT_PPQN / 4) as u32;

    let note_ons: Vec<u32> = events
        .iter()
        .filter(|(_, s)| *s == 0x90)
        .map(|(t, _)| *t)
        .collect();
    let note_offs: Vec<u32> = events
        .iter()
        .filter(|(_, s)| *s == 0x80)
        .map(|(t, _)| *t)
        .collect();
    assert_eq!(note_ons, vec![0, step_ticks]);
    assert_eq!(
        note_offs,
        vec![step_ticks + step_ticks / 8, step_ticks + step_ticks / 2]
    );
}

#[test]
fn bars_resolve_to_loop_count_for_full_pattern() {
    let pattern = full_pattern();
    let config = Config {
        mode: Mode::Export,
        midi: MidiRuntime {
            input_port_name: "TD-3".to_string(),
            output_port_name: "TD-3".to_string(),
            request_timeout: Duration::from_secs(5),
            strict_name_match: false,
            retry_count: 0,
        },
        target: None,
        files: ArtifactPaths::default(),
        render: RenderProfile {
            requested_formats: vec![Format::Mid],
            bpm: 120,
            ppqn: DEFAULT_PPQN,
            midi_channel: 1,
            octave_offset: 12,
            accent_velocity: 110,
            normal_velocity: 78,
            slide_mode: MidiSlideMode::Td3,
            loop_count: 1,
            bars: Some(4),
        },
        bank: BankJob::default(),
        control: ControlRuntime {
            bind_address: String::new(),
            listen_port: 3030,
            scratch_slot: None,
            backup_dir: None,
        },
    };

    let options = config.midi_export_options_for_pattern(&pattern).unwrap();
    assert_eq!(options.loop_count, 1);
}

#[test]
fn bars_override_loop_count() {
    let pattern = full_pattern();
    let config = Config {
        mode: Mode::Export,
        midi: MidiRuntime {
            input_port_name: "TD-3".to_string(),
            output_port_name: "TD-3".to_string(),
            request_timeout: Duration::from_secs(5),
            strict_name_match: false,
            retry_count: 0,
        },
        target: None,
        files: ArtifactPaths::default(),
        render: RenderProfile {
            requested_formats: vec![Format::Mid],
            bpm: 120,
            ppqn: DEFAULT_PPQN,
            midi_channel: 1,
            octave_offset: 12,
            accent_velocity: 110,
            normal_velocity: 78,
            slide_mode: MidiSlideMode::Td3,
            loop_count: 999,
            bars: Some(128),
        },
        bank: BankJob::default(),
        control: ControlRuntime {
            bind_address: String::new(),
            listen_port: 3030,
            scratch_slot: None,
            backup_dir: None,
        },
    };

    let options = config.midi_export_options_for_pattern(&pattern).unwrap();
    assert_eq!(options.loop_count, 32);
}
