use std::io;
use std::path::Path;

use crate::error::Td3Error;

/// Bundled factory defaults.
pub const DEFAULT_TEMPLATE: &str = include_str!("../../config/default_env.template");

/// Runtime config file path relative to the current working directory.
pub const CONFIG_FILE_PATH: &str = "TD3_CONFIG.env";

pub(super) fn write_template(path: &Path) -> Result<(), Td3Error> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let safe_path = crate::path_safety::require_safe_user_path(path)?;
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&safe_path)
    {
        Ok(f) => f,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            return Ok(());
        }
        Err(err) => return Err(Td3Error::Io(err)),
    };
    file.write_all(DEFAULT_TEMPLATE.as_bytes())
        .map_err(Td3Error::Io)?;
    Ok(())
}
