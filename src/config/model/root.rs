use crate::app_env::AppEnv;
use crate::error::Td3Error;
use crate::formats::mid::MidiExportOptions;

use super::{ArtifactPaths, BankJob, ControlRuntime, MidiRuntime, PatternAddress, RenderProfile};

#[derive(Debug, Clone)]
pub enum Mode {
    Export,
    Import,
    ListPorts,
    Control,
    Convert,
    ExtractBank,
    PackBank,
    ImportBank,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub mode: Mode,
    pub midi: MidiRuntime,
    pub target: Option<PatternAddress>,
    pub files: ArtifactPaths,
    pub render: RenderProfile,
    pub bank: BankJob,
    pub control: ControlRuntime,
}

impl Config {
    pub(in crate::config) fn new(mode: Mode, env: &AppEnv) -> Self {
        Self {
            mode,
            midi: MidiRuntime::from_env(env),
            target: None,
            files: ArtifactPaths::default(),
            render: RenderProfile::from_env(env),
            bank: BankJob::default(),
            control: ControlRuntime::from_env(env),
        }
    }

    pub(in crate::config) fn with_validated_render(self) -> Result<Self, Td3Error> {
        self.render.validate()?;
        Ok(self)
    }

    pub fn midi_export_options(&self) -> MidiExportOptions {
        self.render.to_midi_export_options()
    }

    pub fn midi_import_options(&self) -> crate::formats::mid_import::MidiImportOptions {
        let midpoint =
            ((self.render.normal_velocity as u16 + self.render.accent_velocity as u16) / 2) as u8;
        crate::formats::mid_import::MidiImportOptions {
            octave_offset: self.render.octave_offset,
            accent_threshold: midpoint,
        }
    }

    pub fn midi_export_options_for_pattern(
        &self,
        pattern: &crate::pattern::Pattern,
    ) -> Result<MidiExportOptions, Td3Error> {
        let mut options = self.midi_export_options();

        if let Some(target_bars) = self.render.bars {
            let bars_per_loop = u32::from(pattern.active_steps).div_ceil(4).max(1);
            options.loop_count = target_bars.div_ceil(bars_per_loop);
        }

        options.validate()?;
        Ok(options)
    }
}
