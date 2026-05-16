use std::io::Write;
use std::time::Duration;

use clap::{CommandFactory, Parser};

use crate::app_env::AppEnv;
use crate::error::Td3Error;
use crate::formats::Format;

use super::cli::{
    Cli, Command, ControlArgs, ConvertArgs, ExportArgs, ExtractBankArgs, ImportArgs,
    ImportBankArgs, MidiDeviceArgs, MidiRenderArgs, PackBankArgs, PatternSlotArgs,
};
use super::model::{Config, MidiRuntime, Mode, RenderProfile};
use super::parse::{parse_formats, parse_pattern_address};

impl PatternSlotArgs {
    fn resolve(&self) -> Result<super::model::PatternAddress, Td3Error> {
        parse_pattern_address(&self.slot)
    }
}

impl MidiDeviceArgs {
    fn resolve(&self, env: &AppEnv) -> MidiRuntime {
        MidiRuntime {
            input_port_name: self
                .midi_in
                .clone()
                .unwrap_or_else(|| env.midi_port_substring.clone()),
            output_port_name: self
                .midi_out
                .clone()
                .unwrap_or_else(|| env.midi_port_substring.clone()),
            request_timeout: Duration::from_millis(self.timeout_ms.unwrap_or(env.midi_timeout_ms)),
            strict_name_match: self.strict_device_name || env.midi_strict_name_match,
            retry_count: self.retries.unwrap_or(env.midi_retries),
        }
    }
}

impl MidiRenderArgs {
    fn resolve(&self, env: &AppEnv, requested_formats: Vec<Format>) -> RenderProfile {
        RenderProfile {
            requested_formats,
            bpm: self.bpm.unwrap_or(env.ui_default_bpm),
            ppqn: self.ppqn.unwrap_or(env.midi_export_ppqn),
            midi_channel: self.mid_channel.unwrap_or(env.midi_export_channel),
            octave_offset: self
                .mid_octave_offset
                .unwrap_or(env.midi_export_octave_offset),
            accent_velocity: self
                .mid_accent_velocity
                .unwrap_or(env.midi_export_accent_velocity),
            normal_velocity: self
                .mid_normal_velocity
                .unwrap_or(env.midi_export_normal_velocity),
            slide_mode: self.mid_slide.unwrap_or(env.midi_export_slide_mode),
            loop_count: self.loop_count.unwrap_or(env.midi_export_loop_count),
            bars: self.bars,
        }
    }
}

fn parse_cli() -> Result<Cli, Td3Error> {
    match Cli::try_parse() {
        Ok(parsed) => Ok(parsed),
        Err(parse_error) => {
            if parse_error.kind() == clap::error::ErrorKind::DisplayHelp
                || parse_error.kind() == clap::error::ErrorKind::DisplayVersion
            {
                parse_error.exit();
            }
            Err(Td3Error::CliError(parse_error.to_string()))
        }
    }
}

fn print_help_and_exit() -> ! {
    let mut command = Cli::command();
    let exit_code = match command.print_help() {
        Ok(()) => {
            let mut stdout = std::io::stdout();
            if stdout.write_all(b"\n").is_err() {
                2
            } else {
                0
            }
        }
        Err(_) => 2,
    };
    std::process::exit(exit_code);
}

fn env_backup_dir(env: &AppEnv) -> Option<String> {
    let raw = env.backup_dir_path.trim();
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_owned())
    }
}

fn resolve_backup_dir(cli_value: &Option<String>, env: &AppEnv) -> Option<String> {
    cli_value.clone().or_else(|| env_backup_dir(env))
}

fn resolve_export_config(args: ExportArgs, env: &AppEnv) -> Result<Config, Td3Error> {
    let requested_formats = match &args.format {
        Some(raw) => parse_formats(raw)?,
        None => Vec::new(),
    };
    let mut config = Config::new(Mode::Export, env);
    config.midi = args.device.midi.resolve(env);
    config.target = Some(args.device.target.resolve()?);
    config.files.output_path = args.output;
    config.render = args.render.resolve(env, requested_formats);
    config.with_validated_render()
}

fn resolve_import_config(args: ImportArgs, env: &AppEnv) -> Result<Config, Td3Error> {
    let mut config = Config::new(Mode::Import, env);
    config.midi = args.device.midi.resolve(env);
    config.target = Some(args.device.target.resolve()?);
    config.files.input_path = Some(args.input);
    Ok(config)
}

fn resolve_convert_config(args: ConvertArgs, env: &AppEnv) -> Result<Config, Td3Error> {
    let mut config = Config::new(Mode::Convert, env);
    config.files.input_path = Some(args.input);
    config.files.output_path = Some(args.output);
    config.render = args.render.resolve(env, Vec::new());
    config.with_validated_render()
}

fn resolve_extract_bank_config(args: ExtractBankArgs, env: &AppEnv) -> Config {
    let mut config = Config::new(Mode::ExtractBank, env);
    config.files.input_path = Some(args.input);
    config.files.output_path = Some(args.output);
    config.bank.overwrite_existing = args.force;
    config
}

fn resolve_pack_bank_config(args: PackBankArgs, env: &AppEnv) -> Config {
    let mut config = Config::new(Mode::PackBank, env);
    config.files.input_path = Some(args.input);
    config.files.output_path = Some(args.output);
    config.bank.overwrite_existing = args.force;
    config
}

fn resolve_import_bank_config(args: ImportBankArgs, env: &AppEnv) -> Config {
    let mut config = Config::new(Mode::ImportBank, env);
    config.midi = args.midi.resolve(env);
    config.files.input_path = Some(args.input);
    config.bank.partial = args.partial;
    config.bank.include_silent = args.include_silent;
    config.bank.backup_dir = resolve_backup_dir(&args.backup_dir, env);
    config
}

fn resolve_control_config(args: ControlArgs, env: &AppEnv) -> Result<Config, Td3Error> {
    let scratch_raw = args
        .scratch_pattern
        .clone()
        .unwrap_or_else(|| env.ui_scratch_pattern.clone());

    let mut config = Config::new(Mode::Control, env);
    config.midi = args.midi.resolve(env);
    config.control.bind_address = args.bind.unwrap_or_else(|| env.web_bind.clone());
    config.control.listen_port = args.port.unwrap_or(env.web_port);
    config.control.scratch_slot = Some(parse_pattern_address(&scratch_raw)?);
    config.control.backup_dir = resolve_backup_dir(&args.backup_dir, env);
    Ok(config)
}

/// Parse CLI arguments and return a validated Config.
///
/// CLI flags override `env` values, which override bundled template defaults
/// already resolved in `env`.
pub fn load_config(env: &AppEnv) -> Result<Config, Td3Error> {
    let cli = parse_cli()?;

    match cli.command {
        None => print_help_and_exit(),
        Some(Command::ListPorts) => Ok(Config::new(Mode::ListPorts, env)),
        Some(Command::Control(args)) => resolve_control_config(args, env),
        Some(Command::Export(args)) => resolve_export_config(args, env),
        Some(Command::Convert(args)) => resolve_convert_config(args, env),
        Some(Command::ExtractBank(args)) => Ok(resolve_extract_bank_config(args, env)),
        Some(Command::PackBank(args)) => Ok(resolve_pack_bank_config(args, env)),
        Some(Command::ImportBank(args)) => Ok(resolve_import_bank_config(args, env)),
        Some(Command::Import(args)) => resolve_import_config(args, env),
    }
}
