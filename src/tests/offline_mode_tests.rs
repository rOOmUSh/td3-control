//! Regression tests for offline-mode startup (no TD-3 connected).
//!
//! When the configured MIDI port substring doesn't match any port,
//! `app::try_pre_ui_backup` must return `Ok(None)` so the web server can
//! still come up. `run_control_backup_session` must return the underlying
//! `PortNotFound` error so the wrapper has something specific to match.

use std::path::PathBuf;
use std::time::Duration;

use crate::app;
use crate::config::{
    ArtifactPaths, BankJob, Config, ControlRuntime, MidiRuntime, Mode, RenderProfile,
};
use crate::error::Td3Error;
use crate::formats::mid::{MidiSlideMode, DEFAULT_PPQN};

const NEVER_MATCHES_ANY_PORT: &str = "td3-offline-mode-test-nonexistent-port-zzz-9f1c";

fn temp_backup_dir(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "td3-offline-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn control_config_with_unmatched_port(backup_dir: PathBuf) -> Config {
    Config {
        mode: Mode::Control,
        midi: MidiRuntime {
            input_port_name: NEVER_MATCHES_ANY_PORT.to_string(),
            output_port_name: NEVER_MATCHES_ANY_PORT.to_string(),
            request_timeout: Duration::from_secs(1),
            strict_name_match: true,
            retry_count: 0,
        },
        target: None,
        files: ArtifactPaths::default(),
        render: RenderProfile {
            requested_formats: Vec::new(),
            bpm: 120,
            ppqn: DEFAULT_PPQN,
            midi_channel: 1,
            octave_offset: 12,
            accent_velocity: 110,
            normal_velocity: 78,
            slide_mode: MidiSlideMode::Td3,
            loop_count: 1,
            bars: None,
        },
        bank: BankJob::default(),
        control: ControlRuntime {
            bind_address: "127.0.0.1".to_string(),
            listen_port: 3030,
            scratch_slot: None,
            backup_dir: Some(backup_dir.to_string_lossy().into_owned()),
        },
    }
}

#[test]
fn try_pre_ui_backup_returns_none_when_no_device() {
    let dir = temp_backup_dir("none");
    let config = control_config_with_unmatched_port(dir);
    let outcome = app::try_pre_ui_backup(&config).expect("offline path must not fail");
    assert!(
        outcome.is_none(),
        "expected None (offline) when port substring matches nothing"
    );
}

#[test]
fn run_control_backup_session_yields_port_not_found_when_no_device() {
    let dir = temp_backup_dir("portnotfound");
    let config = control_config_with_unmatched_port(dir);
    match app::run_control_backup_session(&config) {
        Err(Td3Error::PortNotFound { .. }) => (),
        Err(other) => panic!("expected PortNotFound, got {:?}", other),
        Ok(_) => {
            panic!("expected PortNotFound, got Ok (impossible without a device on this port name)")
        }
    }
}

#[test]
fn try_pre_ui_backup_propagates_non_port_errors() {
    // Bad backup_dir surfaces BankBackupFailed *before* MIDI is touched, so
    // we can prove the wrapper does not swallow that class of error. A
    // path that already exists as a regular file (rather than a directory)
    // is the stable failure case under the auto-create contract.
    let bogus_path =
        std::env::temp_dir().join(format!("td3-offline-not-a-dir-{}", std::process::id()));
    std::fs::write(&bogus_path, b"oops").unwrap();
    let mut config = control_config_with_unmatched_port(std::env::temp_dir());
    config.control.backup_dir = Some(bogus_path.to_string_lossy().into_owned());
    let outcome = app::try_pre_ui_backup(&config);
    let _ = std::fs::remove_file(&bogus_path);
    match outcome {
        Err(Td3Error::BankBackupFailed(_)) => (),
        Err(other) => panic!("expected BankBackupFailed, got {:?}", other),
        Ok(_) => panic!("expected BankBackupFailed, got Ok"),
    }
}
