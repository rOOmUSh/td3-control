use std::fs;
use std::io;
use std::path::Path;

use crate::error::Td3Error;
use crate::formats::mid::MidiSlideMode;

use super::parse::parse_env;
use super::template::{write_template, DEFAULT_TEMPLATE};

/// Fully resolved runtime config. Every field is typed and validated at load.
#[derive(Debug, Clone)]
pub struct AppEnv {
    pub midi_port_substring: String,
    pub midi_strict_name_match: bool,
    pub midi_timeout_ms: u64,
    pub midi_retries: u32,

    pub web_port: u16,
    pub web_bind: String,
    pub ui_scratch_pattern: String,
    pub ui_auto_connect_to_midi: bool,
    pub ui_auto_set_live_update: bool,

    pub ui_default_bpm: u32,
    pub ui_default_triplet: bool,
    pub ui_max_bank_history_size: u32,

    pub ui_rand_default_root: u8,
    pub ui_rand_default_scale: String,
    pub ui_rand_note_percent: u8,
    pub ui_rand_slide_percent: u8,
    pub ui_rand_acc_percent: u8,
    pub ui_rand_ud_percent: u8,

    pub progression_next_pattern_save_step: u32,

    pub library_database_path: String,
    pub backup_dir_path: String,
    pub pattern_sidecar_dir: String,

    pub midi_export_channel: u8,
    pub midi_export_ppqn: u16,
    pub midi_export_octave_offset: i8,
    pub midi_export_normal_velocity: u8,
    pub midi_export_accent_velocity: u8,
    pub midi_export_slide_mode: MidiSlideMode,
    pub midi_export_loop_count: u32,
}

impl AppEnv {
    /// Load `AppEnv` from `path`.
    ///
    /// If the file does not exist, the bundled template is written to that path
    /// and the template values are used.
    pub fn load_or_create(path: &Path) -> Result<(AppEnv, bool), Td3Error> {
        let safe_path = crate::path_safety::require_safe_user_path(path)?;
        match fs::read_to_string(&safe_path) {
            Ok(user_content) => {
                let env = parse_env(&user_content, Some(DEFAULT_TEMPLATE))?;
                Ok((env, false))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                write_template(&safe_path)?;
                let env = parse_env(DEFAULT_TEMPLATE, None)?;
                Ok((env, true))
            }
            Err(err) => Err(Td3Error::Io(err)),
        }
    }

    /// Build the in-memory defaults from the bundled template.
    #[allow(dead_code)]
    pub fn from_template() -> Result<AppEnv, Td3Error> {
        parse_env(DEFAULT_TEMPLATE, None)
    }
}
