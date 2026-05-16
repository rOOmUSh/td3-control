use crate::config::Config;
use crate::error::Td3Error;

use super::required_output_path;

pub(super) fn run_extract_bank(config: Config) -> Result<(), Td3Error> {
    let midi_opts = config.midi_export_options();
    let input = config.files.input_path.as_deref().ok_or_else(|| {
        Td3Error::CliError("extract-bank mode requires an input path".to_string())
    })?;
    let output = required_output_path(&config, "extract-bank")?;
    let force = config.bank.overwrite_existing;
    let input_path = std::path::PathBuf::from(&input);
    let output_path = std::path::PathBuf::from(&output);
    crate::bank::extract_bank(&input_path, &output_path, force, &midi_opts)?;
    eprintln!("Extracted {} -> {}/", input, output);
    Ok(())
}

pub(super) fn run_pack_bank(config: Config) -> Result<(), Td3Error> {
    let midi_import_opts = config.midi_import_options();
    let input =
        config.files.input_path.as_deref().ok_or_else(|| {
            Td3Error::CliError("pack-bank mode requires an input path".to_string())
        })?;
    let output = required_output_path(&config, "pack-bank")?;
    let force = config.bank.overwrite_existing;
    let input_path = std::path::PathBuf::from(&input);
    let output_path = std::path::PathBuf::from(&output);
    crate::bank::pack_bank(&input_path, &output_path, force, &midi_import_opts)?;
    eprintln!("Packed {}/ -> {}", input, output);
    Ok(())
}
