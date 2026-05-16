use std::time::{Duration, Instant};

use crate::error::Td3Error;

// ---------------------------------------------------------------------------
// SysEx framing constants
// ---------------------------------------------------------------------------

/// TD-3 SysEx header: F0 (start), 00 20 32 (manufacturer), 00 01 0A (device).
pub(crate) const SYSEX_HEADER: &[u8] = &[0xF0, 0x00, 0x20, 0x32, 0x00, 0x01, 0x0A];

/// Standard SysEx terminator byte.
const SYSEX_TERMINATOR: u8 = 0xF7;

// ---------------------------------------------------------------------------
// Port discovery
// ---------------------------------------------------------------------------

/// Find a MIDI port by name.
///
/// If `strict` is true, requires an exact match.
/// If `strict` is false (default), matches any port whose name contains
/// the query string.
fn find_port<T: midir::MidiIO>(
    handle: &T,
    candidates: &[T::Port],
    query: &str,
    strict: bool,
) -> Result<T::Port, Td3Error> {
    let is_match = |candidate_name: &str| -> bool {
        if strict {
            candidate_name == query
        } else {
            candidate_name.contains(query)
        }
    };

    for candidate in candidates {
        if let Ok(candidate_name) = handle.port_name(candidate) {
            if is_match(&candidate_name) {
                return Ok(candidate.clone());
            }
        }
    }

    let available = candidates
        .iter()
        .filter_map(|candidate| handle.port_name(candidate).ok())
        .collect::<Vec<String>>()
        .join(", ");

    Err(Td3Error::PortNotFound {
        port_name: query.to_owned(),
        available,
    })
}

/// Classify a `midir` connect error as a device-busy condition.
///
/// When `find_port` already succeeded, the port was present at the moment of
/// enumeration, so a subsequent `.connect()` failure almost always means another
/// process (commonly another `td3-control` instance) is holding the port open.
/// We surface that explicitly with `Td3Error::DeviceBusy` so `main.rs` can exit
/// with code 3 and the user gets the actionable message.
///
/// The `operation` string is used only if `classify_connect_error` is called
/// with something other than a post-discovery connect failure (so the returned
/// error still carries the original driver text).
pub fn classify_connect_error<E: std::fmt::Display>(operation: &str, err: E) -> Td3Error {
    let driver_error = format!("{} [{}]", err, operation);
    Td3Error::DeviceBusy { driver_error }
}

/// Open matched MIDI input and output ports for TD-3 communication.
pub fn open_ports(
    output_query: &str,
    input_query: &str,
    strict: bool,
) -> Result<
    (
        midir::MidiOutput,
        midir::MidiOutputPort,
        midir::MidiInput,
        midir::MidiInputPort,
    ),
    Td3Error,
> {
    // Output
    let output_handle = midir::MidiOutput::new("")
        .map_err(|error| Td3Error::Midi(format!("failed to create MIDI output: {}", error)))?;
    let output_candidates = output_handle.ports();
    let output_found = find_port(&output_handle, &output_candidates, output_query, strict)?;

    // Input
    let mut input_handle = midir::MidiInput::new("")
        .map_err(|error| Td3Error::Midi(format!("failed to create MIDI input: {}", error)))?;
    input_handle.ignore(midir::Ignore::TimeAndActiveSense);
    let input_candidates = input_handle.ports();
    let input_found = find_port(&input_handle, &input_candidates, input_query, strict)?;

    Ok((output_handle, output_found, input_handle, input_found))
}

// ---------------------------------------------------------------------------
// SysEx frame validation
// ---------------------------------------------------------------------------

/// Check if a raw MIDI message is a valid TD-3 SysEx frame.
/// Must start with SYSEX_HEADER, end with F7, and contain at least one
/// payload byte.
pub(crate) fn is_valid_td3_sysex(frame: &[u8]) -> bool {
    frame.len() >= SYSEX_HEADER.len() + 2
        && frame.starts_with(SYSEX_HEADER)
        && frame.last() == Some(&SYSEX_TERMINATOR)
}

// ---------------------------------------------------------------------------
// Channel utilities
// ---------------------------------------------------------------------------

/// Drain all queued messages from the receive channel.
/// Returns the number of messages discarded.
pub(crate) fn drain_stale(receiver: &std::sync::mpsc::Receiver<Vec<u8>>) -> usize {
    let mut discarded = 0;
    while receiver.try_recv().is_ok() {
        discarded += 1;
    }
    discarded
}

// ---------------------------------------------------------------------------
// Sender abstraction
// ---------------------------------------------------------------------------

/// Anything that can put a raw MIDI byte sequence on the wire. The concrete
/// implementations are:
///
/// - `midir::MidiOutputConnection` - sends directly (used when the transport
///   is idle and the session owns the port).
/// - `web::clock::ClockRunner` - queues the bytes to the dedicated clock
///   thread, which owns the port for the duration of playback. The thread
///   drains the queue between 0xF8 ticks so SysEx sends (e.g. progression
///   pattern swaps) can happen mid-play without tearing down the clock.
///
/// `exchange_sysex` and the typed protocol helpers in `td3_protocol` are generic
/// over this trait so both paths reuse the same request/response logic and
/// response-matching rules.
pub trait SysexSender {
    /// Transmit `bytes` exactly as given. Implementations are responsible
    /// for framing only at the transport level (nothing here adds headers
    /// or terminators - the caller builds a complete F0..F7 frame).
    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), Td3Error>;
}

impl SysexSender for midir::MidiOutputConnection {
    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), Td3Error> {
        self.send(bytes)
            .map_err(|e| Td3Error::Midi(format!("midi send failed: {}", e)))
    }
}

// ---------------------------------------------------------------------------
// Request / response transport
// ---------------------------------------------------------------------------

/// Wait for a TD-3 SysEx response matching the expected command byte.
///
/// Filters out:
/// - Non-SysEx messages (note on/off, CC, etc.)
/// - SysEx with wrong manufacturer header
/// - Valid TD-3 SysEx with wrong response command byte
///
/// Returns the inner payload (between SYSEX_HEADER and F7).
pub(crate) fn receive_response(
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    operation: &str,
    expected_cmd: Option<u8>,
    timeout: Duration,
) -> Result<Vec<u8>, Td3Error> {
    receive_response_matching(receiver, operation, expected_cmd, timeout, |_| Ok(true))
}

/// Wait for a TD-3 SysEx response that matches the command and caller-supplied
/// payload predicate.
pub(crate) fn receive_response_matching<F>(
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    operation: &str,
    expected_cmd: Option<u8>,
    timeout: Duration,
    mut matches_payload: F,
) -> Result<Vec<u8>, Td3Error>
where
    F: FnMut(&[u8]) -> Result<bool, Td3Error>,
{
    let deadline = Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Td3Error::Timeout {
                operation: operation.to_owned(),
            });
        }

        match receiver.recv_timeout(remaining) {
            Ok(incoming) => {
                if !is_valid_td3_sysex(&incoming) {
                    log::trace!(
                        "Skipping non-TD3 message ({}b, first byte: 0x{:02x})",
                        incoming.len(),
                        incoming.first().copied().unwrap_or(0)
                    );
                    continue;
                }

                let body = incoming[SYSEX_HEADER.len()..incoming.len() - 1].to_vec();

                if let Some(expected) = expected_cmd {
                    let actual_cmd = body[0];
                    if actual_cmd != expected {
                        log::debug!(
                            "Skipping response type 0x{:02x} (waiting for 0x{:02x} for {})",
                            actual_cmd,
                            expected,
                            operation
                        );
                        continue;
                    }
                }

                if !matches_payload(&body)? {
                    log::debug!(
                        "Skipping matched command for {} because payload did not match request",
                        operation
                    );
                    continue;
                }

                log::debug!(
                    "<< Response for {} ({}b): {:02x?}",
                    operation,
                    incoming.len(),
                    incoming
                );

                return Ok(body);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                return Err(Td3Error::Timeout {
                    operation: operation.to_owned(),
                });
            }
            Err(_) => {
                return Err(Td3Error::SysexResponse(format!(
                    "receive channel closed while waiting for {}",
                    operation
                )));
            }
        }
    }
}

/// Send a TD-3 SysEx request and wait for a matching response.
///
/// 1. Drains stale messages from the receive channel
/// 2. Sends the SysEx request
/// 3. Waits for a response matching `expected_cmd`
///
/// `expected_cmd`: the command byte expected at payload[0] of the response.
/// Pass `None` to accept any valid TD-3 SysEx (not recommended for production).
///
/// Generic over `SysexSender` so the same request/response logic works for
/// direct port sends (when idle) and clock-thread-queued sends (during
/// playback) - see the trait docs.
pub fn exchange_sysex<S: SysexSender + ?Sized>(
    sender: &mut S,
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    operation: &str,
    request_body: &[u8],
    expected_cmd: Option<u8>,
    timeout: Duration,
) -> Result<Vec<u8>, Td3Error> {
    let purged = drain_stale(receiver);
    if purged > 0 {
        log::debug!("Drained {} stale message(s) before {}", purged, operation);
    }

    // Build complete SysEx frame: midir requires a single send() with [F0 ... F7]
    let mut wire_frame = Vec::with_capacity(SYSEX_HEADER.len() + request_body.len() + 1);
    wire_frame.extend_from_slice(SYSEX_HEADER);
    wire_frame.extend_from_slice(request_body);
    wire_frame.push(SYSEX_TERMINATOR);

    log::debug!(">> Requesting {}, sysex = {:02x?}", operation, wire_frame);
    sender.send_bytes(&wire_frame).map_err(|e| match e {
        Td3Error::Midi(msg) => Td3Error::Midi(format!("{} for {}", msg, operation)),
        other => other,
    })?;

    receive_response(receiver, operation, expected_cmd, timeout)
}

/// Send a TD-3 SysEx request and wait for a response matching a payload
/// predicate in addition to the command byte.
pub fn exchange_sysex_matching<S, F>(
    sender: &mut S,
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    operation: &str,
    request_body: &[u8],
    expected_cmd: Option<u8>,
    timeout: Duration,
    matches_payload: F,
) -> Result<Vec<u8>, Td3Error>
where
    S: SysexSender + ?Sized,
    F: FnMut(&[u8]) -> Result<bool, Td3Error>,
{
    let purged = drain_stale(receiver);
    if purged > 0 {
        log::debug!("Drained {} stale message(s) before {}", purged, operation);
    }

    let mut wire_frame = Vec::with_capacity(SYSEX_HEADER.len() + request_body.len() + 1);
    wire_frame.extend_from_slice(SYSEX_HEADER);
    wire_frame.extend_from_slice(request_body);
    wire_frame.push(SYSEX_TERMINATOR);

    log::debug!(">> Requesting {}, sysex = {:02x?}", operation, wire_frame);
    sender.send_bytes(&wire_frame).map_err(|e| match e {
        Td3Error::Midi(msg) => Td3Error::Midi(format!("{} for {}", msg, operation)),
        other => other,
    })?;

    receive_response_matching(receiver, operation, expected_cmd, timeout, matches_payload)
}
