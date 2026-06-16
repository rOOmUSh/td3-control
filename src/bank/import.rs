//! `import-bank` orchestrator for the bank import safety protocol.
//!
//! Execution order:
//!
//! 1. **Parse** the source `.sqs` file.
//! 2. **Resolve** target address set (full bank or `--partial` filter).
//! 3. **Silent filter** - drop all-REST source patterns unless `--include-silent`.
//! 4. **Device read** - download all 64 device slots. Any read failure aborts
//!    the run before any disk write.
//! 5. **Backup write** - build + persist backup zip via the atomic protocol in
//!    `bank::backup`. Backup always covers the full device bank, not just
//!    target addresses, so the user can recover anything.
//! 6. **Diff** - compare source vs device on the active target set.
//! 7. **Confirmation** - print summary and prompt `[y/N]`, default `N`.
//! 8. **Upload** - write each differing target. On Ctrl-C: backup is already
//!    on disk, no data is lost.
//!
//! Device and user I/O are abstracted behind the [`BankDevice`] and
//! [`UserPrompt`] traits to keep the orchestrator pure and testable.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use crate::bank::address::{full_bank, BankAddress};
use crate::bank::backup::{write_backup_zip, BackupKind, BackupResult};
use crate::bank::diff::{compute_diff, format_address_list, DiffReport};
use crate::error::Td3Error;
use crate::formats::sqs::{is_silent, parse_bank, Bank, BankRecord, PAYLOAD_LEN, RECORD_COUNT};
use crate::td3_protocol;

// ---------------------------------------------------------------------------
// Abstractions (device + prompt) for testability
// ---------------------------------------------------------------------------

/// Minimal device I/O surface used by the import orchestrator.
pub trait BankDevice {
    /// Download the 112-byte payload for one slot.
    fn download(&mut self, group: u8, slot_addr: u8) -> Result<Vec<u8>, Td3Error>;

    /// Upload a 112-byte payload to one slot. Implementations must transmit
    /// bytes verbatim (no decode/re-encode), so CLEAR-residue bytes survive.
    fn upload(&mut self, group: u8, slot_addr: u8, payload: &[u8]) -> Result<(), Td3Error>;
}

/// User confirmation surface. The orchestrator renders the plan and delegates
/// the yes/no to this trait.
pub trait UserPrompt {
    /// Return `true` iff the user explicitly typed `y` or `Y`. Anything else
    /// (including EOF or an `n`) is a rejection.
    fn confirm(&mut self, prompt_text: &str) -> Result<bool, Td3Error>;
}

// ---------------------------------------------------------------------------
// Options + report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub source: PathBuf,
    pub partial: Option<Vec<BankAddress>>,
    pub include_silent: bool,
    pub backup_dir: PathBuf,
    /// Env-resolved MIDI export options. Used by the pre-import backup
    /// zip's per-record `.mid` rendering. Built from
    /// `Config::midi_export_options()` so the env file drives every
    /// runtime export (no hardcoded defaults at backup time).
    pub midi_opts: crate::formats::mid::MidiExportOptions,
}

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub backup: BackupResult,
    /// Number of successful uploads (may be < overwrite.len() if the upload
    /// stage was interrupted - though in the current sync impl, interruption
    /// propagates as an error and this will only count completed writes
    /// before the error).
    pub writes_completed: usize,
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

pub fn import_bank<D: BankDevice, P: UserPrompt>(
    opts: &ImportOptions,
    device: &mut D,
    prompt: &mut P,
) -> Result<ImportReport, Td3Error> {
    // Parse source.sqs.
    let source_bytes = fs::read(&opts.source).map_err(|e| {
        Td3Error::FormatError(format!("cannot read {}: {}", opts.source.display(), e))
    })?;
    let source_bank = parse_bank(&source_bytes)?;

    // Resolve the target set.
    let full_targets = match &opts.partial {
        None => full_bank(),
        Some(list) => {
            if list.is_empty() {
                return Err(Td3Error::CliError(
                    "--partial given but no valid addresses parsed".to_string(),
                ));
            }
            list.clone()
        }
    };

    // Apply the silent-pattern filter.
    let mut silent_skipped: Vec<BankAddress> = Vec::new();
    let active_targets: Vec<BankAddress> = if opts.include_silent {
        full_targets.clone()
    } else {
        full_targets
            .iter()
            .copied()
            .filter(|addr| {
                let rec = source_record(&source_bank, *addr);
                if is_silent(&rec.payload) {
                    silent_skipped.push(*addr);
                    false
                } else {
                    true
                }
            })
            .collect()
    };

    // Pre-flight: ensure backup-dir exists before any MIDI I/O so a missing
    // dir or unwritable path fails fast instead of after a ~30-60s full-bank read.
    crate::bank::backup::ensure_backup_dir(&opts.backup_dir)?;

    // Device read - always full bank, regardless of target filter.
    // The backup must be complete so the user can recover anything.
    eprintln!("Device read: reading all 64 device slots...");
    let device_bank = download_full_bank(device)?;

    // Persist the backup zip.
    eprintln!("Backup write: writing backup zip...");
    let backup = write_backup_zip(
        &opts.backup_dir,
        &device_bank,
        BackupKind::PreImport,
        &opts.midi_opts,
    )?;
    eprintln!(
        "  -> {} (SHA-256 {}…)",
        backup.path.display(),
        &backup.sha256_hex[..16]
    );

    // Compute the diff.
    let diff = compute_diff(&source_bank, &device_bank, &active_targets);

    // Prompt for confirmation.
    let summary = render_plan(opts, &backup, &active_targets, &silent_skipped, &diff);
    eprintln!();
    eprint!("{}", summary);
    eprintln!();

    if diff.overwrite.is_empty() {
        eprintln!("Nothing to write - source matches device on all target addresses. Exiting.");
        return Ok(ImportReport {
            backup,
            writes_completed: 0,
        });
    }

    let prompt_text = format!("Proceed with {} writes?  [y/N]: ", diff.overwrite.len());
    if !prompt.confirm(&prompt_text)? {
        return Err(Td3Error::BankImportAborted);
    }

    // Perform the sequential write.
    let mut writes_completed = 0usize;
    for addr in diff.overwrite.iter() {
        let rec = source_record(&source_bank, *addr);
        device.upload(rec.group, rec.slot_addr, &rec.payload)?;
        writes_completed += 1;
        eprintln!(
            "  wrote {} ({}/{})",
            addr.label(),
            writes_completed,
            diff.overwrite.len()
        );
    }

    Ok(ImportReport {
        backup,
        writes_completed,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn source_record(bank: &Bank, addr: BankAddress) -> &BankRecord {
    let idx = (addr.group as usize) * 16 + (addr.slot_addr as usize);
    &bank.records[idx]
}

pub(crate) fn download_full_bank<D: BankDevice>(device: &mut D) -> Result<Bank, Td3Error> {
    let mut records: Vec<BankRecord> = Vec::with_capacity(RECORD_COUNT);
    for g in 0u8..4 {
        for s in 0u8..16 {
            let payload = device.download(g, s)?;
            if payload.len() != PAYLOAD_LEN as usize {
                return Err(Td3Error::FormatError(format!(
                    "device returned {} bytes for G{}P{}{}, expected {}",
                    payload.len(),
                    g + 1,
                    (s & 0x7) + 1,
                    if (s >> 3) == 0 { "A" } else { "B" },
                    PAYLOAD_LEN
                )));
            }
            records.push(BankRecord {
                group: g,
                slot_addr: s,
                payload,
            });
        }
    }
    let records_arr: [BankRecord; RECORD_COUNT] =
        records.try_into().map_err(|_: Vec<BankRecord>| {
            Td3Error::FormatError("device bank size != 64".to_string())
        })?;

    Ok(Bank {
        product_bytes: crate::formats::sqs::PRODUCT_UTF16BE.to_vec(),
        version_bytes: crate::formats::sqs::VERSION_UTF16BE.to_vec(),
        records: records_arr,
    })
}

fn render_plan(
    opts: &ImportOptions,
    backup: &BackupResult,
    active_targets: &[BankAddress],
    silent_skipped: &[BankAddress],
    diff: &DiffReport,
) -> String {
    let mut s = String::new();
    s.push_str("== Bank import plan ===========================================\n");
    s.push_str(&format!("Source:         {}\n", opts.source.display()));
    s.push_str(&format!("Backup:         {}\n", backup.path.display()));
    s.push_str(&format!("                SHA-256 {}\n", backup.sha256_hex));
    s.push_str("                OK - full 64 slots captured on disk.\n\n");

    let filtered = 64usize.saturating_sub(active_targets.len() + silent_skipped.len());
    s.push_str(&format!(
        "Targets:        {} addresses ({} filtered by --partial, {} silent skipped)\n",
        active_targets.len(),
        filtered,
        silent_skipped.len()
    ));
    s.push_str(&format!(
        "Will overwrite: {} device patterns that differ from source\n",
        diff.overwrite_count()
    ));
    s.push_str(&format!(
        "No-op:          {} device patterns already identical\n",
        diff.noop_count()
    ));
    if !silent_skipped.is_empty() {
        s.push_str(&format!("Silent skipped: {}\n", silent_skipped.len()));
        s.push_str(&format!("{}\n", format_address_list(silent_skipped)));
        s.push_str("                Use --include-silent to force-write these.\n");
    }

    if !diff.overwrite.is_empty() {
        s.push_str("\nOverwrite list (differs from device):\n");
        s.push_str(&format_address_list(&diff.overwrite));
        s.push('\n');
    }

    s
}

// ---------------------------------------------------------------------------
// Concrete impls (MIDI device + stdin prompt)
// ---------------------------------------------------------------------------

/// MIDI-backed `BankDevice` wrapping a midir output connection and a
/// SysEx receive channel. The real CLI uses this; tests use `MockBankDevice`.
pub struct MidiBankDevice<'a> {
    pub out_conn: &'a mut midir::MidiOutputConnection,
    pub rx: &'a std::sync::mpsc::Receiver<Vec<u8>>,
    pub retries: u32,
    pub timeout: Duration,
}

impl<'a> BankDevice for MidiBankDevice<'a> {
    fn download(&mut self, group: u8, slot_addr: u8) -> Result<Vec<u8>, Td3Error> {
        let slot = slot_addr & 0x7;
        let side = slot_addr >> 3;
        let rt = self.retries;
        let to = self.timeout;
        let (raw_payload, _pattern) = td3_protocol::with_retry(rt, "bank download", || {
            td3_protocol::download_pattern(self.out_conn, self.rx, group, slot, side, to)
        })?;
        // raw_payload is the sysex body: [0x78, group, slot_addr, ...112 bytes].
        if raw_payload.len() < 3 + PAYLOAD_LEN as usize {
            return Err(Td3Error::FormatError(format!(
                "download returned {} bytes, expected >= {}",
                raw_payload.len(),
                3 + PAYLOAD_LEN
            )));
        }
        Ok(raw_payload[3..3 + PAYLOAD_LEN as usize].to_vec())
    }

    fn upload(&mut self, group: u8, slot_addr: u8, payload: &[u8]) -> Result<(), Td3Error> {
        td3_protocol::upload_raw_payload(
            self.out_conn,
            self.rx,
            group,
            slot_addr,
            payload,
            self.timeout,
        )
    }
}

/// Stdin-backed `UserPrompt` used by the CLI.
pub struct StdinPrompt;

impl UserPrompt for StdinPrompt {
    fn confirm(&mut self, prompt_text: &str) -> Result<bool, Td3Error> {
        let stdin = io::stdin();
        let mut stdout = io::stderr(); // user prompt to stderr (stdout stays for data)
        stdout
            .write_all(prompt_text.as_bytes())
            .map_err(|e| Td3Error::CliError(format!("prompt write failed: {}", e)))?;
        stdout
            .flush()
            .map_err(|e| Td3Error::CliError(format!("prompt flush failed: {}", e)))?;

        let mut line = String::new();
        let n = stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| Td3Error::CliError(format!("stdin read failed: {}", e)))?;
        if n == 0 {
            // EOF without reply → treat as N.
            return Ok(false);
        }
        Ok(matches!(line.trim(), "y" | "Y"))
    }
}
