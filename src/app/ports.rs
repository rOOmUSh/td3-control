use crate::error::Td3Error;

/// Enumerate MIDI input and output ports to stdout.
pub(super) fn list_ports() -> Result<(), Td3Error> {
    let out_midi = midir::MidiOutput::new("")
        .map_err(|e| Td3Error::Midi(format!("failed to create MIDI output: {}", e)))?;
    let in_midi = midir::MidiInput::new("")
        .map_err(|e| Td3Error::Midi(format!("failed to create MIDI input: {}", e)))?;

    println!("MIDI Output Ports:");
    let out_ports = out_midi.ports();
    if out_ports.is_empty() {
        println!("  (none)");
    } else {
        for (i, port) in out_ports.iter().enumerate() {
            let name = out_midi
                .port_name(port)
                .unwrap_or_else(|_| "(unknown)".into());
            println!("  {}: {}", i, name);
        }
    }

    println!("\nMIDI Input Ports:");
    let in_ports = in_midi.ports();
    if in_ports.is_empty() {
        println!("  (none)");
    } else {
        for (i, port) in in_ports.iter().enumerate() {
            let name = in_midi
                .port_name(port)
                .unwrap_or_else(|_| "(unknown)".into());
            println!("  {}: {}", i, name);
        }
    }

    Ok(())
}
