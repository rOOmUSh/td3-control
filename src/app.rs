//! Application orchestration layer.
//!
//! Coordinates CLI config, MIDI transport, TD-3 protocol, domain model,
//! and file I/O into high-level operations. The CLI calls this layer;
//! this layer calls everything else.

use crate::config::{Config, Mode, PatternAddress};
use crate::error::Td3Error;

mod bank_workflows;
mod control;
mod device;
mod file_io;
mod pattern_import;
mod ports;
mod session;

#[allow(unused_imports)]
pub use control::run_control_backup_session;
pub use control::{force_usb_sync, try_pre_ui_backup};
pub use pattern_import::import_file;

use bank_workflows::{run_extract_bank, run_pack_bank};
use device::run_device_session;
use file_io::run_convert;
use ports::list_ports;

/// Run the application with the given config.
pub fn run(config: Config) -> Result<(), Td3Error> {
    match &config.mode {
        Mode::ListPorts => list_ports(),
        Mode::Export | Mode::Import | Mode::ImportBank => run_device_session(config),
        Mode::Control => Err(Td3Error::CliError(
            "control mode must be started through the web server path".to_string(),
        )),
        Mode::Convert => run_convert(config),
        Mode::ExtractBank => run_extract_bank(config),
        Mode::PackBank => run_pack_bank(config),
    }
}

fn required_input_path<'a>(config: &'a Config, mode: &str) -> Result<&'a str, Td3Error> {
    config
        .files
        .input_path
        .as_deref()
        .ok_or_else(|| Td3Error::CliError(format!("{} mode requires an input path", mode)))
}

fn required_output_path<'a>(config: &'a Config, mode: &str) -> Result<&'a str, Td3Error> {
    config
        .files
        .output_path
        .as_deref()
        .ok_or_else(|| Td3Error::CliError(format!("{} mode requires an output path", mode)))
}

pub(crate) fn required_target(config: &Config, mode: &str) -> Result<PatternAddress, Td3Error> {
    config.target.ok_or_else(|| {
        Td3Error::CliError(format!("{} mode requires resolved pattern target", mode))
    })
}

fn validate_device_session_config(config: &Config) -> Result<(), Td3Error> {
    match config.mode {
        Mode::Export => {
            required_target(config, "export")?;
        }
        Mode::Import => {
            required_target(config, "import")?;
            required_input_path(config, "import")?;
        }
        Mode::ImportBank => {
            required_input_path(config, "import-bank")?;
        }
        _ => {}
    }
    Ok(())
}
