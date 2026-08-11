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
    use std::sync::mpsc;
    use std::time::Duration;

    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Devices::Midi::{MidiInPort, MidiOutPort};

    /// How long the Windows device query is given to answer.
    ///
    /// The query normally returns in milliseconds. A degraded MIDI
    /// service can take minutes: measured at `265` seconds to enumerate
    /// two devices on a machine whose service had wedged. `join()` waits
    /// forever, so every path that lists ports inherited that, including
    /// the launcher's startup, which then showed no window and no
    /// message. A caller is better served by a prompt error it can
    /// report than by a wait with no end.
    const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

    #[derive(Clone, Copy)]
    enum PortKind {
        Input,
        Output,
    }

    impl PortKind {
        fn label(self) -> &'static str {
            match self {
                PortKind::Input => "input",
                PortKind::Output => "output",
            }
        }
    }

    pub fn list_input_names() -> Result<Vec<String>, String> {
        list_device_names(PortKind::Input)
    }

    pub fn list_output_names() -> Result<Vec<String>, String> {
        list_device_names(PortKind::Output)
    }

    /// Enumerate on a worker thread so the wait can be bounded.
    ///
    /// The WinRT handles are not `Send`, so the whole query runs inside
    /// the thread and only the finished names come back. A thread whose
    /// query never returns is left behind rather than joined: it holds
    /// nothing the caller needs, and blocking to reap it would restore
    /// the very wait this exists to avoid.
    fn list_device_names(kind: PortKind) -> Result<Vec<String>, String> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(query_device_names(kind));
        });

        match rx.recv_timeout(QUERY_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "the Windows MIDI service did not answer a {} device query within {} s. \
                 Unplug and replug the device, or restart Windows if it persists",
                kind.label(),
                QUERY_TIMEOUT.as_secs()
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(format!("the {} device query thread stopped", kind.label()))
            }
        }
    }

    fn query_device_names(kind: PortKind) -> Result<Vec<String>, String> {
        let selector = match kind {
            PortKind::Input => MidiInPort::GetDeviceSelector(),
            PortKind::Output => MidiOutPort::GetDeviceSelector(),
        }
        .map_err(|error| format!("WinRT MIDI {} selector: {}", kind.label(), error))?;

        let collection = DeviceInformation::FindAllAsyncAqsFilter(&selector)
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
    /// A host with no MIDI subsystem is reported as a host with no MIDI
    /// ports.
    ///
    /// On Linux `midir` fails to initialise at all when ALSA is absent,
    /// which is the normal state of a headless machine or a container.
    /// Propagating that as an error takes down every caller: the pre-UI
    /// backup refuses to fall back to offline mode and the web UI will
    /// not start, on a machine that simply has no MIDI hardware. An
    /// empty list is what every caller already knows how to handle, and
    /// it is the truth: there are no ports here.
    pub fn list_input_names() -> Result<Vec<String>, String> {
        let Ok(input) = midir::MidiInput::new("td3-input-list") else {
            return Ok(Vec::new());
        };
        Ok(input
            .ports()
            .iter()
            .filter_map(|port| input.port_name(port).ok())
            .collect())
    }

    pub fn list_output_names() -> Result<Vec<String>, String> {
        let Ok(output) = midir::MidiOutput::new("td3-output-list") else {
            return Ok(Vec::new());
        };
        Ok(output
            .ports()
            .iter()
            .filter_map(|port| output.port_name(port).ok())
            .collect())
    }
}
