use crate::launcher::child_args::{build_control_args, LauncherMidiChoice};

#[test]
fn child_args_include_scratch_and_web_port_for_env_default_midi() {
    let args = build_control_args("G2-P3B", &LauncherMidiChoice::EnvDefault, 4040);

    assert_eq!(
        args,
        vec!["control", "--scratch-pattern", "G2-P3B", "--port", "4040",]
    );
}

#[test]
fn child_args_include_exact_midi_pair_and_strict_flag() {
    let args = build_control_args(
        "G1-P1A",
        &LauncherMidiChoice::exact_pair("TD-3 MIDI In", "TD-3 MIDI Out"),
        3030,
    );

    assert_eq!(
        args,
        vec![
            "control",
            "--scratch-pattern",
            "G1-P1A",
            "--port",
            "3030",
            "--midi-in",
            "TD-3 MIDI In",
            "--midi-out",
            "TD-3 MIDI Out",
            "--strict-device-name",
        ]
    );
}

#[test]
fn child_args_keep_midi_names_with_spaces_as_single_values() {
    let args = build_control_args(
        "G4-P8B",
        &LauncherMidiChoice::exact_pair("USB TD-3 MO Input 1", "USB TD-3 MO Output 1"),
        5050,
    );

    assert_eq!(
        args.iter()
            .filter(|arg| arg.as_str() == "USB TD-3 MO Input 1")
            .count(),
        1
    );
    assert_eq!(
        args.iter()
            .filter(|arg| arg.as_str() == "USB TD-3 MO Output 1")
            .count(),
        1
    );
}
