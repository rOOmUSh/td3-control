use std::str::FromStr;

use crate::error::Td3Error;

use super::defaults::{
    default_bpm, DEFAULT_MIDI_ACCENT_VELOCITY, DEFAULT_MIDI_CHANNEL, DEFAULT_MIDI_LOOP_COUNT,
    DEFAULT_MIDI_NORMAL_VELOCITY, DEFAULT_MIDI_OCTAVE_OFFSET, DEFAULT_PPQN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiSlideMode {
    Td3,
    Generic,
    None,
}

impl FromStr for MidiSlideMode {
    type Err = Td3Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "td3" => Ok(MidiSlideMode::Td3),
            "generic" => Ok(MidiSlideMode::Generic),
            "none" => Ok(MidiSlideMode::None),
            _ => Err(Td3Error::FormatError(format!(
                "unknown midi slide mode '{}' (valid: td3, generic, none)",
                s
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiExportOptions {
    pub bpm: u32,
    pub ppqn: u16,
    pub channel: u8,
    pub octave_offset: i8,
    pub accent_velocity: u8,
    pub normal_velocity: u8,
    pub slide_mode: MidiSlideMode,
    pub loop_count: u32,
}

impl Default for MidiExportOptions {
    fn default() -> Self {
        Self {
            bpm: default_bpm(),
            ppqn: DEFAULT_PPQN,
            channel: DEFAULT_MIDI_CHANNEL,
            octave_offset: DEFAULT_MIDI_OCTAVE_OFFSET,
            accent_velocity: DEFAULT_MIDI_ACCENT_VELOCITY,
            normal_velocity: DEFAULT_MIDI_NORMAL_VELOCITY,
            slide_mode: MidiSlideMode::Td3,
            loop_count: DEFAULT_MIDI_LOOP_COUNT,
        }
    }
}

impl MidiExportOptions {
    pub fn from_env(env: &crate::app_env::AppEnv) -> Self {
        Self {
            bpm: env.ui_default_bpm,
            ppqn: env.midi_export_ppqn,
            channel: env.midi_export_channel,
            octave_offset: env.midi_export_octave_offset,
            accent_velocity: env.midi_export_accent_velocity,
            normal_velocity: env.midi_export_normal_velocity,
            slide_mode: env.midi_export_slide_mode,
            loop_count: env.midi_export_loop_count,
        }
    }

    pub fn validate(&self) -> Result<(), Td3Error> {
        if self.bpm == 0 {
            return Err(Td3Error::FormatError("bpm must be > 0".to_string()));
        }
        if self.ppqn == 0 {
            return Err(Td3Error::FormatError("ppqn must be > 0".to_string()));
        }
        if self.slide_mode == MidiSlideMode::Generic {
            return Err(Td3Error::FormatError(
                "generic slide mode is reserved but not implemented yet".to_string(),
            ));
        }
        if self.channel == 0 || self.channel > 16 {
            return Err(Td3Error::FormatError(format!(
                "midi channel must be 1..=16, got {}",
                self.channel
            )));
        }
        if self.accent_velocity == 0 || self.accent_velocity > 127 {
            return Err(Td3Error::FormatError(format!(
                "accent velocity must be 1..=127, got {}",
                self.accent_velocity
            )));
        }
        if self.normal_velocity == 0 || self.normal_velocity > 127 {
            return Err(Td3Error::FormatError(format!(
                "normal velocity must be 1..=127, got {}",
                self.normal_velocity
            )));
        }
        if self.loop_count == 0 {
            return Err(Td3Error::FormatError("loop count must be >= 1".to_string()));
        }
        Ok(())
    }
}
