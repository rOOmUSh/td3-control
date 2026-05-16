use std::sync::mpsc::Receiver;
use std::time::Duration;

use crate::error::Td3Error;
use crate::midi_io;
use crate::td3_protocol::{self, SessionInfo, SyncSourceFailurePolicy};

pub struct Td3MidiSessionConfig<'a> {
    pub input_port_name: &'a str,
    pub output_port_name: &'a str,
    pub strict_name_match: bool,
    pub timeout: Duration,
    pub sync_source_policy: SyncSourceFailurePolicy,
}

pub struct EstablishedTd3MidiSession {
    pub out_conn: midir::MidiOutputConnection,
    pub rx: Receiver<Vec<u8>>,
    pub in_conn: midir::MidiInputConnection<()>,
    pub info: SessionInfo,
}

pub fn establish_td3_midi_session(
    config: Td3MidiSessionConfig<'_>,
) -> Result<EstablishedTd3MidiSession, Td3Error> {
    let (out_midi, out_port, in_midi, in_port) = midi_io::open_ports(
        config.output_port_name,
        config.input_port_name,
        config.strict_name_match,
    )?;

    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let in_conn = in_midi
        .connect(
            &in_port,
            "td3-web",
            move |_stamp, msg, _| {
                let _ = tx.send(msg.to_owned());
            },
            (),
        )
        .map_err(|e| midi_io::classify_connect_error("MIDI input", e))?;

    let mut out_conn = out_midi
        .connect(&out_port, "")
        .map_err(|e| midi_io::classify_connect_error("MIDI output", e))?;

    let info = td3_protocol::establish_session(
        &mut out_conn,
        &rx,
        config.timeout,
        config.sync_source_policy,
    )?;

    Ok(EstablishedTd3MidiSession {
        out_conn,
        rx,
        in_conn,
        info,
    })
}
