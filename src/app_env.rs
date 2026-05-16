//! Central runtime configuration loaded from `TD3_CONFIG.env`.
//!
//! Layering: CLI flag > `TD3_CONFIG.env` > bundled template (`config/default_env.template`).
//!
//! This module only owns the file-loading and typed-struct layer. Individual
//! consumers (clap, web handlers, MIDI code) read fields off `AppEnv` to resolve
//! their defaults.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::error::Td3Error;
use crate::formats::mid::MidiSlideMode;

/// Bundled factory defaults. Written to disk on first run if `TD3_CONFIG.env`
/// is absent. Also used as the validated in-memory defaults when a user file
/// is missing keys.
pub const DEFAULT_TEMPLATE: &str = include_str!("../config/default_env.template");

/// Where the runtime config file lives, relative to CWD.
pub const CONFIG_FILE_PATH: &str = "TD3_CONFIG.env";

/// Fully resolved runtime config. Every field is typed and validated at load.
#[derive(Debug, Clone)]
pub struct AppEnv {
    // --- MIDI & device ---
    pub midi_port_substring: String,
    pub midi_strict_name_match: bool,
    pub midi_timeout_ms: u64,
    pub midi_retries: u32,

    // --- Web UI / server ---
    pub web_port: u16,
    pub web_bind: String,
    pub ui_scratch_pattern: String,
    pub ui_auto_connect_to_midi: bool,
    pub ui_auto_set_live_update: bool,

    // --- Sequencer defaults ---
    pub ui_default_bpm: u32,
    pub ui_default_triplet: bool,
    pub ui_max_bank_history_size: u32,

    // --- Randomizer defaults ---
    pub ui_rand_default_root: u8,
    pub ui_rand_default_scale: String,
    pub ui_rand_note_percent: u8,
    pub ui_rand_slide_percent: u8,
    pub ui_rand_acc_percent: u8,
    pub ui_rand_ud_percent: u8,

    // --- Progression generator ---
    pub progression_next_pattern_save_step: u32,

    // --- Bank & library paths ---
    pub library_database_path: String,
    pub backup_dir_path: String,
    pub pattern_sidecar_dir: String,

    // --- MIDI export defaults ---
    pub midi_export_channel: u8,
    pub midi_export_ppqn: u16,
    pub midi_export_octave_offset: i8,
    pub midi_export_normal_velocity: u8,
    pub midi_export_accent_velocity: u8,
    pub midi_export_slide_mode: MidiSlideMode,
    pub midi_export_loop_count: u32,
}

impl AppEnv {
    /// Load `AppEnv` from `path`. If the file does not exist, write the bundled
    /// template to that path and use the template values. Returns
    /// `(env, first_run_created)` where `first_run_created` is true iff this
    /// call dropped the file.
    ///
    /// Precedence inside the file: user-set keys override bundled template
    /// values. Unknown keys log a warning and are ignored. Malformed lines
    /// and out-of-range values produce hard errors with line context.
    pub fn load_or_create(path: &Path) -> Result<(AppEnv, bool), Td3Error> {
        let safe_path = crate::path_safety::require_safe_user_path(path)?;
        match fs::read_to_string(&safe_path) {
            Ok(user_content) => {
                let env = parse_env(&user_content, Some(DEFAULT_TEMPLATE))?;
                Ok((env, false))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // Use create_new to avoid racing two instances on first run.
                write_template(&safe_path)?;
                let env = parse_env(DEFAULT_TEMPLATE, None)?;
                Ok((env, true))
            }
            Err(err) => Err(Td3Error::Io(err)),
        }
    }

    /// Build the in-memory defaults from the bundled template. Used by tests
    /// and by callers that want a config without touching disk.
    #[allow(dead_code)]
    pub fn from_template() -> Result<AppEnv, Td3Error> {
        parse_env(DEFAULT_TEMPLATE, None)
    }
}

/// Scale-id normalizer: trim, lowercase, spaces → underscores.
pub fn normalize_scale_id(raw: &str) -> String {
    raw.trim().to_lowercase().replace(' ', "_")
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse the entire env content into a typed `AppEnv`.
///
/// When `fallback_template` is `Some`, any key missing from `content` takes its
/// value from the parsed template. When `None`, missing keys are an error -
/// which should only ever trigger on a malformed bundled template.
fn parse_env(content: &str, fallback_template: Option<&str>) -> Result<AppEnv, Td3Error> {
    let user_pairs = parse_pairs(content)?;

    // Resolve the key lookup: if a fallback template is given, parse that too
    // and layer user values over it. This gives us per-key fallback without a
    // second pass.
    let mut pairs = if let Some(template) = fallback_template {
        parse_pairs(template)?
    } else {
        HashMap::new()
    };
    for (k, v) in user_pairs {
        pairs.insert(k, v);
    }

    let get = |key: &str| -> Result<&str, Td3Error> {
        pairs.get(key).map(String::as_str).ok_or_else(|| {
            cfg_err(format!(
                "missing required key '{}' (not in file, not in template)",
                key
            ))
        })
    };

    let env = AppEnv {
        midi_port_substring: get("MIDI_PORT_SUBSTRING")?.to_owned(),
        midi_strict_name_match: parse_bool(
            "MIDI_STRICT_NAME_MATCH",
            get("MIDI_STRICT_NAME_MATCH")?,
        )?,
        midi_timeout_ms: parse_u64("MIDI_TIMEOUT_MS", get("MIDI_TIMEOUT_MS")?)?,
        midi_retries: parse_u32("MIDI_RETRIES", get("MIDI_RETRIES")?)?,

        web_port: parse_u16("WEB_PORT", get("WEB_PORT")?)?,
        web_bind: get("WEB_BIND")?.to_owned(),
        ui_scratch_pattern: get("UI_SCRATCH_PATTERN")?.to_owned(),
        ui_auto_connect_to_midi: parse_bool(
            "UI_AUTO_CONNECT_TO_MIDI",
            get("UI_AUTO_CONNECT_TO_MIDI")?,
        )?,
        ui_auto_set_live_update: parse_bool(
            "UI_AUTO_SET_LIVE_UPDATE",
            get("UI_AUTO_SET_LIVE_UPDATE")?,
        )?,

        ui_default_bpm: parse_u32_range("UI_DEFAULT_BPM", get("UI_DEFAULT_BPM")?, 20, 300)?,
        ui_default_triplet: parse_bool("UI_DEFAULT_TRIPLET", get("UI_DEFAULT_TRIPLET")?)?,
        ui_max_bank_history_size: parse_u32(
            "UI_MAX_BANK_HISTORY_SIZE",
            get("UI_MAX_BANK_HISTORY_SIZE")?,
        )?,

        ui_rand_default_root: parse_u8_range(
            "UI_RAND_DEFAULT_ROOT",
            get("UI_RAND_DEFAULT_ROOT")?,
            0,
            11,
        )?,
        ui_rand_default_scale: normalize_scale_id(get("UI_RAND_DEFAULT_SCALE")?),
        ui_rand_note_percent: parse_u8_range(
            "UI_RAND_NOTE_PERCENT",
            get("UI_RAND_NOTE_PERCENT")?,
            0,
            100,
        )?,
        ui_rand_slide_percent: parse_u8_range(
            "UI_RAND_SLIDE_PERCENT",
            get("UI_RAND_SLIDE_PERCENT")?,
            0,
            100,
        )?,
        ui_rand_acc_percent: parse_u8_range(
            "UI_RAND_ACC_PERCENT",
            get("UI_RAND_ACC_PERCENT")?,
            0,
            100,
        )?,
        ui_rand_ud_percent: parse_u8_range(
            "UI_RAND_UD_PERCENT",
            get("UI_RAND_UD_PERCENT")?,
            0,
            100,
        )?,

        progression_next_pattern_save_step: parse_u32(
            "PROGRESSION_NEXT_PATTERN_SAVE_STEP",
            get("PROGRESSION_NEXT_PATTERN_SAVE_STEP")?,
        )?,

        library_database_path: get("LIBRARY_DATABASE_PATH")?.to_owned(),
        backup_dir_path: get("BACKUP_DIR_PATH")?.to_owned(),
        pattern_sidecar_dir: get("PATTERN_SIDECAR_DIR")?.to_owned(),

        midi_export_channel: parse_u8_range(
            "MIDI_EXPORT_CHANNEL",
            get("MIDI_EXPORT_CHANNEL")?,
            1,
            16,
        )?,
        midi_export_ppqn: parse_u16("MIDI_EXPORT_PPQN", get("MIDI_EXPORT_PPQN")?)?,
        midi_export_octave_offset: parse_i8(
            "MIDI_EXPORT_OCTAVE_OFFSET",
            get("MIDI_EXPORT_OCTAVE_OFFSET")?,
        )?,
        midi_export_normal_velocity: parse_u8_range(
            "MIDI_EXPORT_NORMAL_VELOCITY",
            get("MIDI_EXPORT_NORMAL_VELOCITY")?,
            0,
            127,
        )?,
        midi_export_accent_velocity: parse_u8_range(
            "MIDI_EXPORT_ACCENT_VELOCITY",
            get("MIDI_EXPORT_ACCENT_VELOCITY")?,
            0,
            127,
        )?,
        midi_export_slide_mode: parse_slide_mode(get("MIDI_EXPORT_SLIDE_MODE")?)?,
        midi_export_loop_count: parse_u32(
            "MIDI_EXPORT_LOOP_COUNT",
            get("MIDI_EXPORT_LOOP_COUNT")?,
        )?,
    };
    Ok(env)
}

/// Known keys - used to classify unknown user keys as warnings.
const KNOWN_KEYS: &[&str] = &[
    "MIDI_PORT_SUBSTRING",
    "MIDI_STRICT_NAME_MATCH",
    "MIDI_TIMEOUT_MS",
    "MIDI_RETRIES",
    "WEB_PORT",
    "WEB_BIND",
    "UI_SCRATCH_PATTERN",
    "UI_AUTO_CONNECT_TO_MIDI",
    "UI_AUTO_SET_LIVE_UPDATE",
    "UI_DEFAULT_BPM",
    "UI_DEFAULT_TRIPLET",
    "UI_MAX_BANK_HISTORY_SIZE",
    "UI_RAND_DEFAULT_ROOT",
    "UI_RAND_DEFAULT_SCALE",
    "UI_RAND_NOTE_PERCENT",
    "UI_RAND_SLIDE_PERCENT",
    "UI_RAND_ACC_PERCENT",
    "UI_RAND_UD_PERCENT",
    "PROGRESSION_NEXT_PATTERN_SAVE_STEP",
    "LIBRARY_DATABASE_PATH",
    "BACKUP_DIR_PATH",
    "PATTERN_SIDECAR_DIR",
    "MIDI_EXPORT_CHANNEL",
    "MIDI_EXPORT_PPQN",
    "MIDI_EXPORT_OCTAVE_OFFSET",
    "MIDI_EXPORT_NORMAL_VELOCITY",
    "MIDI_EXPORT_ACCENT_VELOCITY",
    "MIDI_EXPORT_SLIDE_MODE",
    "MIDI_EXPORT_LOOP_COUNT",
];

/// Parse raw KEY=VALUE content into a map. Strips comments, blank lines, and
/// surrounding double quotes. Emits a warning via `log::warn!` for unknown
/// keys but does not fail on them.
fn parse_pairs(content: &str) -> Result<HashMap<String, String>, Td3Error> {
    let mut out = HashMap::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let eq_pos = line.find('=').ok_or_else(|| {
            cfg_err(format!(
                "TD3_CONFIG.env line {}: no '=' found in '{}'",
                lineno, line
            ))
        })?;
        let key = line[..eq_pos].trim();
        if key.is_empty() {
            return Err(cfg_err(format!(
                "TD3_CONFIG.env line {}: empty key before '='",
                lineno
            )));
        }
        let value = strip_quotes(line[eq_pos + 1..].trim());

        if !KNOWN_KEYS.contains(&key) {
            log::warn!(
                "TD3_CONFIG.env line {}: unknown key '{}' - ignoring",
                lineno,
                key
            );
            continue;
        }
        out.insert(key.to_owned(), value.to_owned());
    }
    Ok(out)
}

fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Typed parsers
// ---------------------------------------------------------------------------

fn parse_bool(key: &str, value: &str) -> Result<bool, Td3Error> {
    match value.trim() {
        "0" | "false" | "FALSE" | "no" | "NO" => Ok(false),
        "1" | "true" | "TRUE" | "yes" | "YES" => Ok(true),
        other => Err(cfg_err(format!(
            "TD3_CONFIG.env '{}' must be 0/1 (or true/false), got '{}'",
            key, other
        ))),
    }
}

fn parse_u64(key: &str, value: &str) -> Result<u64, Td3Error> {
    value.trim().parse::<u64>().map_err(|_| {
        cfg_err(format!(
            "TD3_CONFIG.env '{}' must be a non-negative integer, got '{}'",
            key, value
        ))
    })
}

fn parse_u32(key: &str, value: &str) -> Result<u32, Td3Error> {
    value.trim().parse::<u32>().map_err(|_| {
        cfg_err(format!(
            "TD3_CONFIG.env '{}' must be a non-negative integer (u32), got '{}'",
            key, value
        ))
    })
}

fn parse_u16(key: &str, value: &str) -> Result<u16, Td3Error> {
    value.trim().parse::<u16>().map_err(|_| {
        cfg_err(format!(
            "TD3_CONFIG.env '{}' must be a non-negative integer in 0..=65535, got '{}'",
            key, value
        ))
    })
}

fn parse_u8_range(key: &str, value: &str, min: u8, max: u8) -> Result<u8, Td3Error> {
    let n: u8 = value.trim().parse().map_err(|_| {
        cfg_err(format!(
            "TD3_CONFIG.env '{}' must be an integer in {}..={}, got '{}'",
            key, min, max, value
        ))
    })?;
    if n < min || n > max {
        return Err(cfg_err(format!(
            "TD3_CONFIG.env '{}' = {} out of range (expected {}..={})",
            key, n, min, max
        )));
    }
    Ok(n)
}

fn parse_u32_range(key: &str, value: &str, min: u32, max: u32) -> Result<u32, Td3Error> {
    let n = parse_u32(key, value)?;
    if n < min || n > max {
        return Err(cfg_err(format!(
            "TD3_CONFIG.env '{}' = {} out of range (expected {}..={})",
            key, n, min, max
        )));
    }
    Ok(n)
}

fn parse_i8(key: &str, value: &str) -> Result<i8, Td3Error> {
    value.trim().parse::<i8>().map_err(|_| {
        cfg_err(format!(
            "TD3_CONFIG.env '{}' must be an integer in -128..=127, got '{}'",
            key, value
        ))
    })
}

fn parse_slide_mode(value: &str) -> Result<MidiSlideMode, Td3Error> {
    let v = strip_quotes(value.trim());
    match v.to_lowercase().as_str() {
        "td3" => Ok(MidiSlideMode::Td3),
        "generic" => Ok(MidiSlideMode::Generic),
        "none" => Ok(MidiSlideMode::None),
        _ => Err(cfg_err(format!(
            "TD3_CONFIG.env 'MIDI_EXPORT_SLIDE_MODE' must be td3|generic|none, got '{}'",
            value
        ))),
    }
}

// ---------------------------------------------------------------------------
// File write
// ---------------------------------------------------------------------------

fn write_template(path: &Path) -> Result<(), Td3Error> {
    // Atomic create-new: racing instances both trying to drop the file on
    // first run will get exactly one writer; the loser falls through and the
    // retry reads the now-existing file.
    use std::fs::OpenOptions;
    use std::io::Write;

    let safe_path = crate::path_safety::require_safe_user_path(path)?;
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&safe_path)
    {
        Ok(f) => f,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            // A concurrent run beat us. Not an error.
            return Ok(());
        }
        Err(err) => return Err(Td3Error::Io(err)),
    };
    file.write_all(DEFAULT_TEMPLATE.as_bytes())
        .map_err(Td3Error::Io)?;
    Ok(())
}

fn cfg_err(msg: String) -> Td3Error {
    Td3Error::CliError(msg)
}
