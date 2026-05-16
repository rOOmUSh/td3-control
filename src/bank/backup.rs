//! Pre-import backup zip writer (atomic, content-addressed by SHA-256).
//!
//! Protocol:
//!   1. Build the full zip archive in memory (`bank.sqs`, `bank_manifest.json`,
//!      64 × 6 per-format files).
//!   2. Write to `<base>.zip.tmp`, flush, `sync_all`, close.
//!   3. Rename `.zip.tmp` → `.zip` (single atomic step on local FS).
//!   4. Read the on-disk bytes, compute SHA-256, take first 16 hex chars.
//!   5. Rename `.zip` → `<base>-<short-hash>.zip`.
//!
//! Step 2 guarantees the zip is fully flushed before any hash is computed.
//! Step 5 is a pure rename; bytes don't change, so the hash is verifiable
//! forever.

use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use crate::bank::manifest::BankManifest;
use crate::error::Td3Error;
use crate::formats::mid::MidiExportOptions;
use crate::formats::sqs::{folder_name, serialize_bank, Bank, BankRecord};
use crate::formats::{self, Format};
use crate::pattern::{sysex_to_pattern, Pattern};

/// Persisted backup metadata returned to the caller.
#[derive(Debug, Clone)]
pub struct BackupResult {
    pub path: PathBuf,
    pub sha256_hex: String,
}

/// Distinguishes the workflow that triggered the backup so users (and other
/// users digging through `backup_dir` weeks later) can tell at a glance
/// which zip protects which operation.
///
/// - `PreImport`: auto-dump taken by `import-bank` before device writes.
///   Filename stem: `bank_preimport_backup_<TS>`.
/// - `PreUi`: auto-dump taken by the `control` UI session before the
///   scratch-pattern prompt. Filename stem: `bank_ui_backup_<TS>` - the
///   literal `_ui_` substring is the marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    PreImport,
    PreUi,
}

impl BackupKind {
    fn stem_tag(self) -> &'static str {
        match self {
            BackupKind::PreImport => "preimport",
            BackupKind::PreUi => "ui",
        }
    }
}

/// Ensure `backup_dir` exists as a directory. Creates it (including parents)
/// when missing. Errors when the path exists but is a regular file.
pub fn ensure_backup_dir(backup_dir: &Path) -> Result<(), Td3Error> {
    if backup_dir.exists() {
        if !backup_dir.is_dir() {
            return Err(Td3Error::BankBackupFailed(format!(
                "backup-dir path exists but is not a directory: {}",
                backup_dir.display()
            )));
        }
        return Ok(());
    }
    fs::create_dir_all(backup_dir).map_err(|e| {
        Td3Error::BankBackupFailed(format!("create backup-dir {}: {}", backup_dir.display(), e))
    })
}

/// Build and persist a backup zip in `backup_dir`.
///
/// `backup_dir` is created (including parents) if it does not already exist.
/// If the path exists but is not a directory, the call fails.
pub fn write_backup_zip(
    backup_dir: &Path,
    bank: &Bank,
    kind: BackupKind,
    midi_opts: &MidiExportOptions,
) -> Result<BackupResult, Td3Error> {
    ensure_backup_dir(backup_dir)?;

    let base = choose_base_name(backup_dir, kind)?;
    let tmp_path = backup_dir.join(format!("{}.zip.tmp", base));
    let zip_path = backup_dir.join(format!("{}.zip", base));

    let zip_bytes = build_zip_bytes(bank, midi_opts)?;

    write_file_synced(&tmp_path, &zip_bytes)?;

    fs::rename(&tmp_path, &zip_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        Td3Error::BankBackupFailed(format!(
            "rename {} -> {}: {}",
            tmp_path.display(),
            zip_path.display(),
            e
        ))
    })?;

    let on_disk = fs::read(&zip_path).map_err(|e| {
        Td3Error::BankBackupFailed(format!("readback {}: {}", zip_path.display(), e))
    })?;
    let hash_hex = sha256_hex(&on_disk);
    let short = &hash_hex[..16];

    let final_path = backup_dir.join(format!("{}-{}.zip", base, short));
    fs::rename(&zip_path, &final_path).map_err(|e| {
        Td3Error::BankBackupFailed(format!(
            "rename {} -> {}: {}",
            zip_path.display(),
            final_path.display(),
            e
        ))
    })?;

    Ok(BackupResult {
        path: final_path,
        sha256_hex: hash_hex,
    })
}

/// Pick a collision-free base name. Timestamp at second granularity; append
/// PID if a same-second backup already exists in the dir.
fn choose_base_name(dir: &Path, kind: BackupKind) -> Result<String, Td3Error> {
    let ts = timestamp_utc(SystemTime::now());
    let base = format!("bank_{}_backup_{}", kind.stem_tag(), ts);
    if !collides(dir, &base)? {
        return Ok(base);
    }
    let with_pid = format!("{}-{}", base, std::process::id());
    if !collides(dir, &with_pid)? {
        return Ok(with_pid);
    }
    Err(Td3Error::BankBackupFailed(format!(
        "cannot find unique backup filename (tried {} and {})",
        base, with_pid
    )))
}

fn collides(dir: &Path, base: &str) -> Result<bool, Td3Error> {
    if dir.join(format!("{}.zip.tmp", base)).exists() || dir.join(format!("{}.zip", base)).exists()
    {
        return Ok(true);
    }
    let prefix = format!("{}-", base);
    let entries = fs::read_dir(dir)
        .map_err(|e| Td3Error::BankBackupFailed(format!("readdir {}: {}", dir.display(), e)))?;
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with(&prefix) && name.ends_with(".zip") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn write_file_synced(path: &Path, bytes: &[u8]) -> Result<(), Td3Error> {
    let mut f = File::create(path)
        .map_err(|e| Td3Error::BankBackupFailed(format!("create {}: {}", path.display(), e)))?;
    f.write_all(bytes)
        .map_err(|e| Td3Error::BankBackupFailed(format!("write {}: {}", path.display(), e)))?;
    f.sync_all()
        .map_err(|e| Td3Error::BankBackupFailed(format!("fsync {}: {}", path.display(), e)))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Archive construction
// ---------------------------------------------------------------------------

fn build_zip_bytes(bank: &Bank, midi_opts: &MidiExportOptions) -> Result<Vec<u8>, Td3Error> {
    let mut buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        let sqs_bytes = serialize_bank(bank)?;
        zip_write(&mut zip, "bank.sqs", &sqs_bytes, opts)?;

        let manifest = BankManifest::from_bank(bank);
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| Td3Error::BankBackupFailed(format!("manifest serialize: {}", e)))?;
        zip_write(&mut zip, "bank_manifest.json", &manifest_json, opts)?;

        for rec in bank.records.iter() {
            write_record_files(&mut zip, rec, midi_opts, opts)?;
        }

        zip.finish()
            .map_err(|e| Td3Error::BankBackupFailed(format!("zip finalize: {}", e)))?;
    }
    Ok(buf.into_inner())
}

fn zip_write<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    data: &[u8],
    opts: SimpleFileOptions,
) -> Result<(), Td3Error> {
    zip.start_file(name, opts)
        .map_err(|e| Td3Error::BankBackupFailed(format!("start_file {}: {}", name, e)))?;
    zip.write_all(data)
        .map_err(|e| Td3Error::BankBackupFailed(format!("write {}: {}", name, e)))?;
    Ok(())
}

fn write_record_files<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    rec: &BankRecord,
    midi_opts: &MidiExportOptions,
    opts: SimpleFileOptions,
) -> Result<(), Td3Error> {
    let address = folder_name(rec.group, rec.slot_addr);
    let mut raw_sysex = Vec::with_capacity(3 + rec.payload.len());
    raw_sysex.push(0x78);
    raw_sysex.push(rec.group);
    raw_sysex.push(rec.slot_addr);
    raw_sysex.extend_from_slice(&rec.payload);
    let pattern = sysex_to_pattern(&raw_sysex)?;

    for fmt in Format::all() {
        let name = format!("{}/{}.{}", address, address, fmt.extension());
        let data = render_format(*fmt, &pattern, &raw_sysex, &address, midi_opts)?;
        zip_write(zip, &name, &data, opts)?;
    }
    Ok(())
}

fn render_format(
    fmt: Format,
    pattern: &Pattern,
    raw_sysex: &[u8],
    address: &str,
    midi_opts: &MidiExportOptions,
) -> Result<Vec<u8>, Td3Error> {
    Ok(match fmt {
        Format::Syx => formats::syx::export_raw(raw_sysex),
        Format::Toml => formats::toml_fmt::export(pattern)?.into_bytes(),
        Format::Json => formats::json::export(pattern)?.into_bytes(),
        Format::Mid => formats::mid::export(pattern, address, midi_opts)?,
        Format::StepsTxt => formats::steps_txt::export(pattern).into_bytes(),
        Format::Seq => formats::seq::export(pattern)?,
        Format::Pat => formats::pat::export(pattern).into_bytes(),
        // `.rbs` is a bank-level 64-pattern format; it is never emitted as a
        // per-pattern sidecar. `Format::all()` intentionally omits it, so this
        // arm is unreachable in practice but kept as a defence-in-depth guard.
        Format::Rbs => {
            return Err(Td3Error::FormatError(
                ".rbs is a bank-level format; per-pattern rendering is not supported".to_string(),
            ))
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest.iter() {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// UTC timestamp `YYYY-MM-DD_HH-MM-SS` (no chrono dep). Howard Hinnant's civil_from_days.
fn timestamp_utc(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = (secs / 86400) as i64;

    let z = days + 719468;
    let era = if z >= 0 {
        z / 146097
    } else {
        (z - 146096) / 146097
    };
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36525 - doe / 146096) / 365;
    let y_base = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y_base + 1 } else { y_base };

    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        year, month, d, h, m, s
    )
}
