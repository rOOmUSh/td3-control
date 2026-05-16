use std::sync::mpsc;
use std::time::Duration;

use crate::error::Td3Error;
use crate::midi_io::{drain_stale, is_valid_td3_sysex, receive_response, SYSEX_HEADER};

/// Build a valid TD-3 SysEx message with the given payload bytes.
fn make_td3_sysex(payload: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(SYSEX_HEADER.len() + payload.len() + 1);
    msg.extend_from_slice(SYSEX_HEADER);
    msg.extend_from_slice(payload);
    msg.push(0xF7);
    msg
}

// ── is_valid_td3_sysex ──────────────────────────────────────────────

#[test]
fn valid_td3_sysex_accepted() {
    let msg = make_td3_sysex(&[0x07, b'T', b'D', b'-', b'3', 0x00]);
    assert!(is_valid_td3_sysex(&msg));
}

#[test]
fn rejects_too_short() {
    // Just the header + F7, no payload byte
    let mut msg = SYSEX_HEADER.to_vec();
    msg.push(0xF7);
    assert!(!is_valid_td3_sysex(&msg));
}

#[test]
fn rejects_wrong_manufacturer() {
    let msg = vec![0xF0, 0x00, 0x00, 0x00, 0x00, 0x01, 0x0A, 0x07, 0xF7];
    assert!(!is_valid_td3_sysex(&msg));
}

#[test]
fn rejects_missing_terminator() {
    let mut msg = SYSEX_HEADER.to_vec();
    msg.extend_from_slice(&[0x07, 0x00]);
    // No F7
    assert!(!is_valid_td3_sysex(&msg));
}

#[test]
fn rejects_non_sysex() {
    // Note On message
    assert!(!is_valid_td3_sysex(&[0x90, 0x3C, 0x7F]));
}

#[test]
fn rejects_empty() {
    assert!(!is_valid_td3_sysex(&[]));
}

// ── drain_stale ─────────────────────────────────────────────────────

#[test]
fn drain_empty_channel() {
    let (_tx, rx) = mpsc::channel::<Vec<u8>>();
    assert_eq!(drain_stale(&rx), 0);
}

#[test]
fn drain_multiple_messages() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    tx.send(vec![0x90, 0x3C, 0x7F]).unwrap();
    tx.send(vec![0xFE]).unwrap();
    tx.send(make_td3_sysex(&[0x07])).unwrap();
    assert_eq!(drain_stale(&rx), 3);
    // Channel is now empty
    assert_eq!(drain_stale(&rx), 0);
}

// ── receive_response ────────────────────────────────────────────────

const SHORT_TIMEOUT: Duration = Duration::from_millis(100);

#[test]
fn accepts_matching_response() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let expected_payload = vec![0x07, b'T', b'D', b'-', b'3', 0x00];
    tx.send(make_td3_sysex(&expected_payload)).unwrap();

    let result = receive_response(&rx, "product name", Some(0x07), SHORT_TIMEOUT);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), expected_payload);
}

#[test]
fn accepts_any_when_no_expected_cmd() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let payload = vec![0x99, 0x01, 0x02];
    tx.send(make_td3_sysex(&payload)).unwrap();

    let result = receive_response(&rx, "test", None, SHORT_TIMEOUT);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}

#[test]
fn skips_non_sysex_then_accepts_valid() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // Note On (non-sysex)
    tx.send(vec![0x90, 0x3C, 0x7F]).unwrap();
    // Program Change
    tx.send(vec![0xC0, 0x05]).unwrap();
    // Valid TD-3 response
    let payload = vec![0x07, b'T', b'D', b'-', b'3', 0x00];
    tx.send(make_td3_sysex(&payload)).unwrap();

    let result = receive_response(&rx, "product name", Some(0x07), SHORT_TIMEOUT);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}

#[test]
fn skips_wrong_manufacturer_sysex() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // SysEx from a different manufacturer
    tx.send(vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]).unwrap();
    // Valid TD-3 response
    let payload = vec![0x09, 0x00, 0x01, 0x03, 0x07];
    tx.send(make_td3_sysex(&payload)).unwrap();

    let result = receive_response(&rx, "firmware version", Some(0x09), SHORT_TIMEOUT);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}

#[test]
fn skips_wrong_response_type_then_accepts_correct() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // Valid TD-3 sysex but wrong command type (0x09 firmware instead of 0x07 product name)
    tx.send(make_td3_sysex(&[0x09, 0x00, 0x01, 0x03])).unwrap();
    // Correct response type
    let payload = vec![0x07, b'T', b'D', b'-', b'3', 0x00];
    tx.send(make_td3_sysex(&payload)).unwrap();

    let result = receive_response(&rx, "product name", Some(0x07), SHORT_TIMEOUT);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}

#[test]
fn timeout_when_no_valid_response() {
    let (_tx, rx) = mpsc::channel::<Vec<u8>>();
    let result = receive_response(&rx, "test op", Some(0x07), SHORT_TIMEOUT);
    assert!(result.is_err());
    let err = result.unwrap_err();
    match &err {
        Td3Error::Timeout { operation } => assert_eq!(operation, "test op"),
        other => panic!("expected Timeout, got: {}", other),
    }
}

#[test]
fn timeout_when_only_wrong_types() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // Send several wrong-type TD-3 messages
    tx.send(make_td3_sysex(&[0x09, 0x00])).unwrap();
    tx.send(make_td3_sysex(&[0x78, 0x00])).unwrap();
    // No 0x07 response ever arrives

    let result = receive_response(&rx, "product name", Some(0x07), SHORT_TIMEOUT);
    assert!(result.is_err());
    match result.unwrap_err() {
        Td3Error::Timeout { .. } => {}
        other => panic!("expected Timeout, got: {}", other),
    }
}

#[test]
fn channel_closed_error() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    drop(tx); // Close the sender

    let result = receive_response(&rx, "test", Some(0x07), SHORT_TIMEOUT);
    assert!(result.is_err());
    match result.unwrap_err() {
        Td3Error::SysexResponse(msg) => assert!(msg.contains("channel closed")),
        other => panic!("expected SysexResponse, got: {}", other),
    }
}

#[test]
fn skips_truncated_sysex_with_correct_header() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // TD-3 header but no payload byte before F7 (too short)
    let mut truncated = SYSEX_HEADER.to_vec();
    truncated.push(0xF7);
    tx.send(truncated).unwrap();
    // Then a valid response
    let payload = vec![0x07, b'T', b'D', b'-', b'3', 0x00];
    tx.send(make_td3_sysex(&payload)).unwrap();

    let result = receive_response(&rx, "product name", Some(0x07), SHORT_TIMEOUT);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}

#[test]
fn mixed_garbage_before_valid_response() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // Active sensing (single byte)
    tx.send(vec![0xFE]).unwrap();
    // Clock tick
    tx.send(vec![0xF8]).unwrap();
    // Note On
    tx.send(vec![0x90, 0x3C, 0x7F]).unwrap();
    // SysEx from another manufacturer
    tx.send(vec![0xF0, 0x41, 0x10, 0x42, 0x12, 0xF7]).unwrap();
    // TD-3 sysex but wrong response type
    tx.send(make_td3_sysex(&[0x09, 0x00, 0x01])).unwrap();
    // Finally: correct response
    let payload = vec![0x78, 0x00, 0x00, 0x00, 0x01]; // pattern dump header
    tx.send(make_td3_sysex(&payload)).unwrap();

    let result = receive_response(&rx, "pattern download", Some(0x78), Duration::from_secs(1));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}

// ── drain + receive integration ─────────────────────────────────────

#[test]
fn stale_messages_drained_before_fresh_response() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // Stale messages from a previous request
    tx.send(make_td3_sysex(&[0x07, b'T', b'D', b'-', b'3', 0x00]))
        .unwrap();
    tx.send(make_td3_sysex(&[0x09, 0x00, 0x01, 0x03])).unwrap();

    // Drain the stale messages
    let drained = drain_stale(&rx);
    assert_eq!(drained, 2);

    // Now the "real" response arrives after drain
    let payload = vec![0x78, 0x00, 0x01];
    tx.send(make_td3_sysex(&payload)).unwrap();

    let result = receive_response(&rx, "pattern", Some(0x78), SHORT_TIMEOUT);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}
