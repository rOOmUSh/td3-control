use std::collections::HashMap;

use crate::error::Td3Error;

use super::keys::KNOWN_KEYS;
use super::model::AppEnv;
use super::normalize_scale_id;
use super::values::{
    cfg_err, parse_bool, parse_i8, parse_slide_mode, parse_u16, parse_u32, parse_u32_range,
    parse_u64, parse_u8_range, strip_quotes,
};

pub(super) fn parse_env(
    content: &str,
    fallback_template: Option<&str>,
) -> Result<AppEnv, Td3Error> {
    let user_pairs = parse_pairs(content)?;

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
