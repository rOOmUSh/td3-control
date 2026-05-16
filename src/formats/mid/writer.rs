use crate::error::Td3Error;
use crate::pattern::Pattern;

use super::events::{encode_vlq, TimedMidiEvent};
use super::options::MidiExportOptions;
use super::timeline::build_timeline;

pub fn export(
    pattern: &Pattern,
    address: &str,
    options: &MidiExportOptions,
) -> Result<Vec<u8>, Td3Error> {
    options.validate()?;

    let mut events = build_timeline(pattern, address, options)?;
    events.sort_by(|a, b| a.tick.cmp(&b.tick).then_with(|| a.order.cmp(&b.order)));

    let track_data = build_track_data(&events);
    let mut smf = Vec::with_capacity(14 + 8 + track_data.len());
    write_header_chunk(&mut smf, options.ppqn);
    write_track_chunk(&mut smf, &track_data);
    Ok(smf)
}

fn build_track_data(events: &[TimedMidiEvent]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut last_tick = 0u32;

    for event in events {
        let delta = event.tick - last_tick;
        out.extend_from_slice(&encode_vlq(delta));
        out.extend_from_slice(&event.data);
        last_tick = event.tick;
    }

    out
}

fn write_header_chunk(out: &mut Vec<u8>, ppqn: u16) {
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&ppqn.to_be_bytes());
}

fn write_track_chunk(out: &mut Vec<u8>, track_data: &[u8]) {
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
    out.extend_from_slice(track_data);
}
