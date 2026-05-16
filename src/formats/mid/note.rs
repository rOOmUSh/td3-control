use crate::error::Td3Error;
use crate::step;

use super::defaults::TD3_MIDI_BASE_PITCH;
use super::options::MidiExportOptions;

pub(super) fn midi_note_number(step: &step::Step, octave_offset: i8) -> Result<u8, Td3Error> {
    let td3_pitch =
        TD3_MIDI_BASE_PITCH + step.note as i16 + step.transpose.pitch_base_offset() as i16;
    let midi_pitch = td3_pitch + octave_offset as i16;
    if !(0..=127).contains(&midi_pitch) {
        return Err(Td3Error::FormatError(format!(
            "midi note out of range after octave offset: {}",
            midi_pitch
        )));
    }
    Ok(midi_pitch as u8)
}

pub(super) fn velocity_for_step(step: &step::Step, options: &MidiExportOptions) -> u8 {
    if step.accent == step::Accent::On {
        options.accent_velocity
    } else {
        options.normal_velocity
    }
}
