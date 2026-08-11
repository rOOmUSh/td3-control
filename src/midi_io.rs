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

fn port_name_matches(candidate_name: &str, query: &str, strict: bool) -> bool {
    if strict {
        candidate_name == query
    } else {
        candidate_name.contains(query)
    }
}

pub(crate) fn ensure_port_name_available(
    candidates: &[String],
    query: &str,
    strict: bool,
) -> Result<(), Td3Error> {
    if candidates
        .iter()
        .any(|candidate_name| port_name_matches(candidate_name, query, strict))
    {
        return Ok(());
    }

    Err(Td3Error::PortNotFound {
        port_name: query.to_owned(),
        available: candidates.join(", "),
    })
}

fn preflight_port_names(
    output_query: &str,
    input_query: &str,
    strict: bool,
) -> Result<(), Td3Error> {
    let ports = crate::midi_ports::list_port_names()?;
    let outputs = ports.outputs;
    let inputs = ports.inputs;
    ensure_port_name_available(&outputs, output_query, strict)?;
    ensure_port_name_available(&inputs, input_query, strict)?;
    Ok(())
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

/// Attempts to open the device before giving up, and the wait between
/// them.
///
/// A port another process is genuinely holding stays held, so retrying
/// costs a fraction of a second and changes nothing. What it covers is a
/// port still being released: the launcher enumerates MIDI devices
/// through WinRT, then spawns the control process and exits without
/// unwinding, and the driver can still refuse the child's open
/// milliseconds later.
/// Windows serialises MIDI access through a system service, and a
/// device query or open issued while another process is mid-operation
/// can be refused outright. A short retry absorbs that. It is
/// deliberately short: a device a program is actually holding stays
/// held, and waiting longer only delays a failure the user needs to
/// see.
const OPEN_RETRY_WINDOW: Duration = Duration::from_secs(3);
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(250);

/// Overrides the retry window, in milliseconds. `0` disables retrying,
/// which is what a diagnostic probe wants: it needs to report the
/// device's current state, not wait for it to change.
pub const OPEN_RETRY_ENV: &str = "TD3_MIDI_OPEN_RETRY_MS";

fn open_retry_window() -> Duration {
    match std::env::var(OPEN_RETRY_ENV) {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(ms) => Duration::from_millis(ms),
            Err(_) => OPEN_RETRY_WINDOW,
        },
        Err(_) => OPEN_RETRY_WINDOW,
    }
}
/// Say something once the wait is long enough to look like a hang.
const OPEN_RETRY_NOTICE_AFTER: Duration = Duration::from_millis(1_500);

/// An open device: the output connection, the receive channel fed by the
/// input callback, and the input connection that must outlive it.
pub type ConnectedPorts = (
    midir::MidiOutputConnection,
    std::sync::mpsc::Receiver<Vec<u8>>,
    midir::MidiInputConnection<()>,
);

/// Find the ports and open both connections, retrying a busy device.
///
/// Every path that talks to the device goes through here, so a transient
/// refusal is absorbed once rather than per caller. Only the open is
/// retried: once the ports are open the caller's handshake owns the
/// failure, and a protocol error is not something another attempt at
/// opening would fix.
pub fn open_device_with_retry(
    output_port_name: &str,
    input_port_name: &str,
    strict: bool,
    input_client_name: &str,
    output_client_name: &str,
) -> Result<ConnectedPorts, Td3Error> {
    let started = Instant::now();
    let window = open_retry_window();
    let mut attempt = 1u32;
    let mut notice_deadline: Option<Instant> = None;
    loop {
        match open_device(
            output_port_name,
            input_port_name,
            strict,
            input_client_name,
            output_client_name,
        ) {
            Ok(connected) => {
                if attempt > 1 {
                    eprintln!(
                        "MIDI port opened on attempt {} after {} ms of the device refusing it.",
                        attempt,
                        started.elapsed().as_millis()
                    );
                }
                return Ok(connected);
            }
            Err(error) => {
                // A port that is missing or misnamed will still be
                // missing in 250 ms, so only a device the driver
                // reported busy is worth another attempt.
                if !matches!(error, Td3Error::DeviceBusy { .. }) {
                    return Err(error);
                }
                if started.elapsed() >= window {
                    // How long it stayed refused separates a device
                    // still being released from one another program is
                    // holding, which need different answers.
                    // midir discards the MMSYSERR code, so ask Windows
                    // directly what it objects to before giving up.
                    let raw = crate::midi_diagnostics::probe_input_open(input_port_name);
                    return Err(match error {
                        Td3Error::DeviceBusy { driver_error } => Td3Error::DeviceBusy {
                            driver_error: format!(
                                "{} (still refused after {} attempts over {} ms)
       {}",
                                driver_error,
                                attempt,
                                started.elapsed().as_millis(),
                                raw
                            ),
                        },
                        other => other,
                    });
                }
                if attempt == 1 {
                    notice_deadline = Some(Instant::now() + OPEN_RETRY_NOTICE_AFTER);
                }
                if notice_deadline.is_some_and(|at| Instant::now() >= at) {
                    notice_deadline = None;
                    eprintln!(
                        "Waiting for the TD-3 MIDI port to be released (up to {} s)...",
                        window.as_secs()
                    );
                }
                attempt += 1;
                std::thread::sleep(OPEN_RETRY_DELAY);
            }
        }
    }
}

fn open_device(
    output_port_name: &str,
    input_port_name: &str,
    strict: bool,
    input_client_name: &str,
    output_client_name: &str,
) -> Result<ConnectedPorts, Td3Error> {
    let (out_midi, out_port, in_midi, in_port) =
        open_ports(output_port_name, input_port_name, strict)?;

    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let in_conn = in_midi
        .connect(
            &in_port,
            input_client_name,
            move |_stamp, msg, _| {
                let _ = tx.send(msg.to_owned());
            },
            (),
        )
        .map_err(|e| classify_connect_error("MIDI input", e))?;

    let out_conn = out_midi
        .connect(&out_port, output_client_name)
        .map_err(|e| classify_connect_error("MIDI output", e))?;

    Ok((out_conn, rx, in_conn))
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
    preflight_port_names(output_query, input_query, strict)?;

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
    let _exchange_guard = crate::midi_exchange_lock::acquire(operation, timeout)?;
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
    let _exchange_guard = crate::midi_exchange_lock::acquire(operation, timeout)?;
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
