//! Per-pattern diff between a source `.sqs` and the device-current bank.
//!
//! The comparison is byte-exact on the 112-byte payload. This catches any
//! field-level change (note, transpose, accent, slide, time, triplet, active
//! steps) without having to enumerate fields. Marker bytes are compared too -
//! a pure origin-marker diff is reported as a difference (the device won't
//! notice, but the user should know the source was last touched by a
//! different tool).

use crate::bank::address::BankAddress;
use crate::formats::sqs::{folder_name, Bank};

/// Summary of how a planned write set compares to current device state.
#[derive(Debug, Clone)]
pub struct DiffReport {
    /// Addresses whose source payload differs from the current device payload.
    pub overwrite: Vec<BankAddress>,
    /// Addresses where source and device are already byte-identical.
    pub noop: Vec<BankAddress>,
}

impl DiffReport {
    pub fn overwrite_count(&self) -> usize {
        self.overwrite.len()
    }

    pub fn noop_count(&self) -> usize {
        self.noop.len()
    }
}

/// Compute `DiffReport` for `targets` only. `source` and `device` must both be
/// full 64-record banks (the caller filters at the target-set stage, not here).
pub fn compute_diff(source: &Bank, device: &Bank, targets: &[BankAddress]) -> DiffReport {
    let mut overwrite = Vec::new();
    let mut noop = Vec::new();

    for &addr in targets {
        let idx = bank_index(addr);
        let src_rec = &source.records[idx];
        let dev_rec = &device.records[idx];
        if src_rec.payload == dev_rec.payload {
            noop.push(addr);
        } else {
            overwrite.push(addr);
        }
    }

    DiffReport { overwrite, noop }
}

/// File-order index of a `(group, slot_addr)` in a `Bank.records` array.
fn bank_index(addr: BankAddress) -> usize {
    (addr.group as usize) * 16 + (addr.slot_addr as usize)
}

/// Pretty-print a list of addresses wrapped to 12 per line, sorted file-order.
/// Used by the confirmation UI in `import.rs`.
pub fn format_address_list(addrs: &[BankAddress]) -> String {
    let mut labels: Vec<String> = addrs
        .iter()
        .map(|a| folder_name(a.group, a.slot_addr))
        .collect();
    labels.sort();
    let mut out = String::new();
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            if i % 12 == 0 {
                out.push('\n');
                out.push_str("  ");
            } else {
                out.push(' ');
            }
        } else {
            out.push_str("  ");
        }
        out.push_str(label);
    }
    out
}
