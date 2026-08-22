use std::time::Duration;
use std::{fs, path::PathBuf};

use clap::Parser;

use crate::app_env::{AppEnv, CONFIG_FILE_PATH};
use crate::config::{
    ArtifactPaths, BankJob, Cli, Command, Config, ControlRuntime, MidiRuntime, Mode, RenderProfile,
};
use crate::formats::mid::{MidiExportOptions, MidiSlideMode};
use crate::formats::mid_import::MidiImportOptions;
use crate::formats::steps_txt;
use crate::pattern::{pattern_to_sysex, Pattern};

fn base_config(mode: Mode) -> Config {
    Config {
        mode,
        midi: MidiRuntime {
            input_port_name: "TD-3".to_string(),
            output_port_name: "TD-3".to_string(),
            request_timeout: Duration::from_millis(1),
            strict_name_match: true,
            retry_count: 0,
            device_channel: 1,
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

fn scratch_dir(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "td3-stepsdsl-{}-{}-{}",
        label,
        std::process::id(),
        stamp
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn render_steps_with_bpm(bpm: u32) -> String {
    steps_txt::export_with_integer_bpm(&Pattern::default(), bpm).unwrap()
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

#[test]
fn cli_steps_upload_ignores_document_bpm() {
    let dir = scratch_dir("upload-bpm");
    let fixture = include_str!("../../tests/fixtures/stepsdslv1_1.steps.txt");
    let variants = [
        fixture.replace("bpm=128\n", ""),
        fixture.to_string(),
        fixture.replace("bpm=128", "bpm=200"),
    ];
    let mut payloads = Vec::new();
    for (index, text) in variants.iter().enumerate() {
        let path = dir.join(format!("variant_{}.steps.txt", index));
        fs::write(&path, text).unwrap();
        let filename = path.to_string_lossy();
        let pattern = crate::app::import_file(&filename, &MidiImportOptions::default()).unwrap();
        payloads.push(pattern_to_sysex(&pattern, 0, 0, 0).unwrap());
    }

    assert_eq!(payloads[0], payloads[1]);
    assert_eq!(payloads[1], payloads[2]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_steps_upload_does_not_forward_bpm_to_protocol() {
    let import_pattern: fn(&str, &MidiImportOptions) -> Result<Pattern, crate::error::Td3Error> =
        crate::app::import_file;

    let pattern = import_pattern(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/stepsdslv1_1.steps.txt"
        ),
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert!(!pattern_to_sysex(&pattern, 0, 0, 0).unwrap().is_empty());
}

#[test]
fn cli_steps_download_uses_env_default_bpm() {
    let dir = scratch_dir("env-bpm");
    let path = dir.join(CONFIG_FILE_PATH);
    fs::write(&path, "UI_DEFAULT_BPM=156\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    let options = MidiExportOptions::from_env(&env);
    assert!(render_steps_with_bpm(options.bpm).contains("\nbpm=156\n"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_steps_download_uses_120_when_config_file_is_missing() {
    let dir = scratch_dir("missing-config");
    let path = dir.join(CONFIG_FILE_PATH);
    let (env, created) = AppEnv::load_or_create(&path).unwrap();
    assert!(created);
    assert_eq!(env.ui_default_bpm, 120);
    assert!(render_steps_with_bpm(env.ui_default_bpm).contains("\nbpm=120\n"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_steps_download_uses_120_when_bpm_key_is_missing() {
    let dir = scratch_dir("missing-bpm-key");
    let path = dir.join(CONFIG_FILE_PATH);
    fs::write(&path, "WEB_PORT=4040\n").unwrap();
    let (env, created) = AppEnv::load_or_create(&path).unwrap();
    assert!(!created);
    assert_eq!(env.ui_default_bpm, 120);
    assert!(render_steps_with_bpm(env.ui_default_bpm).contains("\nbpm=120\n"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_bpm_flag_overrides_env_for_download() {
    let dir = scratch_dir("cli-override");
    let path = dir.join(CONFIG_FILE_PATH);
    fs::write(&path, "UI_DEFAULT_BPM=156\n").unwrap();
    let (env, _) = AppEnv::load_or_create(&path).unwrap();
    let cli = Cli::try_parse_from([
        "td3-control",
        "convert",
        "input.steps.txt",
        "output.steps.txt",
        "--bpm",
        "222",
    ])
    .unwrap();
    let render = match cli.command.unwrap() {
        Command::Convert(args) => args.render.resolve(&env, Vec::new()),
        other => panic!("expected convert command, got {:?}", other),
    };
    assert_eq!(render.bpm, 222);
    assert!(render_steps_with_bpm(render.bpm).contains("\nbpm=222\n"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_steps_download_rejects_invalid_explicit_env_bpm() {
    for value in ["abc", "19", "301"] {
        let dir = scratch_dir("invalid-env-bpm");
        let path = dir.join(CONFIG_FILE_PATH);
        fs::write(&path, format!("UI_DEFAULT_BPM={}\n", value)).unwrap();
        let err = AppEnv::load_or_create(&path).unwrap_err().to_string();
        assert!(err.contains("UI_DEFAULT_BPM"), "got: {}", err);
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn cli_steps_convert_output_uses_resolved_bpm_and_short_rows() {
    let dir = scratch_dir("convert-output");
    let input = dir.join("input.steps.txt");
    let output = dir.join("output.steps.txt");
    fs::write(
        &input,
        include_str!("../../tests/fixtures/stepsdslv1_1.steps.txt"),
    )
    .unwrap();

    let mut config = base_config(Mode::Convert);
    config.files.input_path = Some(input.to_string_lossy().into_owned());
    config.files.output_path = Some(output.to_string_lossy().into_owned());
    config.render.bpm = 156;
    crate::app::run(config).unwrap();

    let text = fs::read_to_string(output).unwrap();
    assert!(text.contains("\nbpm=156\n"));
    assert!(text.contains("\n03  G:---:T|CO:64|GT:50\n"));
    assert!(!text.contains("\n04 "));
    let _ = fs::remove_dir_all(dir);
}
