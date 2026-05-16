pub(crate) const ORDER_META: u8 = 0;
pub(crate) const ORDER_NOTE_OFF: u8 = 20;
pub(crate) const ORDER_NOTE_ON: u8 = 30;
pub(crate) const ORDER_SLIDE_NOTE_ON: u8 = 40;
pub(crate) const ORDER_SLIDE_NOTE_OFF: u8 = 41;
pub(crate) const ORDER_END_OF_TRACK: u8 = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TimedMidiEvent {
    pub tick: u32,
    pub order: u8,
    pub data: Vec<u8>,
}

pub(crate) fn encode_vlq(mut value: u32) -> Vec<u8> {
    let mut buffer = [0u8; 5];
    let mut idx = 4usize;
    buffer[idx] = (value & 0x7F) as u8;
    value >>= 7;

    while value > 0 {
        idx -= 1;
        buffer[idx] = ((value & 0x7F) as u8) | 0x80;
        value >>= 7;
    }

    buffer[idx..].to_vec()
}

pub(crate) fn track_name_meta_event(name: &str) -> Vec<u8> {
    let name_bytes = name.as_bytes();
    let mut out = vec![0xFF, 0x03];
    out.extend_from_slice(&encode_vlq(name_bytes.len() as u32));
    out.extend_from_slice(name_bytes);
    out
}

pub(crate) fn tempo_meta_event(bpm: u32) -> Vec<u8> {
    let mpqn = 60_000_000u32 / bpm;
    vec![
        0xFF,
        0x51,
        0x03,
        ((mpqn >> 16) & 0xFF) as u8,
        ((mpqn >> 8) & 0xFF) as u8,
        (mpqn & 0xFF) as u8,
    ]
}

pub(crate) fn time_signature_meta_event() -> Vec<u8> {
    vec![0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08]
}

pub(crate) fn note_on_event(channel: u8, note: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 | ((channel - 1) & 0x0F), note, velocity]
}

pub(crate) fn note_off_event(channel: u8, note: u8) -> Vec<u8> {
    vec![0x80 | ((channel - 1) & 0x0F), note, 0]
}
