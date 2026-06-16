use crate::launcher::child_args::LauncherMidiChoice;
use crate::launcher::persist::{build_updates, LauncherPersistChoice};

#[test]
fn launcher_persist_updates_scratch_and_web_port() {
    let updates = build_updates(&LauncherPersistChoice {
        scratch: "G2-P3B".to_string(),
        web_port: 4040,
        midi: LauncherMidiChoice::EnvDefault,
    })
    .unwrap();

    assert_eq!(
        updates.get("UI_SCRATCH_PATTERN").map(String::as_str),
        Some("G2-P3B")
    );
    assert_eq!(updates.get("WEB_PORT").map(String::as_str), Some("4040"));
    assert!(!updates.contains_key("MIDI_PORT_SUBSTRING"));
}

#[test]
fn launcher_persist_writes_exact_single_name_midi_choice() {
    let updates = build_updates(&LauncherPersistChoice {
        scratch: "G1-P1A".to_string(),
        web_port: 3030,
        midi: LauncherMidiChoice::exact_pair("USB TD-3", "USB TD-3"),
    })
    .unwrap();

    assert_eq!(
        updates.get("MIDI_PORT_SUBSTRING").map(String::as_str),
        Some("USB TD-3")
    );
    assert_eq!(
        updates.get("MIDI_STRICT_NAME_MATCH").map(String::as_str),
        Some("1")
    );
}

#[test]
fn launcher_persist_does_not_write_midi_choice_when_names_differ() {
    let updates = build_updates(&LauncherPersistChoice {
        scratch: "G1-P1A".to_string(),
        web_port: 3030,
        midi: LauncherMidiChoice::exact_pair("USB TD-3 In", "USB TD-3 Out"),
    })
    .unwrap();

    assert!(!updates.contains_key("MIDI_PORT_SUBSTRING"));
    assert!(!updates.contains_key("MIDI_STRICT_NAME_MATCH"));
}

#[test]
fn launcher_persist_rejects_invalid_scratch_slot() {
    let result = build_updates(&LauncherPersistChoice {
        scratch: "G9-P1A".to_string(),
        web_port: 3030,
        midi: LauncherMidiChoice::EnvDefault,
    });

    assert!(result.is_err());
}
