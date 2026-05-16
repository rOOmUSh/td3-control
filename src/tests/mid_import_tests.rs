//! Tests for .mid → Pattern import, including round-trip contracts against
//! the exporter in `formats::mid`.

#![allow(clippy::field_reassign_with_default)]

use crate::error::Td3Error;
use crate::formats::mid::{export, MidiExportOptions, DEFAULT_PPQN};
use crate::formats::mid_import::{
    import, LowestPitchResolver, MidiImportOptions, PolyphonyCandidate, PolyphonyResolver,
    RejectPolyphonyResolver,
};
use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_step(note: u8, transpose: Transpose, accent: Accent, slide: Slide, time: Time) -> Step {
    Step {
        note,
        transpose,
        accent,
        slide,
        time,
    }
}

fn assert_steps_equal(a: &Pattern, b: &Pattern, upto: usize) {
    assert_eq!(a.triplet, b.triplet, "triplet mismatch");
    assert_eq!(a.active_steps, b.active_steps, "active_steps mismatch");
    for i in 0..upto {
        assert_eq!(a.step[i], b.step[i], "step {} mismatch", i);
    }
}

fn roundtrip(pattern: &Pattern) -> Pattern {
    let bytes = export(pattern, "G1-P1A", &MidiExportOptions::default()).unwrap();
    let mut resolver = RejectPolyphonyResolver;
    import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap()
}

// ---------------------------------------------------------------------------
// Basic parsing
// ---------------------------------------------------------------------------

#[test]
fn rejects_missing_mthd_header() {
    let mut resolver = RejectPolyphonyResolver;
    let err = import(
        b"not a midi file",
        &MidiImportOptions::default(),
        &mut resolver,
    )
    .unwrap_err();
    assert!(format!("{}", err).contains("MThd"));
}

#[test]
fn rejects_empty_file() {
    let mut resolver = RejectPolyphonyResolver;
    let err = import(&[], &MidiImportOptions::default(), &mut resolver).unwrap_err();
    assert!(matches!(err, Td3Error::FormatError(_)));
}

#[test]
fn rejects_smpte_timing() {
    // MThd with division = 0xE728 (negative → SMPTE)
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MThd");
    bytes.extend_from_slice(&6u32.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes()); // format 0
    bytes.extend_from_slice(&1u16.to_be_bytes()); // ntrks
    bytes.extend_from_slice(&0xE728u16.to_be_bytes()); // SMPTE
    bytes.extend_from_slice(b"MTrk");
    bytes.extend_from_slice(&0u32.to_be_bytes());

    let mut resolver = RejectPolyphonyResolver;
    let err = import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap_err();
    assert!(format!("{}", err).contains("SMPTE"));
}

#[test]
fn accepts_file_with_no_note_ons_as_empty_pattern() {
    // Valid MThd + empty MTrk (just end-of-track meta at delta 0). Real
    // TD-3 banks routinely contain all-rest patterns used as recording
    // markers; the .mid export of such a pattern carries no note events,
    // and the importer must still produce a valid Pattern.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MThd");
    bytes.extend_from_slice(&6u32.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&(DEFAULT_PPQN).to_be_bytes());

    let track: Vec<u8> = vec![
        0x00, 0xFF, 0x2F, 0x00, // delta 0, end of track
    ];
    bytes.extend_from_slice(b"MTrk");
    bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&track);

    let mut resolver = RejectPolyphonyResolver;
    let pattern = import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap();
    assert!(!pattern.triplet);
    assert_eq!(pattern.active_steps, 1, "EOT at delta 0 collapses to 1 step");
    for (i, s) in pattern.step.iter().enumerate() {
        assert_eq!(s.time, Time::TieRest, "step {} should be TieRest", i + 1);
    }
}

// ---------------------------------------------------------------------------
// Round-trip contracts
// ---------------------------------------------------------------------------

fn empty_pattern(triplet: bool, active_steps: u8, time: Time) -> Pattern {
    let mut pattern = Pattern::default();
    pattern.triplet = triplet;
    pattern.active_steps = active_steps;
    for s in pattern.step.iter_mut() {
        s.time = time;
    }
    pattern
}

#[test]
fn roundtrip_empty_tierest_pattern_16_step_straight() {
    // Hardware stores empty/marker patterns with both tie and rest bits set.
    let pattern = empty_pattern(false, 16, Time::TieRest);
    let imported = roundtrip(&pattern);
    assert_steps_equal(&pattern, &imported, 16);
}

#[test]
fn roundtrip_empty_tierest_pattern_16_step_triplet() {
    let pattern = empty_pattern(true, 16, Time::TieRest);
    let imported = roundtrip(&pattern);
    assert_steps_equal(&pattern, &imported, 16);
}

#[test]
fn roundtrip_empty_tierest_pattern_4_step_straight() {
    let pattern = empty_pattern(false, 4, Time::TieRest);
    let imported = roundtrip(&pattern);
    assert!(!imported.triplet);
    assert_eq!(imported.active_steps, 4);
    for i in 0..4 {
        assert_eq!(imported.step[i].time, Time::TieRest);
    }
}

#[test]
fn empty_rest_pattern_canonicalizes_to_tierest() {
    // The exporter emits zero MIDI events for Tie, Rest, and TieRest steps
    // alike, so an all-Rest source pattern is indistinguishable from an
    // all-TieRest one in the .mid bytes. The importer chooses TieRest to
    // match the device's empty-pattern convention. Audibly identical.
    let pattern = empty_pattern(false, 16, Time::Rest);
    let imported = roundtrip(&pattern);
    assert_eq!(imported.active_steps, 16);
    for i in 0..16 {
        assert_eq!(imported.step[i].time, Time::TieRest);
    }
}

#[test]
fn roundtrip_single_normal_step_preserved() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);

    let imported = roundtrip(&pattern);
    assert_steps_equal(&pattern, &imported, 1);
}

#[test]
fn roundtrip_accent_preserved() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::On, Slide::Off, Time::Normal);

    let imported = roundtrip(&pattern);
    assert_eq!(imported.step[0].accent, Accent::On);
}

#[test]
fn roundtrip_slide_preserved() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::On, Time::Normal);
    pattern.step[1] = make_step(2, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);

    let imported = roundtrip(&pattern);
    assert_eq!(imported.step[0].slide, Slide::On);
    assert_eq!(imported.step[1].slide, Slide::Off);
}

#[test]
fn roundtrip_tie_preserved_as_tie() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Tie);

    let imported = roundtrip(&pattern);
    assert_eq!(imported.step[0].time, Time::Normal);
    assert_eq!(imported.step[1].time, Time::Tie);
}

#[test]
fn roundtrip_rest_preserved_as_rest() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 2;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);

    let imported = roundtrip(&pattern);
    assert_eq!(imported.step[0].time, Time::Normal);
    assert_eq!(imported.step[1].time, Time::Rest);
}

#[test]
fn roundtrip_transpose_down_preserved() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(0, Transpose::Down, Accent::Off, Slide::Off, Time::Normal);

    let imported = roundtrip(&pattern);
    assert_eq!(imported.step[0].transpose, Transpose::Down);
    assert_eq!(imported.step[0].note, 0);
}

#[test]
fn roundtrip_transpose_up_preserved() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(5, Transpose::Up, Accent::Off, Slide::Off, Time::Normal);

    let imported = roundtrip(&pattern);
    assert_eq!(imported.step[0].transpose, Transpose::Up);
    assert_eq!(imported.step[0].note, 5);
}

#[test]
fn roundtrip_triplet_grid_detected() {
    let mut pattern = Pattern::default();
    pattern.triplet = true;
    pattern.active_steps = 3;
    pattern.step[0] = make_step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[1] = make_step(2, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);
    pattern.step[2] = make_step(4, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal);

    let imported = roundtrip(&pattern);
    assert!(imported.triplet, "triplet grid should be auto-detected");
    assert_steps_equal(&pattern, &imported, 3);
}

#[test]
fn roundtrip_full_16_step_pattern() {
    let mut pattern = Pattern::default();
    pattern.active_steps = 16;
    for i in 0..16u8 {
        let note = i % 12;
        let transpose = match i / 6 {
            0 => Transpose::Down,
            1 => Transpose::Normal,
            _ => Transpose::Up,
        };
        let accent = if i % 3 == 0 { Accent::On } else { Accent::Off };
        let slide = if i % 4 == 0 { Slide::On } else { Slide::Off };
        pattern.step[i as usize] = make_step(note, transpose, accent, slide, Time::Normal);
    }

    let imported = roundtrip(&pattern);
    assert_steps_equal(&pattern, &imported, 16);
}

// ---------------------------------------------------------------------------
// Behaviour tests (not round-trip)
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_preserves_c_sharp_on_up_octave_boundary() {
    // Regression for a real-device round-trip bug: step with note=12 (C^)
    // on transpose=Up encodes to TD-3 pitch 48 → MIDI 60. The importer used
    // to clamp 48 as out-of-range and drop to 36 (C/Up), losing an octave.
    let mut pattern = Pattern::default();
    pattern.active_steps = 1;
    pattern.step[0] = make_step(12, Transpose::Up, Accent::On, Slide::On, Time::Normal);

    let imported = roundtrip(&pattern);
    assert_eq!(imported.step[0].note, 12);
    assert_eq!(imported.step[0].transpose, Transpose::Up);
    assert_eq!(imported.step[0].accent, Accent::On);
    assert_eq!(imported.step[0].slide, Slide::On);
}

#[test]
fn pitch_out_of_range_is_clamped_to_nearest_octave() {
    // Very high pitch (2 octaves above TD-3 Up-C#) should clamp down by
    // whole octaves to the same (note, transpose). Using C# instead of C
    // avoids the note=12 (C^) boundary, which is a separate case.
    let bytes = build_mid_with_note(midi_pitch_for(1, Transpose::Up) + 24, 90, 0, 100);
    let mut resolver = RejectPolyphonyResolver;
    let pattern = import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap();
    assert_eq!(pattern.step[0].note, 1);
    assert_eq!(pattern.step[0].transpose, Transpose::Up);
}

#[test]
fn pitch_below_range_is_clamped_up() {
    // TD-3 Down-C is MIDI 24. Send MIDI 0.
    let bytes = build_mid_with_note(0, 90, 0, 100);
    let mut resolver = RejectPolyphonyResolver;
    let pattern = import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap();
    // Should have clamped up by 2 octaves → TD-3 Down-C.
    assert_eq!(pattern.step[0].note, 0);
    assert_eq!(pattern.step[0].transpose, Transpose::Down);
}

#[test]
fn polyphony_calls_resolver_with_candidates_sorted_by_pitch() {
    // Two note-ons at tick 0, pitches 36 (C) and 40 (E).
    let mut bytes = mthd_header(DEFAULT_PPQN);
    let track: Vec<u8> = vec![
        0x00,
        0x90,
        36,
        80,
        0x00,
        0x90,
        40,
        80,
        (DEFAULT_PPQN / 4) as u8,
        0x80,
        36,
        0,
        0x00,
        0x80,
        40,
        0,
        0x00,
        0xFF,
        0x2F,
        0x00,
    ];
    append_mtrk(&mut bytes, &track);

    struct Capture {
        called: bool,
        picked: usize,
    }
    impl PolyphonyResolver for Capture {
        fn choose(
            &mut self,
            step_index: usize,
            candidates: &[PolyphonyCandidate],
        ) -> Result<usize, Td3Error> {
            assert_eq!(step_index, 0);
            assert_eq!(candidates.len(), 2);
            assert!(candidates[0].midi_pitch <= candidates[1].midi_pitch);
            self.called = true;
            Ok(self.picked)
        }
    }

    let mut r = Capture {
        called: false,
        picked: 1,
    }; // pick the higher pitch (E)
    let pattern = import(&bytes, &MidiImportOptions::default(), &mut r).unwrap();
    assert!(r.called);
    // MIDI 40 - offset 12 = TD-3 pitch 28 → transpose=Normal, note=4 (E).
    assert_eq!(pattern.step[0].note, 4);
    assert_eq!(pattern.step[0].transpose, Transpose::Normal);
}

#[test]
fn reject_polyphony_resolver_errors_on_chord() {
    let mut bytes = mthd_header(DEFAULT_PPQN);
    let track: Vec<u8> = vec![
        0x00,
        0x90,
        36,
        80,
        0x00,
        0x90,
        40,
        80,
        (DEFAULT_PPQN / 4) as u8,
        0x80,
        36,
        0,
        0x00,
        0x80,
        40,
        0,
        0x00,
        0xFF,
        0x2F,
        0x00,
    ];
    append_mtrk(&mut bytes, &track);

    let mut resolver = RejectPolyphonyResolver;
    let err = import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap_err();
    assert!(format!("{}", err).contains("polyphony"));
}

#[test]
fn lowest_pitch_resolver_picks_lowest() {
    let mut bytes = mthd_header(DEFAULT_PPQN);
    let track: Vec<u8> = vec![
        0x00,
        0x90,
        36,
        80,
        0x00,
        0x90,
        40,
        80,
        (DEFAULT_PPQN / 4) as u8,
        0x80,
        36,
        0,
        0x00,
        0x80,
        40,
        0,
        0x00,
        0xFF,
        0x2F,
        0x00,
    ];
    append_mtrk(&mut bytes, &track);

    let mut resolver = LowestPitchResolver;
    let pattern = import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap();
    // MIDI 36 → TD-3 24 → transpose=Normal, note=0 (C).
    assert_eq!(pattern.step[0].note, 0);
    assert_eq!(pattern.step[0].transpose, Transpose::Normal);
}

#[test]
fn running_status_is_handled() {
    // Two note-ons on separate steps, all reusing a single 0x90 status byte.
    // Note-offs are emitted as "0x90 <pitch> 0" via running status - a common
    // pattern in real DAW output and in SMF spec. Tests that the parser
    // correctly threads running status across multiple events.
    let mut bytes = mthd_header(DEFAULT_PPQN);
    let step_ticks = (DEFAULT_PPQN / 4) as u8;
    let track: Vec<u8> = vec![
        0x00, 0x90, 36, 80, // NoteOn pitch=36
        step_ticks, 36, 0, // running 0x90 → NoteOff (vel=0)
        0x00, 38, 80, // running 0x90 → NoteOn pitch=38
        step_ticks, 38, 0, // running 0x90 → NoteOff
        0x00, 0xFF, 0x2F, 0x00, // EOT
    ];
    append_mtrk(&mut bytes, &track);

    let mut resolver = RejectPolyphonyResolver;
    let pattern = import(&bytes, &MidiImportOptions::default(), &mut resolver).unwrap();
    assert_eq!(pattern.active_steps, 2);
    assert_eq!(pattern.step[0].time, Time::Normal);
    assert_eq!(pattern.step[1].time, Time::Normal);
    assert_eq!(pattern.step[0].note, 0);
    assert_eq!(pattern.step[1].note, 2);
}

#[test]
fn chord_progression_uses_scripted_user_choices() {
    // Matches the example: step 1 has C/E/G, step 2 has D/F/A.
    // A scripted resolver picks the 3rd candidate (G) for step 1 and the 1st
    // candidate (D) for step 2 - proving that the interactive menu drives the
    // per-step decision, never the heuristic.
    let step_ticks = (DEFAULT_PPQN / 4) as u8;
    let mut bytes = mthd_header(DEFAULT_PPQN);
    let track: Vec<u8> = vec![
        // STEP 1: C (36), E (40), G (43) all at tick 0
        0x00, 0x90, 36, 80, 0x00, 0x90, 40, 80, 0x00, 0x90, 43, 80, // Release at end of step 1
        step_ticks, 0x80, 36, 0, 0x00, 0x80, 40, 0, 0x00, 0x80, 43, 0,
        // STEP 2: D (38), F (41), A (45) at step_ticks
        0x00, 0x90, 38, 80, 0x00, 0x90, 41, 80, 0x00, 0x90, 45, 80, // Release at end of step 2
        step_ticks, 0x80, 38, 0, 0x00, 0x80, 41, 0, 0x00, 0x80, 45, 0, 0x00, 0xFF, 0x2F, 0x00,
    ];
    append_mtrk(&mut bytes, &track);

    struct Scripted {
        picks: Vec<usize>,
        step: usize,
    }
    impl PolyphonyResolver for Scripted {
        fn choose(
            &mut self,
            step_index: usize,
            candidates: &[PolyphonyCandidate],
        ) -> Result<usize, Td3Error> {
            assert_eq!(step_index, self.step);
            assert_eq!(candidates.len(), 3);
            let pick = self.picks[self.step];
            self.step += 1;
            Ok(pick)
        }
    }

    // User types "3" at step 1 → G, then "1" at step 2 → D.
    let mut r = Scripted {
        picks: vec![2, 0],
        step: 0,
    };
    let pattern = import(&bytes, &MidiImportOptions::default(), &mut r).unwrap();

    // G at default octave → TD-3 Normal-G (note=7, transpose=Normal).
    assert_eq!(pattern.step[0].note, 7);
    assert_eq!(pattern.step[0].transpose, Transpose::Normal);
    // D at default octave → TD-3 Normal-D (note=2, transpose=Normal).
    assert_eq!(pattern.step[1].note, 2);
    assert_eq!(pattern.step[1].transpose, Transpose::Normal);
}

#[test]
fn velocity_threshold_controls_accent() {
    // One step with velocity just above threshold → Accent On.
    let bytes = build_mid_with_note(36, 120, 0, 100);
    let opts = MidiImportOptions::default();
    let mut resolver = RejectPolyphonyResolver;
    let pattern = import(&bytes, &opts, &mut resolver).unwrap();
    assert_eq!(pattern.step[0].accent, Accent::On);

    // And one just below the threshold → Off.
    let bytes2 = build_mid_with_note(36, 50, 0, 100);
    let pattern2 = import(&bytes2, &opts, &mut RejectPolyphonyResolver).unwrap();
    assert_eq!(pattern2.step[0].accent, Accent::Off);
}

// ---------------------------------------------------------------------------
// MIDI file construction helpers for tests
// ---------------------------------------------------------------------------

fn midi_pitch_for(note: u8, transpose: Transpose) -> u8 {
    let t = match transpose {
        Transpose::Down => 0,
        Transpose::Normal => 1,
        Transpose::Up => 2,
    };
    24 + note + 12 * t
}

fn mthd_header(ppqn: u16) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"MThd");
    v.extend_from_slice(&6u32.to_be_bytes());
    v.extend_from_slice(&0u16.to_be_bytes());
    v.extend_from_slice(&1u16.to_be_bytes());
    v.extend_from_slice(&ppqn.to_be_bytes());
    v
}

fn append_mtrk(out: &mut Vec<u8>, track: &[u8]) {
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(track.len() as u32).to_be_bytes());
    out.extend_from_slice(track);
}

/// Build a minimal SMF with a single note occupying one step.
fn build_mid_with_note(pitch: u8, velocity: u8, start_tick: u8, gate: u8) -> Vec<u8> {
    let mut bytes = mthd_header(DEFAULT_PPQN);
    let track: Vec<u8> = vec![
        start_tick, 0x90, pitch, velocity, gate, 0x80, pitch, 0, 0x00, 0xFF, 0x2F, 0x00,
    ];
    append_mtrk(&mut bytes, &track);
    bytes
}
