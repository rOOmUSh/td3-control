//! Tests for the launcher module's pure logic - selection state machine,
//! port-name matching, and help-text generation. The egui app itself is
//! exercised manually since eframe windows can't be rendered headlessly
//! in CI without a display.

use crate::launcher::child_args::LauncherMidiChoice;
use crate::launcher::choice::{store_outcome, LauncherChoice, LauncherOutcome};
use crate::launcher::help_text;
use crate::launcher::midi_probe;
use crate::launcher::selection::SelectionState;

// =========================================================================
// SelectionState
// =========================================================================

#[test]
fn default_selection_is_g1_p1_a() {
    let s = SelectionState::default();
    assert_eq!(s.label(), "G1-P1A");
    assert_eq!(s.group, 1);
    assert_eq!(s.pattern, 1);
    assert!(!s.side_b);
}

#[test]
fn label_format_uses_dash_separator_and_letter_side() {
    assert_eq!(SelectionState::new(1, 1, false).label(), "G1-P1A");
    assert_eq!(SelectionState::new(2, 4, true).label(), "G2-P4B");
    assert_eq!(SelectionState::new(4, 8, false).label(), "G4-P8A");
}

#[test]
fn from_label_parses_dashed_form() {
    let s = SelectionState::from_label("G2-P4B").unwrap();
    assert_eq!(s.group, 2);
    assert_eq!(s.pattern, 4);
    assert!(s.side_b);
}

#[test]
fn from_label_parses_compact_form() {
    let s = SelectionState::from_label("G3P5A").unwrap();
    assert_eq!(s.group, 3);
    assert_eq!(s.pattern, 5);
    assert!(!s.side_b);
}

#[test]
fn from_label_is_case_insensitive() {
    let s = SelectionState::from_label("g4-p8b").unwrap();
    assert_eq!(s.group, 4);
    assert_eq!(s.pattern, 8);
    assert!(s.side_b);
}

#[test]
fn from_label_rejects_out_of_range_group() {
    assert!(SelectionState::from_label("G0-P1A").is_none());
    assert!(SelectionState::from_label("G5-P1A").is_none());
}

#[test]
fn from_label_rejects_out_of_range_pattern() {
    assert!(SelectionState::from_label("G1-P0A").is_none());
    assert!(SelectionState::from_label("G1-P9A").is_none());
}

#[test]
fn from_label_rejects_invalid_side() {
    assert!(SelectionState::from_label("G1-P1C").is_none());
    assert!(SelectionState::from_label("G1-P1Z").is_none());
}

#[test]
fn from_label_rejects_garbage() {
    assert!(SelectionState::from_label("").is_none());
    assert!(SelectionState::from_label("hello").is_none());
    assert!(SelectionState::from_label("X1-P1A").is_none());
}

#[test]
fn label_round_trip_preserves_state() {
    for g in 1..=4 {
        for p in 1..=8 {
            for side_b in [false, true] {
                let s = SelectionState::new(g, p, side_b);
                let parsed = SelectionState::from_label(&s.label()).unwrap();
                assert_eq!(s, parsed);
            }
        }
    }
}

// =========================================================================
// midi_probe::matches
// =========================================================================

#[test]
fn matches_substring_case_insensitive() {
    assert!(midi_probe::matches("Microsoft TD-3 Out", "td-3", false));
    assert!(midi_probe::matches("MIDI TD-3", "TD-3", false));
    assert!(!midi_probe::matches("MIDI Other", "td-3", false));
}

#[test]
fn matches_strict_requires_exact() {
    assert!(midi_probe::matches("TD-3", "TD-3", true));
    assert!(!midi_probe::matches("Microsoft TD-3", "TD-3", true));
}

#[test]
fn empty_substring_never_matches() {
    assert!(!midi_probe::matches("anything", "", false));
    assert!(!midi_probe::matches("anything", "", true));
}

// =========================================================================
// help_text
// =========================================================================

#[test]
fn full_help_lists_every_subcommand() {
    let help = help_text::full_help();
    for sub in [
        "export",
        "import",
        "list-ports",
        "control",
        "convert",
        "extract-bank",
        "pack-bank",
        "import-bank",
    ] {
        assert!(
            help.contains(sub),
            "help text missing subcommand: {}\n{}",
            sub,
            help
        );
    }
}

#[test]
fn full_help_mentions_program_name() {
    assert!(help_text::full_help().contains("td3-control"));
}

// =========================================================================
// Launcher outcome
// =========================================================================

#[test]
fn store_outcome_records_start_choice() {
    let outcome = std::sync::Arc::new(std::sync::Mutex::new(LauncherOutcome::default()));
    let choice = LauncherChoice {
        scratch: "G2-P3B".to_string(),
        persist: true,
        midi: LauncherMidiChoice::EnvDefault,
        web_port: 3030,
    };

    assert!(store_outcome(&outcome, Some(choice)));

    let stored = outcome.lock().unwrap().0.clone().unwrap();
    assert_eq!(stored.scratch, "G2-P3B");
    assert!(stored.persist);
}

#[test]
fn store_outcome_records_cancel() {
    let outcome = std::sync::Arc::new(std::sync::Mutex::new(LauncherOutcome(Some(
        LauncherChoice {
            scratch: "G1-P1A".to_string(),
            persist: false,
            midi: LauncherMidiChoice::EnvDefault,
            web_port: 3030,
        },
    ))));

    assert!(store_outcome(&outcome, None));

    assert!(outcome.lock().unwrap().0.is_none());
}
