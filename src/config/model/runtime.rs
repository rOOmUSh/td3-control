use std::time::Duration;

use crate::app_env::AppEnv;
use crate::error::Td3Error;
use crate::formats::mid::{MidiExportOptions, MidiSlideMode};
use crate::formats::Format;

use super::address::ScratchPattern;

pub const DEFAULT_DEVICE_NAME: &str = "TD-3";

#[derive(Debug, Clone)]
pub struct MidiRuntime {
    pub input_port_name: String,
    pub output_port_name: String,
    pub request_timeout: Duration,
    pub strict_name_match: bool,
    pub retry_count: u32,
}

impl MidiRuntime {
    pub(super) fn from_env(env: &AppEnv) -> Self {
        Self {
            input_port_name: env.midi_port_substring.clone(),
            output_port_name: env.midi_port_substring.clone(),
            request_timeout: Duration::from_millis(env.midi_timeout_ms),
            strict_name_match: env.midi_strict_name_match,
            retry_count: env.midi_retries,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactPaths {
    pub input_path: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderProfile {
    pub requested_formats: Vec<Format>,
    pub bpm: u32,
    pub ppqn: u16,
    pub midi_channel: u8,
    pub octave_offset: i8,
    pub accent_velocity: u8,
    pub normal_velocity: u8,
    pub slide_mode: MidiSlideMode,
    pub loop_count: u32,
    pub bars: Option<u32>,
}

impl RenderProfile {
    pub(super) fn from_env(env: &AppEnv) -> Self {
        Self {
            requested_formats: Vec::new(),
            bpm: env.ui_default_bpm,
            ppqn: env.midi_export_ppqn,
            midi_channel: env.midi_export_channel,
            octave_offset: env.midi_export_octave_offset,
            accent_velocity: env.midi_export_accent_velocity,
            normal_velocity: env.midi_export_normal_velocity,
            slide_mode: env.midi_export_slide_mode,
            loop_count: env.midi_export_loop_count,
            bars: None,
        }
    }

    pub(super) fn to_midi_export_options(&self) -> MidiExportOptions {
        MidiExportOptions {
            bpm: self.bpm,
            ppqn: self.ppqn,
            channel: self.midi_channel,
            octave_offset: self.octave_offset,
            accent_velocity: self.accent_velocity,
            normal_velocity: self.normal_velocity,
            slide_mode: self.slide_mode,
            loop_count: self.loop_count,
        }
    }

    pub(super) fn validate(&self) -> Result<(), Td3Error> {
        self.to_midi_export_options().validate()?;
        if self.bars == Some(0) {
            return Err(Td3Error::CliError("bars must be >= 1".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct BankJob {
    pub overwrite_existing: bool,
    pub partial: Option<String>,
    pub include_silent: bool,
    pub backup_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ControlRuntime {
    pub bind_address: String,
    pub listen_port: u16,
    pub scratch_slot: Option<ScratchPattern>,
    pub backup_dir: Option<String>,
}

impl ControlRuntime {
    pub(super) fn from_env(env: &AppEnv) -> Self {
        Self {
            bind_address: env.web_bind.clone(),
            listen_port: env.web_port,
            scratch_slot: None,
            backup_dir: None,
        }
    }
}
