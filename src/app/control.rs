use super::session::CliDeviceSession;
use crate::config::Config;
use crate::error::Td3Error;

/// Open a MIDI session, download all 64 device slots, and write a
/// `BackupKind::PreUi` zip to `config.control.backup_dir` (or CWD if unset).
///
/// Called from `main::run` before the scratch-pattern confirmation prompt so
/// the warning box's promise ("a full device bank backup will be created
/// before any writes occur") is fulfilled on disk, not just in the browser's
/// IndexedDB. Ports are opened and closed inside this function so the web
/// server can reopen them afterwards without fighting midir's single-open
/// restriction on Windows.
pub fn run_control_backup_session(
    config: &Config,
) -> Result<crate::bank::backup::BackupResult, Td3Error> {
    let backup_dir_path = match &config.control.backup_dir {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir()
            .map_err(|e| Td3Error::CliError(format!("cannot resolve CWD for backup-dir: {}", e)))?,
    };

    crate::bank::backup::ensure_backup_dir(&backup_dir_path)?;

    let mut session = CliDeviceSession::open(config)?;

    eprintln!("Pre-UI-session backup: reading all 64 device slots...");
    let mut device = session.bank_device();
    let bank = crate::bank::import::download_full_bank(&mut device)?;

    let result = crate::bank::backup::write_backup_zip(
        &backup_dir_path,
        &bank,
        crate::bank::backup::BackupKind::PreUi,
        &config.midi_export_options(),
    )?;
    eprintln!(
        "Pre-UI-session backup written: {} (SHA-256 {}…)",
        result.path.display(),
        &result.sha256_hex[..16]
    );

    // `session` drops here, releasing the MIDI ports for the web server
    // to reopen.
    Ok(result)
}

/// Attempt the pre-UI bank backup, returning `Ok(None)` if no TD-3 is
/// connected so the web UI can still come up in offline mode.
///
/// `PortNotFound` is the only error mapped to "offline" - every other
/// failure (timeout, busy, malformed reply, disk error) still aborts so we
/// don't fake success on a half-broken device.
pub fn try_pre_ui_backup(
    config: &Config,
) -> Result<Option<crate::bank::backup::BackupResult>, Td3Error> {
    match run_control_backup_session(config) {
        Ok(result) => Ok(Some(result)),
        Err(Td3Error::PortNotFound { .. }) => Ok(None),
        Err(other) => Err(other),
    }
}

/// Open a short-lived MIDI session and set the TD-3 sequencer clock source
/// to MIDI USB. Logs the outcome and never propagates a failure.
pub fn force_usb_sync(config: &Config) {
    match try_force_usb_sync(config) {
        Ok(()) => eprintln!("Sync source set to USB."),
        Err(err) => eprintln!("Could not force USB sync (continuing anyway): {}", err),
    }
}

fn try_force_usb_sync(config: &Config) -> Result<(), Td3Error> {
    let mut session = CliDeviceSession::open(config)?;
    session.set_sync_source(crate::td3_protocol::SyncSource::MidiUsb)
}
