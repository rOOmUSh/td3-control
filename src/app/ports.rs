use crate::error::Td3Error;
use crate::midi_ports;

/// Enumerate MIDI input and output ports to stdout.
pub(super) fn list_ports() -> Result<(), Td3Error> {
    let ports = midi_ports::list_port_names()?;

    println!("MIDI Output Ports:");
    if ports.outputs.is_empty() {
        println!("  (none)");
    } else {
        for (i, name) in ports.outputs.iter().enumerate() {
            println!("  {}: {}", i, name);
        }
    }

    println!("\nMIDI Input Ports:");
    if ports.inputs.is_empty() {
        println!("  (none)");
    } else {
        for (i, name) in ports.inputs.iter().enumerate() {
            println!("  {}: {}", i, name);
        }
    }

    Ok(())
}
