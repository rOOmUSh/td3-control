use crate::config::{Config, Mode};
use crate::error::Td3Error;
use crate::formats;

use super::file_io::{export_package, export_single_file};
use super::pattern_import::import_file;
use super::session::CliDeviceSession;
use super::{required_input_path, required_target, validate_device_session_config};

/// Open MIDI ports, probe device, and run the requested operation.
pub(super) fn run_device_session(config: Config) -> Result<(), Td3Error> {
    validate_device_session_config(&config)?;
    let mut session = CliDeviceSession::open(&config)?;

    match &config.mode {
        Mode::Export => export_from_device(&mut session, &config),
        Mode::Import => import_to_device(&mut session, &config),
        Mode::ImportBank => run_import_bank_session(&mut session, config),
        Mode::ListPorts => unreachable!("list-ports handled before device session"),
        Mode::Control => unreachable!("control mode handled in main"),
        Mode::Convert => unreachable!("convert mode handled before device session"),
        Mode::ExtractBank => unreachable!("extract-bank handled before device session"),
        Mode::PackBank => unreachable!("pack-bank handled before device session"),
    }
}

fn export_from_device(session: &mut CliDeviceSession, config: &Config) -> Result<(), Td3Error> {
    let target = required_target(config, "export")?;
    let address = formats::format_address(target.patgroup, target.slot, target.side);
    let (raw_payload, pattern) =
        session.download_pattern(target.patgroup, target.slot, target.side)?;

    if config.files.output_path.is_some() {
        export_single_file(&pattern, &raw_payload, config, &address)
    } else {
        export_package(&pattern, &raw_payload, config, &address)
    }
}

fn import_to_device(session: &mut CliDeviceSession, config: &Config) -> Result<(), Td3Error> {
    let target = required_target(config, "import")?;
    let input = required_input_path(config, "import")?;
    let address = formats::format_address(target.patgroup, target.slot, target.side);
    let pattern = import_file(input, &config.midi_import_options())?;
    session.upload_pattern(&pattern, target.patgroup, target.slot, target.side)?;
    eprintln!("Imported {} to {}", input, address);
    Ok(())
}

fn run_import_bank_session(session: &mut CliDeviceSession, config: Config) -> Result<(), Td3Error> {
    let midi_opts = config.midi_export_options();
    let input =
        config.files.input_path.as_deref().ok_or_else(|| {
            Td3Error::CliError("import-bank mode requires an input path".to_string())
        })?;
    let partial = config.bank.partial.clone();
    let include_silent = config.bank.include_silent;
    let backup_dir = config.bank.backup_dir.clone();

    let partial_list = match partial {
        Some(csv) => Some(crate::bank::address::parse_partial(&csv)?),
        None => None,
    };

    let backup_dir_path = match backup_dir {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir()
            .map_err(|e| Td3Error::CliError(format!("cannot resolve CWD for backup-dir: {}", e)))?,
    };

    let opts = crate::bank::import::ImportOptions {
        source: std::path::PathBuf::from(input),
        partial: partial_list,
        include_silent,
        backup_dir: backup_dir_path,
        midi_opts,
    };

    let mut device = session.bank_device();
    let mut prompt = crate::bank::import::StdinPrompt;

    let report = crate::bank::import::import_bank(&opts, &mut device, &mut prompt)?;
    eprintln!(
        "\nDone. {} write(s) completed, backup at {}",
        report.writes_completed,
        report.backup.path.display()
    );
    Ok(())
}
