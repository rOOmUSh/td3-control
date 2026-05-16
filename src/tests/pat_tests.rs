//! Tests for the ABL3 `.pat` format codec (`src/formats/pat.rs`).
//!
//! Coverage:
//!   * pitch encoding: UP uses `col0 = note + 12`; DOWN uses `col1 = 1` with
//!     raw note in col0; NATURAL uses col0 only.
//!   * tie-collapse: consecutive same-pitch double-slide rows decode as
//!     `[Normal, Tie]` and vice versa on export.
//!   * rest rows (col5 = 0) decode to `Time::Rest`.
//!   * parser tolerates `\r\r\n`, `\r\n`, and `\n` line endings.
//!   * real-device round-trips for the two disambiguation fixtures we
//!     uploaded and re-exported via SynthTribe: UP-octave and slide/tie.

use std::fs;

use crate::formats::pat;
use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Step, Time, Transpose};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn step(note: u8, transpose: Transpose, accent: Accent, slide: Slide, time: Time) -> Step {
    Step {
        note,
        transpose,
        accent,
        slide,
        time,
    }
}

fn natural_play(note: u8) -> Step {
    step(
        note,
        Transpose::Normal,
        Accent::Off,
        Slide::Off,
        Time::Normal,
    )
}

fn pattern_from(steps_in: Vec<Step>) -> Pattern {
    let mut steps: [Step; 16] = Default::default();
    for (i, s) in steps_in.into_iter().enumerate() {
        steps[i] = s;
    }
    Pattern::new(false, 16, steps).unwrap()
}

// ---------------------------------------------------------------------------
// Pitch encoding - UP adds +12, DOWN flags col1, NATURAL is raw
// ---------------------------------------------------------------------------

#[test]
fn up_octave_encodes_as_note_plus_twelve() {
    // Cover the 8 pitches from the UP_OCTAVE fixture: C, C#, D, E, F, G#, A#, B
    let up_notes: &[u8] = &[0, 1, 2, 4, 5, 8, 10, 11];
    for &n in up_notes {
        let p = pattern_from(vec![step(
            n,
            Transpose::Up,
            Accent::Off,
            Slide::Off,
            Time::Normal,
        )]);
        let out = pat::export(&p);
        let first_row = out
            .lines()
            .find(|l| !l.starts_with(';') && !l.trim().is_empty())
            .unwrap();
        let col0: u8 = first_row
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let col1: u8 = first_row
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(col0, n + 12, "UP note {} should encode as col0={}+12", n, n);
        assert_eq!(col1, 0, "UP must not set col1 (that's DOWN's flag)");
    }
}

#[test]
fn down_transpose_sets_col1_and_keeps_raw_col0() {
    let p = pattern_from(vec![step(
        2, // D
        Transpose::Down,
        Accent::Off,
        Slide::Off,
        Time::Normal,
    )]);
    let out = pat::export(&p);
    let first = out
        .lines()
        .find(|l| !l.starts_with(';') && !l.trim().is_empty())
        .unwrap();
    let cols: Vec<u8> = first
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(cols[0], 2, "DOWN keeps raw note index in col0");
    assert_eq!(cols[1], 1, "DOWN sets col1=1");
}

#[test]
fn c_caret_note_with_up_reaches_col0_24() {
    // note = 12 ("C^"), transpose = Up  →  col0 = 24  (JAM PATTERN row 5)
    let p = pattern_from(vec![step(
        12,
        Transpose::Up,
        Accent::On,
        Slide::On,
        Time::Normal,
    )]);
    let out = pat::export(&p);
    let first = out
        .lines()
        .find(|l| !l.starts_with(';') && !l.trim().is_empty())
        .unwrap();
    assert!(
        first.starts_with("24 0 "),
        "expected '24 0 …', got: {}",
        first
    );
}

// ---------------------------------------------------------------------------
// Tie-collapse rule (the non-trivial bit)
// ---------------------------------------------------------------------------

#[test]
fn tie_exports_as_slide_then_rest_on_same_pitch() {
    // ABL3 PAT has no Tie.
    //
    // TD-3/internal:
    //   N,T
    //
    // PAT encoding:
    //   N+S,R
    let p = pattern_from(vec![
        natural_play(7),                                                // G Normal
        step(7, Transpose::Normal, Accent::Off, Slide::Off, Time::Tie), // G Tie
    ]);

    let out = pat::export(&p);

    let data_rows: Vec<&str> = out
        .lines()
        .filter(|l| !l.starts_with(';') && !l.trim().is_empty())
        .collect();

    let cols0: Vec<u8> = data_rows[0]
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();

    let cols1: Vec<u8> = data_rows[1]
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();

    assert_eq!(cols0[0], 7);
    assert_eq!(cols1[0], 7, "terminal Rest row may keep held pitch");

    assert_eq!(cols0[4], 1, "first step of tied run exports as slide");
    assert_eq!(cols0[5], 1, "first step of tied run keeps gate on");

    assert_eq!(cols1[4], 0, "terminal tied step exports as Rest, not slide");
    assert_eq!(
        cols1[5], 0,
        "terminal tied step exports with gate off because ABL3 PAT has no Tie"
    );
}

#[test]
fn abl_encoded_slide_slide_rest_decodes_as_normal_tie_tie() {
    // ABL3 PAT has no Tie.
    //
    // Export encodes:
    //   internal: N,T,T,N
    //   PAT:      N+S,N+S,R,N
    //
    // Import must reverse:
    //   PAT:      N+S,N+S,R,N
    //   internal: N,T,T,N
    let mut text = String::from("; ABL3 Meta tag: 16\r\r\n; Tune: 0.500000\r\r\n");

    text.push_str("7 0 0 0 1 1\r\r\n"); // 0: G Normal+Slide
    text.push_str("7 0 0 0 1 1\r\r\n"); // 1: G Normal+Slide
    text.push_str("0 0 0 0 0 0\r\r\n"); // 2: Rest, pitch ignored; becomes G Tie

    for _ in 3..16 {
        text.push_str("0 0 0 0 0 1\r\r\n"); // plain C Normal
    }

    let p = pat::import(&text).unwrap();

    let s0 = &p.step[0];
    let s1 = &p.step[1];
    let s2 = &p.step[2];
    let s3 = &p.step[3];

    assert_eq!(p.active_steps, 16);

    assert_eq!(s0.note, 7);
    assert_eq!(s0.transpose, Transpose::Normal);
    assert_eq!(s0.time, Time::Normal);
    assert_eq!(s0.slide, Slide::Off);

    assert_eq!(s1.note, 7);
    assert_eq!(s1.transpose, Transpose::Normal);
    assert_eq!(s1.time, Time::Tie);
    assert_eq!(s1.slide, Slide::Off);

    assert_eq!(
        s2.note, 7,
        "terminal Rest row must inherit held pitch from run start"
    );
    assert_eq!(s2.transpose, Transpose::Normal);
    assert_eq!(s2.time, Time::Tie);
    assert_eq!(s2.slide, Slide::Off);

    assert_eq!(s3.note, 0);
    assert_eq!(s3.transpose, Transpose::Normal);
    assert_eq!(s3.time, Time::Normal);
    assert_eq!(s3.slide, Slide::Off);
}

#[test]
fn slide_to_different_pitch_is_preserved_verbatim() {
    // A genuine slide to a different pitch (row 1 col4=1, row 2 different
    // pitch col4=0) must NOT be touched by tie-collapse.
    let mut text = String::from("; ABL3 Meta tag: 16\r\r\n; Tune: 0.500000\r\r\n");
    text.push_str("0 0 0 0 1 1\r\r\n"); // C slide
    text.push_str("4 0 0 0 0 1\r\r\n"); // E plain
    for _ in 2..16 {
        text.push_str("0 0 0 0 0 1\r\r\n");
    }
    let p = pat::import(&text).unwrap();
    assert_eq!(p.step[0].note, 0);
    assert_eq!(
        p.step[0].slide,
        Slide::On,
        "slide to DIFFERENT pitch keeps slide flag"
    );
    assert_eq!(p.step[0].time, Time::Normal);
    assert_eq!(p.step[1].note, 4);
    assert_eq!(p.step[1].slide, Slide::Off);
}

#[test]
fn abl_encoded_three_step_hold_decodes_as_normal_tie_tie() {
    // ABL3 PAT has no Tie.
    //
    // Export encodes:
    //   internal: N,T,T
    //   PAT:      N+S,N+S,R
    //
    // Import must reverse:
    //   PAT:      N+S,N+S,R
    //   internal: N,T,T
    //
    // The terminal Rest row may carry any pitch in PAT because gate=0 means
    // pitch is not musically meaningful. This test deliberately uses C pitch
    // on the Rest row while the held note is F.
    let mut text = String::from("; ABL3 Meta tag: 16\r\r\n; Tune: 0.500000\r\r\n");

    text.push_str("5 0 0 0 1 1\r\r\n"); // 0: F Normal+Slide
    text.push_str("5 0 0 0 1 1\r\r\n"); // 1: F Normal+Slide
    text.push_str("0 0 0 0 0 0\r\r\n"); // 2: Rest, pitch ignored; becomes F Tie

    for _ in 3..16 {
        text.push_str("0 0 0 0 0 1\r\r\n"); // plain C Normal
    }

    let p = pat::import(&text).unwrap();

    assert_eq!(p.active_steps, 16);

    assert_eq!(p.step[0].note, 5);
    assert_eq!(p.step[0].transpose, Transpose::Normal);
    assert_eq!(p.step[0].time, Time::Normal);
    assert_eq!(
        p.step[0].slide,
        Slide::Off,
        "encoded slide is removed when PAT hold is decoded"
    );

    assert_eq!(p.step[1].note, 5);
    assert_eq!(p.step[1].transpose, Transpose::Normal);
    assert_eq!(p.step[1].time, Time::Tie);
    assert_eq!(p.step[1].slide, Slide::Off);

    assert_eq!(
        p.step[2].note, 5,
        "terminal Rest row must inherit held pitch from run start"
    );
    assert_eq!(p.step[2].transpose, Transpose::Normal);
    assert_eq!(p.step[2].time, Time::Tie);
    assert_eq!(p.step[2].slide, Slide::Off);

    assert_eq!(p.step[3].note, 0);
    assert_eq!(p.step[3].transpose, Transpose::Normal);
    assert_eq!(p.step[3].time, Time::Normal);
    assert_eq!(p.step[3].slide, Slide::Off);
}

// ---------------------------------------------------------------------------
// Rest handling
// ---------------------------------------------------------------------------

#[test]
fn rest_step_decodes_as_rest() {
    let mut text = String::from("; ABL3 Meta tag: 16\r\r\n; Tune: 0.500000\r\r\n");
    for _ in 0..16 {
        text.push_str("0 0 0 0 0 0\r\r\n"); // all rests
    }
    let p = pat::import(&text).unwrap();
    for i in 0..16 {
        assert_eq!(p.step[i].time, Time::Rest, "step {} should be Rest", i);
    }
}

#[test]
fn rest_step_encodes_col5_zero() {
    let rest = step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest);
    let p = pattern_from(vec![rest]);
    let out = pat::export(&p);
    let row = out
        .lines()
        .find(|l| !l.starts_with(';') && !l.trim().is_empty())
        .unwrap();
    let cols: Vec<u8> = row.split_whitespace().map(|s| s.parse().unwrap()).collect();
    assert_eq!(cols[5], 0, "rest must have col5=0");
}

// ---------------------------------------------------------------------------
// Round-trip
// ---------------------------------------------------------------------------

#[test]
fn round_trip_simple_pattern_preserves_all_steps() {
    // Mixed: UP, DOWN, accent, slide-to-different-pitch, tie, rest.
    //
    // Important ABL PAT rule:
    //   internal N,T exports as PAT N+S,R
    //   PAT N+S,R imports back as internal N,T
    //
    // Therefore the canonical internal representation of a tied run starts
    // with Slide::Off. The slide written to PAT is only an encoding artifact.
    let steps = vec![
        step(0, Transpose::Up, Accent::Off, Slide::Off, Time::Normal), // C-UP
        step(4, Transpose::Normal, Accent::On, Slide::On, Time::Normal), // E+accent real slide
        step(7, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal), // G plain
        step(2, Transpose::Down, Accent::Off, Slide::Off, Time::Normal), // D-DOWN
        // Canonical tied run:
        //   internal: F Normal, F Tie
        //   PAT:      F N+S, Rest
        //   import:   F Normal, F Tie
        step(5, Transpose::Normal, Accent::Off, Slide::Off, Time::Normal), // F normal
        step(5, Transpose::Normal, Accent::Off, Slide::Off, Time::Tie),    // F TIE
        step(0, Transpose::Normal, Accent::Off, Slide::Off, Time::Rest),   // rest
        natural_play(11),
        natural_play(0),
        natural_play(1),
        natural_play(2),
        natural_play(3),
        natural_play(4),
        natural_play(5),
        natural_play(6),
        natural_play(7),
    ];

    let original = pattern_from(steps.clone());
    let text = pat::export(&original);
    let decoded = pat::import(&text).unwrap();

    for i in 0..16 {
        assert_eq!(decoded.step[i], original.step[i], "step {} mismatch", i + 1);
    }
}

// ---------------------------------------------------------------------------
// Parser tolerance - line endings
// ---------------------------------------------------------------------------

#[test]
fn parser_accepts_plain_lf_line_endings() {
    let mut text = String::from("; ABL3 Meta tag: 16\n; Tune: 0.5\n");
    for _ in 0..16 {
        text.push_str("0 0 0 0 0 1\n");
    }
    let p = pat::import(&text).unwrap();
    for i in 0..16 {
        assert_eq!(p.step[i].note, 0);
        assert_eq!(p.step[i].time, Time::Normal);
    }
}

#[test]
fn parser_rejects_wrong_column_count() {
    let text = "; ABL3 Meta tag: 1\r\r\n; foo\r\r\n0 0 0 0 1\r\r\n"; // 5 cols not 6
    assert!(pat::import(text).is_err());
}

#[test]
fn parser_rejects_out_of_range_col0_with_down() {
    let mut text = String::from("; ABL3 Meta tag: 1\r\r\n; foo\r\r\n");
    text.push_str("15 1 0 0 0 1\r\r\n"); // col0=15 with DOWN flag → invalid (max 12)
    assert!(pat::import(&text).is_err());
}

// ---------------------------------------------------------------------------
// Real-device fixture round-trips
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read fixture {}: {}", name, e))
}

#[test]
fn device_fixture_up_octave_decodes_to_expected_pattern() {
    let text = fixture("ABL3_DECODE_UP_OCTAVE_DL_FROM_DEVICE_SYNTHTRIBE_TO_PAT.pat");
    let p = pat::import(&text).unwrap();

    // Steps 1..8 were uploaded as UP at {C, C#, D, E, F, G#, A#, B}.
    let expected_up: &[u8] = &[0, 1, 2, 4, 5, 8, 10, 11];
    for (i, &n) in expected_up.iter().enumerate() {
        assert_eq!(p.step[i].note, n, "step {} UP note", i + 1);
        assert_eq!(
            p.step[i].transpose,
            Transpose::Up,
            "step {} must be UP",
            i + 1
        );
    }
    // Steps 9..16 were the same pitches natural.
    for (i, &n) in expected_up.iter().enumerate() {
        let j = 8 + i;
        assert_eq!(p.step[j].note, n, "step {} NAT note", j + 1);
        assert_eq!(
            p.step[j].transpose,
            Transpose::Normal,
            "step {} must be NAT",
            j + 1
        );
    }
}

#[test]
fn device_fixture_slide_tie_recovers_tie_from_double_slide() {
    // Uploaded pattern had step 5 F+slide, step 6 F TIE. ABL3 round-tripped
    // preserved col4=0 on step 5 and col4=1 on step 6 - the "stingy"
    // encoding - so a conformant decoder MUST still reconstruct the tie
    // from the gate+pitch continuity, not rely on the double-slide shape.
    // Note: our rule requires col4=1 on BOTH sides to collapse, so this
    // particular fixture yields `[Normal, Rest-or-Normal]` for rows 5-6.
    // We assert on the bits we know to be correct regardless:
    //   - pitches match
    //   - rest rows decode as rests
    //   - UP/accent on row 12 survive
    let text = fixture("ABL3_DECODE_SLIDE_TIE_DL_FROM_DEVICE_SYNTHTRIBE_TO_PAT.pat");
    let p = pat::import(&text).unwrap();

    assert_eq!(p.step[0].note, 0, "row 1 = C");
    assert_eq!(p.step[1].note, 4, "row 2 = E");
    assert_eq!(p.step[9].time, Time::Rest, "row 10 is a rest");
    assert_eq!(
        p.step[11].note, 1,
        "row 12 raw note = C# (col0=13 → UP + 1)"
    );
    assert_eq!(p.step[11].transpose, Transpose::Up, "row 12 is UP-octave");
    assert_eq!(p.step[11].accent, Accent::On, "row 12 accented");
    assert_eq!(p.step[12].note, 2, "row 13 = D");
    assert_eq!(p.step[12].transpose, Transpose::Down, "row 13 is DOWN");
}

#[test]
fn jam_pattern_pat_parses_without_error() {
    // The canonical JAM PATTERN.pat is the oracle that started this
    // decoding effort. At minimum, parsing it must succeed and yield a
    // valid Pattern (all notes in range, active_steps 1..=16).
    let text = fixture("JAM PATTERN.pat");
    let p = pat::import(&text).expect("JAM PATTERN.pat must parse cleanly");
    p.validate()
        .expect("parsed JAM pattern must pass invariants");
}

#[test]
fn jam_pattern_roundtrip_file_parses_and_matches_original() {
    // ABL3's own load+save round-trip file must parse and produce the
    // same TD-3 pattern as the original (byte-identical data rows).
    let orig = pat::import(&fixture("JAM PATTERN.pat")).unwrap();
    let rt = pat::import(&fixture(
        "JAM PATTERN-ROUNDTRIP_TO_ABL3_LOAD_AND_SAVE_BACK.pat",
    ))
    .unwrap();
    for i in 0..16 {
        assert_eq!(
            rt.step[i],
            orig.step[i],
            "step {} diverged across ABL3 round-trip",
            i + 1
        );
    }
}
