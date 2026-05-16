use crate::config::{parse_formats, parse_group, parse_pattern, Cli, Command};
use crate::error::Td3Error;
use crate::formats::Format;
use clap::Parser;

// ── parse_group ─────────────────────────────────────────────────────

#[test]
fn parse_group_valid_range() {
    assert_eq!(parse_group("1").unwrap(), 0);
    assert_eq!(parse_group("2").unwrap(), 1);
    assert_eq!(parse_group("3").unwrap(), 2);
    assert_eq!(parse_group("4").unwrap(), 3);
}

#[test]
fn parse_group_rejects_zero() {
    assert!(parse_group("0").is_err());
}

#[test]
fn parse_group_rejects_five() {
    assert!(parse_group("5").is_err());
}

#[test]
fn parse_group_rejects_non_numeric() {
    assert!(parse_group("abc").is_err());
}

// ── parse_pattern ───────────────────────────────────────────────────

#[test]
fn parse_pattern_valid() {
    assert_eq!(parse_pattern("1A").unwrap(), (0, 0));
    assert_eq!(parse_pattern("1B").unwrap(), (0, 1));
    assert_eq!(parse_pattern("8A").unwrap(), (7, 0));
    assert_eq!(parse_pattern("8B").unwrap(), (7, 1));
}

#[test]
fn parse_pattern_case_insensitive() {
    assert_eq!(parse_pattern("3a").unwrap(), (2, 0));
    assert_eq!(parse_pattern("3b").unwrap(), (2, 1));
}

#[test]
fn parse_pattern_rejects_zero() {
    assert!(parse_pattern("0A").is_err());
}

#[test]
fn parse_pattern_rejects_nine() {
    assert!(parse_pattern("9A").is_err());
}

#[test]
fn parse_pattern_rejects_bad_suffix() {
    assert!(parse_pattern("1C").is_err());
}

#[test]
fn parse_pattern_rejects_wrong_length() {
    assert!(parse_pattern("1").is_err());
    assert!(parse_pattern("1AB").is_err());
}

// ── parse_formats ───────────────────────────────────────────────────

#[test]
fn parse_formats_single() {
    let fmts = parse_formats("json").unwrap();
    assert_eq!(fmts, vec![Format::Json]);
}

#[test]
fn parse_formats_multiple() {
    let fmts = parse_formats("syx,toml,steps").unwrap();
    assert_eq!(fmts, vec![Format::Syx, Format::Toml, Format::StepsTxt]);
}

#[test]
fn parse_formats_with_spaces() {
    let fmts = parse_formats("json , steps").unwrap();
    assert_eq!(fmts, vec![Format::Json, Format::StepsTxt]);
}

#[test]
fn parse_formats_rejects_unknown() {
    assert!(parse_formats("midi").is_err());
}

#[test]
fn parse_formats_empty_string() {
    let fmts = parse_formats("").unwrap();
    assert!(fmts.is_empty());
}

// ── clap parsing ────────────────────────────────────────────────────

#[test]
fn clap_parses_export_subcommand() {
    let cli = Cli::try_parse_from(["td3-control", "export", "G1P1A"]).unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert_eq!(args.device.target.slot, "G1P1A");
            assert!(args.device.midi.midi_in.is_none());
            assert!(args.device.midi.midi_out.is_none());
            assert!(args.output.is_none());
            assert!(args.format.is_none());
            assert!(args.device.midi.timeout_ms.is_none());
            assert!(!args.device.midi.strict_device_name);
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_export_subcommand_dashed_form() {
    let cli = Cli::try_parse_from(["td3-control", "export", "G1-P1A"]).unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => assert_eq!(args.device.target.slot, "G1-P1A"),
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_import_subcommand() {
    let cli =
        Cli::try_parse_from(["td3-control", "import", "G2P3B", "--input", "pat.toml"]).unwrap();
    match cli.command.unwrap() {
        Command::Import(args) => {
            assert_eq!(args.device.target.slot, "G2P3B");
            assert_eq!(args.input, "pat.toml");
        }
        _ => panic!("expected Import command"),
    }
}

#[test]
fn clap_parses_list_ports() {
    let cli = Cli::try_parse_from(["td3-control", "list-ports"]).unwrap();
    assert!(matches!(cli.command.unwrap(), Command::ListPorts));
}

#[test]
fn clap_parses_custom_timeout() {
    let cli =
        Cli::try_parse_from(["td3-control", "export", "G1P1A", "--timeout-ms", "10000"]).unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert_eq!(args.device.midi.timeout_ms, Some(10000));
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_strict_device_name() {
    let cli =
        Cli::try_parse_from(["td3-control", "export", "G1P1A", "--strict-device-name"]).unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert!(args.device.midi.strict_device_name);
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_custom_ports() {
    let cli = Cli::try_parse_from([
        "td3-control",
        "export",
        "G1P1A",
        "--midi-in",
        "MyMIDI In",
        "--midi-out",
        "MyMIDI Out",
    ])
    .unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert_eq!(args.device.midi.midi_in.as_deref(), Some("MyMIDI In"));
            assert_eq!(args.device.midi.midi_out.as_deref(), Some("MyMIDI Out"));
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_format_flag() {
    let cli =
        Cli::try_parse_from(["td3-control", "export", "G1P1A", "--format", "json,syx"]).unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert_eq!(args.format.as_deref(), Some("json,syx"));
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_midi_loop_flag() {
    let cli = Cli::try_parse_from([
        "td3-control",
        "export",
        "G1P1A",
        "--format",
        "mid",
        "--loop",
        "128",
    ])
    .unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert_eq!(args.format.as_deref(), Some("mid"));
            assert_eq!(args.render.loop_count, Some(128));
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_midi_bars_flag() {
    let cli = Cli::try_parse_from([
        "td3-control",
        "export",
        "G1P1A",
        "--format",
        "mid",
        "--bars",
        "128",
    ])
    .unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert_eq!(args.format.as_deref(), Some("mid"));
            assert_eq!(args.render.bars, Some(128));
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_parses_output_short_flag() {
    let cli = Cli::try_parse_from(["td3-control", "export", "G1P1A", "-o", "out.toml"]).unwrap();
    match cli.command.unwrap() {
        Command::Export(args) => {
            assert_eq!(args.output.as_deref(), Some("out.toml"));
        }
        _ => panic!("expected Export command"),
    }
}

#[test]
fn clap_rejects_import_without_input() {
    let result = Cli::try_parse_from(["td3-control", "import", "G1P1A"]);
    assert!(result.is_err());
}

#[test]
fn clap_rejects_unknown_subcommand() {
    let result = Cli::try_parse_from(["td3-control", "delete", "G1P1A"]);
    assert!(result.is_err());
}

// ── P9.1: CLI error type tests ─────────────────────────────────────

#[test]
fn parse_group_returns_cli_error() {
    let err = parse_group("99").unwrap_err();
    assert!(matches!(err, Td3Error::CliError(_)));
    assert!(err.to_string().contains("99"));
}

#[test]
fn parse_pattern_returns_cli_error() {
    let err = parse_pattern("9Z").unwrap_err();
    assert!(matches!(err, Td3Error::CliError(_)));
}

#[test]
fn parse_formats_returns_format_error() {
    let err = parse_formats("unknown_fmt").unwrap_err();
    assert!(matches!(err, Td3Error::FormatError(_)));
    assert!(err.to_string().contains("unknown_fmt"));
}

#[test]
fn cli_error_message_prefix() {
    let err = Td3Error::CliError("test message".into());
    assert!(err.to_string().starts_with("invalid argument:"));
}
