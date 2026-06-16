use crate::error::Td3Error;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MidiPortListing {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

pub fn list_port_names() -> Result<MidiPortListing, Td3Error> {
    Ok(MidiPortListing {
        inputs: list_input_names()?,
        outputs: list_output_names()?,
    })
}

pub fn list_input_names() -> Result<Vec<String>, Td3Error> {
    platform::list_input_names()
        .map(clean_names)
        .map_err(|error| Td3Error::Midi(format!("failed to list MIDI input ports: {}", error)))
}

pub fn list_output_names() -> Result<Vec<String>, Td3Error> {
    platform::list_output_names()
        .map(clean_names)
        .map_err(|error| Td3Error::Midi(format!("failed to list MIDI output ports: {}", error)))
}

pub(crate) fn clean_names(mut names: Vec<String>) -> Vec<String> {
    names.retain(|name| !name.trim().is_empty());
    names.sort();
    names.dedup();
    names
}

#[cfg(windows)]
mod platform {
    use windows::core::HSTRING;
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Devices::Midi::{MidiInPort, MidiOutPort};

    pub fn list_input_names() -> Result<Vec<String>, String> {
        let selector = MidiInPort::GetDeviceSelector()
            .map_err(|error| format!("WinRT MIDI input selector: {}", error))?;
        list_device_names(&selector)
    }

    pub fn list_output_names() -> Result<Vec<String>, String> {
        let selector = MidiOutPort::GetDeviceSelector()
            .map_err(|error| format!("WinRT MIDI output selector: {}", error))?;
        list_device_names(&selector)
    }

    fn list_device_names(selector: &HSTRING) -> Result<Vec<String>, String> {
        let collection = DeviceInformation::FindAllAsyncAqsFilter(selector)
            .map_err(|error| format!("WinRT MIDI device query: {}", error))?
            .join()
            .map_err(|error| format!("WinRT MIDI device query wait: {}", error))?;
        let mut names = Vec::new();
        for device_info in collection.into_iter() {
            let name = device_info
                .Name()
                .map_err(|error| format!("WinRT MIDI device name: {}", error))?;
            names.push(name.to_string());
        }
        Ok(names)
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn list_input_names() -> Result<Vec<String>, String> {
        let input = midir::MidiInput::new("td3-input-list").map_err(|error| error.to_string())?;
        Ok(input
            .ports()
            .iter()
            .filter_map(|port| input.port_name(port).ok())
            .collect())
    }

    pub fn list_output_names() -> Result<Vec<String>, String> {
        let output =
            midir::MidiOutput::new("td3-output-list").map_err(|error| error.to_string())?;
        Ok(output
            .ports()
            .iter()
            .filter_map(|port| output.port_name(port).ok())
            .collect())
    }
}
