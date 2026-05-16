//! Tests for the single-instance guards.
//!
//! Exit-code contract:
//!   - `Td3Error::InstanceRunning` → exit 2 (control UI port already bound)
//!   - `Td3Error::DeviceBusy`      → exit 3 (MIDI port held by another process)
//!
//! These tests pin the message formatting and the helper that classifies a
//! post-discovery `midir::connect()` failure as a busy-port condition. The
//! real port/bind collisions are exercised by running two servers at once,
//! which requires a physical TD-3 and is out of scope for unit tests - we
//! validate the error-shaping here, and `main.rs` wires the variants to the
//! exit codes.

use crate::error::Td3Error;
use crate::midi_io::classify_connect_error;

// ── Td3Error::InstanceRunning - message formatting ──────────────────

#[test]
fn instance_running_message_includes_bind_and_port() {
    let err = Td3Error::InstanceRunning {
        bind: "127.0.0.1".to_string(),
        port: 3030,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("127.0.0.1:3030"),
        "missing bind:port in: {msg}"
    );
    assert!(msg.contains("TD-3 control UI"), "missing product in: {msg}");
    assert!(msg.contains("WEB_PORT"), "missing config hint in: {msg}");
}

#[test]
fn instance_running_mentions_td3_config_env() {
    // The message must point users at the correct config file, not a generic
    // "change the port" suggestion. If this drifts, users stop finding the
    // file that actually controls the port.
    let err = Td3Error::InstanceRunning {
        bind: "0.0.0.0".to_string(),
        port: 8080,
    };
    assert!(err.to_string().contains("TD3_CONFIG.env"));
}

// ── Td3Error::DeviceBusy - message formatting ───────────────────────

#[test]
fn device_busy_preserves_driver_error() {
    // Users need the original driver text to diagnose busy-vs-other issues
    // (e.g. "access denied" vs "resource busy"). The classifier must not
    // swallow it.
    let err = Td3Error::DeviceBusy {
        driver_error: "port 'TD-3 MIDI In' could not be opened [MIDI input]".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("could not be opened"),
        "lost driver text: {msg}"
    );
    assert!(msg.contains("device busy"));
    assert!(msg.contains("td3-control"));
}

// ── classify_connect_error ──────────────────────────────────────────

#[test]
fn classify_connect_error_returns_device_busy() {
    // Any post-discovery connect failure is a busy port because `find_port`
    // already proved the port exists. OS-specific string matching is
    // intentionally absent - we want every driver to map to exit 3.
    let err = classify_connect_error("MIDI input", "port in use");
    assert!(matches!(err, Td3Error::DeviceBusy { .. }));
}

#[test]
fn classify_connect_error_tags_operation() {
    // The operation string ("MIDI input" / "MIDI output") helps users
    // identify which direction failed when both fail on the same run.
    let err = classify_connect_error("MIDI output", "access denied");
    let msg = err.to_string();
    assert!(msg.contains("MIDI output"), "missing operation tag: {msg}");
    assert!(
        msg.contains("access denied"),
        "missing driver detail: {msg}"
    );
}

#[test]
fn classify_connect_error_accepts_display_types() {
    // The helper is generic over Display - callers pass `midir::ConnectError`
    // directly, which Display-formats itself. Smoke-check with a non-String
    // error to keep the bound from silently narrowing to &str.
    struct FakeDriverErr;
    impl std::fmt::Display for FakeDriverErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "fake driver failure")
        }
    }
    let err = classify_connect_error("MIDI input", FakeDriverErr);
    assert!(err.to_string().contains("fake driver failure"));
}
