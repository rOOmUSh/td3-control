//! Pure startup state decisions for the launcher.

use super::child_args::LauncherMidiChoice;
use super::device_options::{self, ConfigPortSelection};
use super::midi_probe::{self, PortListing};
use super::web_port::WebPortStatus;

pub(crate) fn load_port_listing() -> (PortListing, Option<String>) {
    match midi_probe::list_ports() {
        Ok(ports) => (ports, None),
        Err(error) => (PortListing::default(), Some(error)),
    }
}

pub(crate) fn configured_selection_message(
    input: &ConfigPortSelection,
    output: &ConfigPortSelection,
) -> Option<String> {
    let mut messages = Vec::new();
    if let Some(message) = input.message("input") {
        messages.push(message);
    }
    if let Some(message) = output.message("output") {
        messages.push(message);
    }
    if messages.is_empty() {
        None
    } else {
        Some(messages.join(" | "))
    }
}

pub(crate) fn current_midi_choice(
    ports: &PortListing,
    selected_input: &Option<String>,
    selected_output: &Option<String>,
) -> Result<LauncherMidiChoice, String> {
    match (selected_input, selected_output) {
        (None, None) => Ok(LauncherMidiChoice::EnvDefault),
        (Some(input), Some(output)) => {
            if !device_options::exact_name_present(&ports.inputs, input) {
                return Err(format!("selected MIDI input is not available: {}", input));
            }
            if !device_options::exact_name_present(&ports.outputs, output) {
                return Err(format!("selected MIDI output is not available: {}", output));
            }
            Ok(LauncherMidiChoice::exact_pair(
                input.clone(),
                output.clone(),
            ))
        }
        (Some(_), None) => Err("select a MIDI output port or use TD3_CONFIG.env".to_string()),
        (None, Some(_)) => Err("select a MIDI input port or use TD3_CONFIG.env".to_string()),
    }
}

pub(crate) fn can_start(
    web_port_status: &WebPortStatus,
    selected_input: &Option<String>,
    selected_output: &Option<String>,
) -> bool {
    web_port_status.is_available()
        && matches!(
            (selected_input, selected_output),
            (None, None) | (Some(_), Some(_))
        )
}

pub(crate) fn midi_selection_is_session_only(
    input: &Option<String>,
    output: &Option<String>,
) -> bool {
    match (input, output) {
        (Some(input), Some(output)) => input != output,
        _ => false,
    }
}
