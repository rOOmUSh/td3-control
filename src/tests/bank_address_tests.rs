// Tests for `bank::address::parse_partial` and `BankAddress` helpers.
//
// The `--partial` CLI flag is the single user-facing way to pick a subset of
// the 64 bank addresses. Any parser slack here becomes a data-loss risk, so
// every malformed / edge-case input must be rejected with a clear error.

use crate::bank::address::{full_bank, parse_partial, BankAddress};
use crate::error::Td3Error;

// ---------------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------------

#[test]
fn parse_single_entry_a_side() {
    let got = parse_partial("1-1A").unwrap();
    assert_eq!(
        got,
        vec![BankAddress {
            group: 0,
            slot_addr: 0
        }]
    );
}

#[test]
fn parse_single_entry_b_side() {
    let got = parse_partial("4-8B").unwrap();
    // group=3 (G4), slot=7, side=1 → slot_addr = 7 | 8 = 15
    assert_eq!(
        got,
        vec![BankAddress {
            group: 3,
            slot_addr: 15
        }]
    );
}

#[test]
fn parse_multi_entry_preserves_order() {
    let got = parse_partial("1-1A,2-3B,4-8A").unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(
        got[0],
        BankAddress {
            group: 0,
            slot_addr: 0
        }
    );
    // group=1 (G2), slot=2, side=1 → slot_addr = 2 | 8 = 10
    assert_eq!(
        got[1],
        BankAddress {
            group: 1,
            slot_addr: 10
        }
    );
    // group=3 (G4), slot=7, side=0 → slot_addr = 7
    assert_eq!(
        got[2],
        BankAddress {
            group: 3,
            slot_addr: 7
        }
    );
}

#[test]
fn parse_is_case_insensitive_for_side() {
    let upper = parse_partial("1-1A,2-2B").unwrap();
    let lower = parse_partial("1-1a,2-2b").unwrap();
    let mixed = parse_partial("1-1a,2-2B").unwrap();
    assert_eq!(upper, lower);
    assert_eq!(upper, mixed);
}

#[test]
fn parse_tolerates_whitespace_around_entries() {
    let got = parse_partial("  1-1A , 2-3B  ,4-8A ").unwrap();
    assert_eq!(got.len(), 3);
}

#[test]
fn parse_tolerates_whitespace_inside_entries() {
    let got = parse_partial("1 - 1 A, 2 - 3 B").unwrap();
    assert_eq!(got.len(), 2);
}

#[test]
fn parse_empty_string_returns_empty_list() {
    let got = parse_partial("").unwrap();
    assert!(got.is_empty());
}

#[test]
fn parse_whitespace_only_returns_empty_list() {
    let got = parse_partial("   ,  ,  ").unwrap();
    assert!(got.is_empty());
}

// ---------------------------------------------------------------------------
// Duplicate rejection
// ---------------------------------------------------------------------------

#[test]
fn parse_rejects_exact_duplicate() {
    let err = parse_partial("1-1A,1-1A").unwrap_err();
    match err {
        Td3Error::BankAddressDuplicate(s) => assert_eq!(s, "G1P1A"),
        other => panic!("wrong error variant: {:?}", other),
    }
}

#[test]
fn parse_rejects_case_different_duplicate() {
    let err = parse_partial("1-1A,1-1a").unwrap_err();
    assert!(matches!(err, Td3Error::BankAddressDuplicate(_)));
}

// ---------------------------------------------------------------------------
// Invalid syntax / ranges
// ---------------------------------------------------------------------------

fn assert_invalid(input: &str) {
    let err = parse_partial(input).unwrap_err();
    assert!(
        matches!(err, Td3Error::BankAddressInvalid(_)),
        "input {:?} should be invalid, got: {:?}",
        input,
        err
    );
}

#[test]
fn parse_rejects_group_out_of_range() {
    assert_invalid("0-1A"); // group 0 is not valid (1..=4)
    assert_invalid("5-1A"); // group 5 is not valid
    assert_invalid("9-1A");
}

#[test]
fn parse_rejects_slot_out_of_range() {
    assert_invalid("1-0A"); // slot 0 is not valid (1..=8)
    assert_invalid("1-9A"); // slot 9 is not valid
}

#[test]
fn parse_rejects_invalid_side_letter() {
    assert_invalid("1-1C");
    assert_invalid("1-1X");
    assert_invalid("1-1");
}

#[test]
fn parse_rejects_garbage_tokens() {
    assert_invalid("abc");
    assert_invalid("1-A");
    assert_invalid("-1A");
    assert_invalid("1--1A");
    assert_invalid("1-1AA");
    assert_invalid("11-1A"); // group two digits
    assert_invalid("1-11A"); // slot two digits
}

#[test]
fn parse_rejects_partial_csv_with_one_bad_entry() {
    // Valid first entry, second entry invalid - the whole parse must fail.
    let err = parse_partial("1-1A,bogus").unwrap_err();
    assert!(matches!(err, Td3Error::BankAddressInvalid(_)));
}

// ---------------------------------------------------------------------------
// BankAddress helpers & full_bank()
// ---------------------------------------------------------------------------

#[test]
fn bank_address_label_is_folder_name() {
    let a = BankAddress {
        group: 0,
        slot_addr: 0,
    };
    assert_eq!(a.label(), "G1P1A");

    let b = BankAddress {
        group: 1,
        slot_addr: 10,
    };
    assert_eq!(b.label(), "G2P3B");

    let c = BankAddress {
        group: 3,
        slot_addr: 15,
    };
    assert_eq!(c.label(), "G4P8B");
}

#[test]
fn full_bank_has_64_entries_in_file_order() {
    let all = full_bank();
    assert_eq!(all.len(), 64);
    for (idx, addr) in all.iter().enumerate() {
        assert_eq!(addr.group as usize, idx / 16, "pos {}", idx);
        assert_eq!(addr.slot_addr as usize, idx % 16, "pos {}", idx);
    }
}

#[test]
fn full_bank_contains_corner_addresses() {
    let all = full_bank();
    assert!(all.contains(&BankAddress {
        group: 0,
        slot_addr: 0
    }));
    assert!(all.contains(&BankAddress {
        group: 0,
        slot_addr: 15
    }));
    assert!(all.contains(&BankAddress {
        group: 3,
        slot_addr: 0
    }));
    assert!(all.contains(&BankAddress {
        group: 3,
        slot_addr: 15
    }));
}
