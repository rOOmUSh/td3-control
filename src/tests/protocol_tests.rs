use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Duration;

use crate::error::Td3Error;
use crate::midi_io::{SysexSender, SYSEX_HEADER};
use crate::pattern::Pattern;
use crate::td3_protocol;

use super::fixtures;

struct QueueingSender {
    tx: mpsc::Sender<Vec<u8>>,
    responses: VecDeque<Vec<u8>>,
}

impl QueueingSender {
    fn new(tx: mpsc::Sender<Vec<u8>>, responses: Vec<Vec<u8>>) -> Self {
        Self {
            tx,
            responses: responses.into(),
        }
    }
}

impl SysexSender for QueueingSender {
    fn send_bytes(&mut self, _bytes: &[u8]) -> Result<(), Td3Error> {
        if let Some(response) = self.responses.pop_front() {
            self.tx
                .send(response)
                .map_err(|e| Td3Error::SysexResponse(format!("test send failed: {}", e)))?;
        }
        Ok(())
    }
}

struct BurstSender {
    tx: mpsc::Sender<Vec<u8>>,
    responses: Vec<Vec<u8>>,
    sent: bool,
}

impl BurstSender {
    fn new(tx: mpsc::Sender<Vec<u8>>, responses: Vec<Vec<u8>>) -> Self {
        Self {
            tx,
            responses,
            sent: false,
        }
    }
}

impl SysexSender for BurstSender {
    fn send_bytes(&mut self, _bytes: &[u8]) -> Result<(), Td3Error> {
        if self.sent {
            return Ok(());
        }
        self.sent = true;
        for response in self.responses.drain(..) {
            self.tx
                .send(response)
                .map_err(|e| Td3Error::SysexResponse(format!("test send failed: {}", e)))?;
        }
        Ok(())
    }
}

fn td3_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(SYSEX_HEADER.len() + payload.len() + 1);
    frame.extend_from_slice(SYSEX_HEADER);
    frame.extend_from_slice(payload);
    frame.push(0xF7);
    frame
}

fn product_name_response(name: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(name.len() + 2);
    payload.push(0x07);
    payload.extend_from_slice(name.as_bytes());
    payload.push(0x00);
    td3_frame(&payload)
}

fn firmware_response(version: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(version.len() + 2);
    payload.extend_from_slice(&[0x09, 0x00]);
    payload.extend_from_slice(version);
    td3_frame(&payload)
}

fn config_response(sync_source: u8) -> Vec<u8> {
    td3_frame(&[
        0x76,
        0x00,
        0x08,
        0x0C,
        0x02,
        0x02,
        0x00,
        0x01,
        0x02,
        sync_source,
        0x46,
    ])
}

// Tests for the protocol/app layer: error types, error conversions,
// and app-level import_file behavior.

#[test]
fn probe_timeout_on_no_device() {
    // probe_device needs a MidiOutputConnection which we can't mock directly,
    // but we verify the error type expected from timeout situations.
    let err = Td3Error::Timeout {
        operation: "product name".to_string(),
    };
    assert!(err.to_string().contains("product name"));
}

#[test]
fn probe_product_name_missing_null_returns_error() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(tx, vec![td3_frame(&[0x07, b'T', b'D', b'-', b'3'])]);

    let result = td3_protocol::probe_device(&mut sender, &rx, Duration::from_millis(100));
    let err = match result {
        Ok(_) => panic!("expected product name error"),
        Err(err) => err.to_string(),
    };
    assert!(
        err.contains("missing null terminator"),
        "expected null terminator error, got: {}",
        err
    );
}

#[test]
fn establish_session_success_reads_identity_firmware_and_sync_source() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(
        tx,
        vec![
            product_name_response("TD-3"),
            firmware_response(&[1, 2, 3]),
            config_response(0x01),
        ],
    );

    let session = td3_protocol::establish_session(
        &mut sender,
        &rx,
        Duration::from_millis(100),
        td3_protocol::SyncSourceFailurePolicy::ReturnError,
    )
    .expect("valid scripted responses must establish a session");

    assert_eq!(session.product_name, "TD-3");
    assert_eq!(session.firmware_version, "1.2.3");
    assert_eq!(session.sync_source, td3_protocol::SyncSource::MidiDin);
    assert!(session.sync_source_error.is_none());
}

#[test]
fn establish_session_wrong_device_returns_typed_error() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(tx, vec![product_name_response("TD-3 MO")]);

    let result = td3_protocol::establish_session(
        &mut sender,
        &rx,
        Duration::from_millis(100),
        td3_protocol::SyncSourceFailurePolicy::ReturnError,
    );

    match result {
        Err(Td3Error::DeviceMismatch { expected, actual }) => {
            assert_eq!(expected, "TD-3");
            assert_eq!(actual, "TD-3 MO");
        }
        other => panic!("expected DeviceMismatch, got {:?}", other.err()),
    }
}

#[test]
fn establish_session_timeout_returns_typed_error() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(tx, Vec::new());

    let result = td3_protocol::establish_session(
        &mut sender,
        &rx,
        Duration::from_millis(20),
        td3_protocol::SyncSourceFailurePolicy::ReturnError,
    );

    match result {
        Err(Td3Error::Timeout { operation }) => assert_eq!(operation, "product name"),
        other => panic!("expected Timeout, got {:?}", other.err()),
    }
}

#[test]
fn establish_session_sync_source_failure_can_default_to_usb() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(
        tx,
        vec![product_name_response("TD-3"), firmware_response(&[1, 2, 3])],
    );

    let session = td3_protocol::establish_session(
        &mut sender,
        &rx,
        Duration::from_millis(20),
        td3_protocol::SyncSourceFailurePolicy::DefaultToUsb,
    )
    .expect("sync-source fallback should preserve session setup");

    assert_eq!(session.sync_source, td3_protocol::SyncSource::MidiUsb);
    match session.sync_source_error {
        Some(Td3Error::Timeout { operation }) => assert_eq!(operation, "configuration"),
        other => panic!("expected sync-source timeout, got {:?}", other),
    }
}

#[test]
fn establish_session_sync_source_failure_can_return_error() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(
        tx,
        vec![product_name_response("TD-3"), firmware_response(&[1, 2, 3])],
    );

    let result = td3_protocol::establish_session(
        &mut sender,
        &rx,
        Duration::from_millis(20),
        td3_protocol::SyncSourceFailurePolicy::ReturnError,
    );

    match result {
        Err(Td3Error::Timeout { operation }) => assert_eq!(operation, "configuration"),
        other => panic!("expected sync-source timeout, got {:?}", other.err()),
    }
}

#[test]
fn download_pattern_skips_wrong_address_dump_before_correct_dump() {
    let (tx, rx) = mpsc::channel();
    let mut wrong = fixtures::simple_sysex();
    wrong[2] = 0x01;
    let correct = fixtures::simple_sysex();
    let mut sender = BurstSender::new(tx, vec![td3_frame(&wrong), td3_frame(&correct)]);

    let (raw, pattern) =
        td3_protocol::download_pattern(&mut sender, &rx, 0, 0, 0, Duration::from_millis(100))
            .expect("correct address dump must be accepted");

    assert_eq!(raw[1], 0);
    assert_eq!(raw[2], 0);
    assert_eq!(pattern.active_steps, 16);
}

#[test]
fn upload_pattern_rejects_ack_failure_status() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(tx, vec![td3_frame(&[0x01, 0x01, 0x00])]);

    let result = td3_protocol::upload_pattern(
        &mut sender,
        &rx,
        &Pattern::default(),
        0,
        0,
        0,
        Duration::from_millis(100),
    );

    match result {
        Err(Td3Error::UploadFailed(msg)) => assert!(msg.contains("status bytes")),
        other => panic!("expected UploadFailed, got {:?}", other.err()),
    }
}

#[test]
fn upload_raw_payload_rejects_ack_failure_status() {
    let (tx, rx) = mpsc::channel();
    let mut sender = QueueingSender::new(tx, vec![td3_frame(&[0x01, 0x00, 0x01])]);
    let payload = [0u8; 112];

    let result = td3_protocol::upload_raw_payload(
        &mut sender,
        &rx,
        0,
        0,
        &payload,
        Duration::from_millis(100),
    );

    match result {
        Err(Td3Error::UploadFailed(msg)) => assert!(msg.contains("status bytes")),
        other => panic!("expected UploadFailed, got {:?}", other.err()),
    }
}

#[test]
fn retry_timeout_without_retries_returns_typed_error() {
    let result: Result<(), Td3Error> = td3_protocol::with_retry(0, "retry test", || {
        Err(Td3Error::Timeout {
            operation: "inner".to_string(),
        })
    });

    match result.unwrap_err() {
        Td3Error::Timeout { operation } => assert_eq!(operation, "inner"),
        other => panic!("expected Timeout, got: {}", other),
    }
}

#[test]
fn device_mismatch_error_message() {
    let err = Td3Error::DeviceMismatch {
        expected: "TD-3".into(),
        actual: "TD-3 MO".into(),
    };
    assert!(err.to_string().contains("TD-3"));
    assert!(err.to_string().contains("TD-3 MO"));
}

#[test]
fn payload_too_short_error_message() {
    let err = Td3Error::PayloadTooShort {
        expected: 3,
        actual: 1,
    };
    assert!(err.to_string().contains("3"));
    assert!(err.to_string().contains("1"));
}

#[test]
fn io_error_converts_to_td3error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let td3_err: Td3Error = io_err.into();
    assert!(td3_err.to_string().contains("file not found"));
}

#[test]
fn utf8_error_converts_to_td3error() {
    // Create an invalid UTF-8 sequence at runtime to avoid the compile-time lint
    let bytes: Vec<u8> = vec![0xff, 0xfe];
    let utf8_err = std::str::from_utf8(&bytes).unwrap_err();
    let td3_err: Td3Error = utf8_err.into();
    assert!(td3_err.to_string().contains("UTF-8"));
}

// ── import_file tests (app layer) ──────────────────────────────────

#[test]
fn import_rejects_unknown_extension() {
    let result = crate::app::import_file(
        "pattern.midi",
        &crate::formats::mid_import::MidiImportOptions::default(),
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cannot detect format"));
}

#[test]
fn import_rejects_nonexistent_file() {
    let result = crate::app::import_file(
        "nonexistent.toml",
        &crate::formats::mid_import::MidiImportOptions::default(),
    );
    assert!(result.is_err());
    // Should be an IO error
    let err = result.unwrap_err();
    match err {
        Td3Error::Io(_) => {} // expected
        other => panic!("expected Io error, got: {}", other),
    }
}

// ── P8.5: App-layer failure integration tests ──────────────────────

#[test]
fn import_mid_missing_file_fails_with_io_error() {
    // .mid import is now supported, but a missing file should still surface
    // a clean I/O error before the parser ever runs.
    let result = crate::app::import_file(
        "pattern_does_not_exist.mid",
        &crate::formats::mid_import::MidiImportOptions::default(),
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Td3Error::Io(_)),
        "expected Io error, got: {}",
        err
    );
}

#[test]
fn import_rejects_extensionless_file() {
    let result = crate::app::import_file(
        "pattern",
        &crate::formats::mid_import::MidiImportOptions::default(),
    );
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("cannot detect format"));
}

#[test]
fn error_timeout_contains_operation_name() {
    let err = Td3Error::Timeout {
        operation: "firmware version".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("firmware version"));
    assert!(msg.contains("timeout"));
    assert!(
        msg.contains("connected"),
        "timeout should hint at checking connection: {}",
        msg
    );
}

#[test]
fn error_upload_failed_message() {
    let err =
        Td3Error::UploadFailed("unexpected ACK payload length: expected 3, got 5".to_string());
    let msg = err.to_string();
    assert!(msg.contains("upload failed"));
    assert!(msg.contains("expected 3"));
}

#[test]
fn error_port_not_found_lists_available() {
    let err = Td3Error::PortNotFound {
        port_name: "TD-3".to_string(),
        available: "MIDI Out 1, USB MIDI".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("TD-3"));
    assert!(msg.contains("MIDI Out 1"));
    assert!(msg.contains("USB MIDI"));
    assert!(
        msg.contains("list-ports"),
        "should suggest list-ports command: {}",
        msg
    );
}

#[test]
fn error_device_mismatch_suggests_flag() {
    let err = Td3Error::DeviceMismatch {
        expected: "TD-3".into(),
        actual: "TD-3 MO".into(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("strict-device-name"),
        "should mention override flag: {}",
        msg
    );
}

#[test]
fn error_wrong_message_id() {
    let err = Td3Error::WrongMessageId { actual: 0x77 };
    let msg = err.to_string();
    assert!(msg.contains("0x77"));
}

#[test]
fn error_invalid_active_steps() {
    let err = Td3Error::InvalidActiveSteps { value: 0 };
    let msg = err.to_string();
    assert!(msg.contains("0") && msg.contains("1..=16"));
}

#[test]
fn error_invalid_note() {
    let err = Td3Error::InvalidNote { step: 3, value: 99 };
    let msg = err.to_string();
    assert!(msg.contains("step 3"));
    assert!(msg.contains("99"));
}

#[test]
fn error_format_error_message() {
    let err = Td3Error::FormatError("bad field value".to_string());
    let msg = err.to_string();
    assert!(msg.contains("bad field value"));
}
