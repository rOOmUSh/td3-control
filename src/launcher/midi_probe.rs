//! Non-destructive TD-3 port enumeration for the launcher status row.
//!
//! Opens an unconnected `MidiInput` long enough to list port names, then
//! drops it. Does NOT open a connection on the matched port, so a parallel
//! `cargo run` instance or a DAW holding the device is not disturbed.

use midir::MidiInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeStatus {
    Connected { port_name: String },
    NotFound,
    DriverError(String),
}

pub fn probe(substring: &str, strict: bool) -> ProbeStatus {
    let input = match MidiInput::new("td3-launcher-probe") {
        Ok(i) => i,
        Err(e) => return ProbeStatus::DriverError(e.to_string()),
    };
    let ports = input.ports();
    for port in &ports {
        let Ok(name) = input.port_name(port) else {
            continue;
        };
        if matches(&name, substring, strict) {
            return ProbeStatus::Connected { port_name: name };
        }
    }
    ProbeStatus::NotFound
}

pub fn matches(port_name: &str, substring: &str, strict: bool) -> bool {
    if substring.is_empty() {
        return false;
    }
    if strict {
        port_name == substring
    } else {
        port_name.to_lowercase().contains(&substring.to_lowercase())
    }
}
