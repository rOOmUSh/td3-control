//! Non-destructive TD-3 port enumeration for the launcher status row.
//!
//! Lists device names without opening a MIDI connection on the matched port.

use crate::midi_ports;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PortListing {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeStatus {
    Connected { port_name: String },
    NotFound,
    DriverError(String),
}

pub fn probe(substring: &str, strict: bool) -> ProbeStatus {
    let ports = match midi_ports::list_input_names() {
        Ok(ports) => ports,
        Err(error) => return ProbeStatus::DriverError(error.to_string()),
    };
    for name in ports {
        if matches(&name, substring, strict) {
            return ProbeStatus::Connected { port_name: name };
        }
    }
    ProbeStatus::NotFound
}

pub fn list_ports() -> Result<PortListing, String> {
    let ports = midi_ports::list_port_names().map_err(|error| error.to_string())?;
    Ok(PortListing {
        inputs: ports.inputs,
        outputs: ports.outputs,
    })
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
