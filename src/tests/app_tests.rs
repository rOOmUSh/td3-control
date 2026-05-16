use std::time::Duration;

use crate::config::{
    ArtifactPaths, BankJob, Config, ControlRuntime, MidiRuntime, Mode, RenderProfile,
};
use crate::formats::mid::MidiSlideMode;

fn base_config(mode: Mode) -> Config {
    Config {
        mode,
        midi: MidiRuntime {
            input_port_name: "TD-3".to_string(),
            output_port_name: "TD-3".to_string(),
            request_timeout: Duration::from_millis(1),
            strict_name_match: true,
            retry_count: 0,
        },
        target: None,
        files: ArtifactPaths::default(),
        render: RenderProfile {
            requested_formats: Vec::new(),
            bpm: 120,
            ppqn: 96,
            midi_channel: 1,
            octave_offset: 0,
            accent_velocity: 110,
            normal_velocity: 80,
            slide_mode: MidiSlideMode::Td3,
            loop_count: 1,
            bars: None,
        },
        bank: BankJob::default(),
        control: ControlRuntime {
            bind_address: "127.0.0.1".to_string(),
            listen_port: 3030,
            scratch_slot: None,
            backup_dir: None,
        },
    }
}

#[test]
fn export_mode_missing_resolved_target_returns_error_before_midi_open() {
    let config = base_config(Mode::Export);
    let err = crate::app::run(config).unwrap_err().to_string();
    assert!(
        err.contains("export mode requires resolved pattern target"),
        "expected missing target error, got: {}",
        err
    );
}
