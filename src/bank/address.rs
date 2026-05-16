//! `--partial` CSV parser for `import-bank`.
//!
//! Grammar (case-insensitive, whitespace-tolerant):
//!
//! ```text
//! list    = entry ("," entry)*
//! entry   = group "-" slot side
//! group   = "1".."4"
//! slot    = "1".."8"
//! side    = "A" | "B"
//! ```
//!
//! Duplicates are rejected (user intent must be unambiguous). Order is preserved.

use crate::error::Td3Error;

/// 0-indexed bank address, matching the fields on `BankRecord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BankAddress {
    pub group: u8,
    pub slot_addr: u8,
}

impl BankAddress {
    /// Human form, e.g. `G1P1A`.
    pub fn label(&self) -> String {
        crate::formats::sqs::folder_name(self.group, self.slot_addr)
    }
}

/// Parse the `--partial` CSV into a deduplicated list preserving input order.
///
/// Empty input (after trim) returns an empty list; the caller decides whether
/// that means "full bank" (no filter) or an error.
pub fn parse_partial(input: &str) -> Result<Vec<BankAddress>, Td3Error> {
    let mut out: Vec<BankAddress> = Vec::new();

    for token in input.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let addr = parse_entry(trimmed)?;
        if out.contains(&addr) {
            return Err(Td3Error::BankAddressDuplicate(addr.label()));
        }
        out.push(addr);
    }

    Ok(out)
}

/// Parse a single entry like `1-1A`, `2-3b`, or `" 1 - 1A "` into a `BankAddress`.
fn parse_entry(s: &str) -> Result<BankAddress, Td3Error> {
    // Strip all whitespace; the grammar is structural, not layout-sensitive.
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(Td3Error::BankAddressInvalid(s.to_string()));
    }

    let dash = compact
        .find('-')
        .ok_or_else(|| Td3Error::BankAddressInvalid(s.to_string()))?;

    let group_str = &compact[..dash];
    let rest = &compact[dash + 1..];
    if rest.len() != 2 {
        return Err(Td3Error::BankAddressInvalid(s.to_string()));
    }

    let group_num: u8 = group_str
        .parse()
        .map_err(|_| Td3Error::BankAddressInvalid(s.to_string()))?;
    if !(1..=4).contains(&group_num) {
        return Err(Td3Error::BankAddressInvalid(s.to_string()));
    }

    let slot_num: u8 = rest[0..1]
        .parse()
        .map_err(|_| Td3Error::BankAddressInvalid(s.to_string()))?;
    if !(1..=8).contains(&slot_num) {
        return Err(Td3Error::BankAddressInvalid(s.to_string()));
    }

    let side: u8 = match rest.as_bytes()[1] {
        b'A' | b'a' => 0,
        b'B' | b'b' => 1,
        _ => return Err(Td3Error::BankAddressInvalid(s.to_string())),
    };

    Ok(BankAddress {
        group: group_num - 1,
        slot_addr: (slot_num - 1) | (side << 3),
    })
}

/// Every slot address in file order: `(group=0..3) × (slot_addr=0..15)`.
pub fn full_bank() -> Vec<BankAddress> {
    let mut v = Vec::with_capacity(64);
    for g in 0u8..4 {
        for s in 0u8..16 {
            v.push(BankAddress {
                group: g,
                slot_addr: s,
            });
        }
    }
    v
}
