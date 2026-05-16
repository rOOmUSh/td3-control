use std::sync::mpsc;
use std::time::Duration;

use crate::error::Td3Error;
use crate::midi_io::SysexSender;
use crate::td3_protocol::{read_sync_source, set_sync_source, SyncSource};

const TIMEOUT: Duration = Duration::from_millis(200);
const SYSEX_HEADER: &[u8] = &[0xF0, 0x00, 0x20, 0x32, 0x00, 0x01, 0x0A];

fn wrap(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(SYSEX_HEADER.len() + payload.len() + 1);
    frame.extend_from_slice(SYSEX_HEADER);
    frame.extend_from_slice(payload);
    frame.push(0xF7);
    frame
}

/// Records sent frames and feeds a queued response into the rx channel
/// the next time `send_bytes` is called.
struct ScriptedSender {
    tx: mpsc::Sender<Vec<u8>>,
    sent: Vec<Vec<u8>>,
    next_response: Option<Vec<u8>>,
}

impl ScriptedSender {
    fn new(tx: mpsc::Sender<Vec<u8>>, response: Option<Vec<u8>>) -> Self {
        Self {
            tx,
            sent: Vec::new(),
            next_response: response,
        }
    }
}

impl SysexSender for ScriptedSender {
    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), Td3Error> {
        self.sent.push(bytes.to_vec());
        if let Some(resp) = self.next_response.take() {
            self.tx
                .send(resp)
                .map_err(|e| Td3Error::Midi(format!("test channel send failed: {}", e)))?;
        }
        Ok(())
    }
}

// ── SyncSource enum ────────────────────────────────────────────────

#[test]
fn from_byte_round_trip_all_valid() {
    for value in [
        SyncSource::Internal,
        SyncSource::MidiDin,
        SyncSource::MidiUsb,
        SyncSource::Trigger,
    ] {
        let byte = value.as_byte();
        let parsed = SyncSource::from_byte(byte).expect("valid byte must parse");
        assert_eq!(parsed, value);
    }
}

#[test]
fn from_byte_rejects_out_of_range() {
    for byte in 0x04u8..=0xFF {
        assert!(
            SyncSource::from_byte(byte).is_err(),
            "byte 0x{:02x} must be rejected",
            byte
        );
    }
}

#[test]
fn str_round_trip_all_valid() {
    for value in [
        SyncSource::Internal,
        SyncSource::MidiDin,
        SyncSource::MidiUsb,
        SyncSource::Trigger,
    ] {
        let s = value.as_str();
        let parsed = SyncSource::from_str(s).expect("valid str must parse");
        assert_eq!(parsed, value);
    }
}

#[test]
fn from_str_rejects_unknown() {
    assert!(SyncSource::from_str("midi").is_err());
    assert!(SyncSource::from_str("INT").is_err());
    assert!(SyncSource::from_str("").is_err());
}

#[test]
fn enum_byte_values_match_protocol() {
    assert_eq!(SyncSource::Internal.as_byte(), 0x00);
    assert_eq!(SyncSource::MidiDin.as_byte(), 0x01);
    assert_eq!(SyncSource::MidiUsb.as_byte(), 0x02);
    assert_eq!(SyncSource::Trigger.as_byte(), 0x03);
}

// ── set_sync_source ────────────────────────────────────────────────

#[test]
fn set_sync_source_emits_correct_sysex_and_accepts_ack() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let ack = wrap(&[0x01, 0x00, 0x00]);
    let mut sender = ScriptedSender::new(tx, Some(ack));

    set_sync_source(&mut sender, &rx, SyncSource::MidiUsb, TIMEOUT).expect("ack must be accepted");

    let expected_frame = wrap(&[0x1B, 0x02]);
    assert_eq!(sender.sent.len(), 1);
    assert_eq!(sender.sent[0], expected_frame);
}

#[test]
fn set_sync_source_emits_correct_byte_per_variant() {
    let cases = [
        (SyncSource::Internal, 0x00u8),
        (SyncSource::MidiDin, 0x01),
        (SyncSource::MidiUsb, 0x02),
        (SyncSource::Trigger, 0x03),
    ];
    for (variant, byte) in cases {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let ack = wrap(&[0x01, 0x00, 0x00]);
        let mut sender = ScriptedSender::new(tx, Some(ack));

        set_sync_source(&mut sender, &rx, variant, TIMEOUT).expect("ack must be accepted");

        let frame = &sender.sent[0];
        let payload_start = SYSEX_HEADER.len();
        assert_eq!(frame[payload_start], 0x1B);
        assert_eq!(
            frame[payload_start + 1],
            byte,
            "wrong wire byte for {:?}",
            variant
        );
    }
}

#[test]
fn set_sync_source_times_out_when_no_ack() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let mut sender = ScriptedSender::new(tx, None);

    let result = set_sync_source(
        &mut sender,
        &rx,
        SyncSource::MidiUsb,
        Duration::from_millis(50),
    );
    match result {
        Err(Td3Error::Timeout { .. }) => (),
        other => panic!("expected Timeout, got {:?}", other.err()),
    }
}

#[test]
fn set_sync_source_rejects_short_ack() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let short_ack = wrap(&[0x01, 0x00]);
    let mut sender = ScriptedSender::new(tx, Some(short_ack));

    let result = set_sync_source(&mut sender, &rx, SyncSource::MidiUsb, TIMEOUT);
    match result {
        Err(Td3Error::UploadFailed(msg)) => assert!(msg.contains("ACK length")),
        other => panic!("expected UploadFailed, got {:?}", other.err()),
    }
}

#[test]
fn set_sync_source_rejects_failure_status_ack() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let failed_ack = wrap(&[0x01, 0x01, 0x00]);
    let mut sender = ScriptedSender::new(tx, Some(failed_ack));

    let result = set_sync_source(&mut sender, &rx, SyncSource::MidiUsb, TIMEOUT);
    match result {
        Err(Td3Error::UploadFailed(msg)) => assert!(msg.contains("status bytes")),
        other => panic!("expected UploadFailed, got {:?}", other.err()),
    }
}

// ── read_sync_source ───────────────────────────────────────────────

fn config_response_with_clock(clock_source: u8) -> Vec<u8> {
    wrap(&[
        0x76,
        0x00,
        0x08,
        0x0C,
        0x02,
        0x02,
        0x00,
        0x01,
        0x02,
        clock_source,
        0x46,
    ])
}

#[test]
fn read_sync_source_parses_each_value() {
    for (byte, expected) in [
        (0x00u8, SyncSource::Internal),
        (0x01, SyncSource::MidiDin),
        (0x02, SyncSource::MidiUsb),
        (0x03, SyncSource::Trigger),
    ] {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let mut sender = ScriptedSender::new(tx, Some(config_response_with_clock(byte)));

        let result =
            read_sync_source(&mut sender, &rx, TIMEOUT).expect("valid response must parse");
        assert_eq!(result, expected, "wrong parse for byte 0x{:02x}", byte);
    }
}

#[test]
fn read_sync_source_emits_get_configuration_request() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let mut sender = ScriptedSender::new(tx, Some(config_response_with_clock(0x02)));

    read_sync_source(&mut sender, &rx, TIMEOUT).expect("valid response must parse");

    let expected_frame = wrap(&[0x75]);
    assert_eq!(sender.sent[0], expected_frame);
}

#[test]
fn read_sync_source_rejects_truncated_payload() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let truncated = wrap(&[0x76, 0x00, 0x08]);
    let mut sender = ScriptedSender::new(tx, Some(truncated));

    let result = read_sync_source(&mut sender, &rx, TIMEOUT);
    match result {
        Err(Td3Error::PayloadTooShort { .. }) => (),
        other => panic!("expected PayloadTooShort, got {:?}", other.err()),
    }
}

#[test]
fn read_sync_source_rejects_invalid_clock_byte() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let mut sender = ScriptedSender::new(tx, Some(config_response_with_clock(0x05)));

    let result = read_sync_source(&mut sender, &rx, TIMEOUT);
    match result {
        Err(Td3Error::SysexResponse(msg)) => assert!(msg.contains("invalid sync source")),
        other => panic!("expected SysexResponse, got {:?}", other.err()),
    }
}

#[test]
fn read_sync_source_times_out_when_no_response() {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let mut sender = ScriptedSender::new(tx, None);

    let result = read_sync_source(&mut sender, &rx, Duration::from_millis(50));
    match result {
        Err(Td3Error::Timeout { .. }) => (),
        other => panic!("expected Timeout, got {:?}", other.err()),
    }
}
