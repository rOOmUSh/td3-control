//! Typed metadata for the `TD3_CONFIG.env` keys exposed through the web
//! Settings UI.
//!
//! The table in `FIELDS` is the single source of truth for:
//!
//! - which keys the SETTINGS page lets the user edit
//! - what type / range / option-set each key accepts
//! - which of the seven section groupings each key belongs to
//! - what user-readable description is shown beside the input
//!
//! The ranges here are deliberately tighter than the loader ranges in
//! `app_env.rs`. The loader stays permissive for backward-compatibility
//! with hand-edited files; the UI refuses to *write* anything outside
//! the stricter ranges.

use crate::error::Td3Error;

/// A single editable config key.
pub struct FieldMeta {
    pub key: &'static str,
    pub section_id: &'static str,
    pub kind: FieldKind,
    pub description: &'static str,
}

/// Runtime type of a config field.
pub enum FieldKind {
    /// Non-empty text.
    String,
    /// Signed integer clamped to `[min, max]`.
    Integer { min: i64, max: i64 },
    /// Accepts `0/1/true/false/yes/no` (case-insensitive for load;
    /// the UI always sends `0` or `1`).
    Bool,
    /// Must match one of `options` (case-insensitive).
    Enum { options: &'static [&'static str] },
    /// A scale id from `config/scales-config.json` (or the embedded scales-defaults.json
    /// fallback). Validated as a non-empty string server-side; the UI enforces the
    /// dropdown choice.
    ScaleId,
}

/// A sidebar section group in the SETTINGS → CONFIG navigation.
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
}

pub const SECTIONS: &[Section] = &[
    Section {
        id: "midi_device",
        title: "MIDI & DEVICE",
    },
    Section {
        id: "web_server",
        title: "WEB SERVER",
    },
    Section {
        id: "sequencer",
        title: "SEQUENCER DEFAULTS",
    },
    Section {
        id: "randomizer",
        title: "RANDOMIZER",
    },
    Section {
        id: "progression",
        title: "PROGRESSION",
    },
    Section {
        id: "library",
        title: "BANK & LIBRARY",
    },
    Section {
        id: "midi_export",
        title: "MIDI EXPORT",
    },
];

pub const FIELDS: &[FieldMeta] = &[
    // --- MIDI & DEVICE ---
    FieldMeta {
        key: "MIDI_PORT_SUBSTRING",
        section_id: "midi_device",
        kind: FieldKind::String,
        description: "Substring used to identify the TD-3 MIDI input/output ports.",
    },
    FieldMeta {
        key: "MIDI_STRICT_NAME_MATCH",
        section_id: "midi_device",
        kind: FieldKind::Bool,
        description: "If 1, requires an exact case-sensitive match for MIDI port names.",
    },
    FieldMeta {
        key: "MIDI_TIMEOUT_MS",
        section_id: "midi_device",
        kind: FieldKind::Integer { min: 100, max: 60_000 },
        description: "Timeout in milliseconds for MIDI SysEx request/response cycles.",
    },
    FieldMeta {
        key: "MIDI_RETRIES",
        section_id: "midi_device",
        kind: FieldKind::Integer { min: 0, max: 10 },
        description: "Retries on a failed MIDI probe or pattern download. Uploads are never retried.",
    },

    // --- WEB SERVER ---
    FieldMeta {
        key: "WEB_PORT",
        section_id: "web_server",
        kind: FieldKind::Integer { min: 1, max: 65_535 },
        description: "The TCP port the web server will listen on.",
    },
    FieldMeta {
        key: "WEB_BIND",
        section_id: "web_server",
        kind: FieldKind::String,
        description: "Server bind address. 127.0.0.1 = local only, 0.0.0.0 = network reachable.",
    },
    FieldMeta {
        key: "UI_SCRATCH_PATTERN",
        section_id: "web_server",
        kind: FieldKind::String,
        description: "Scratch pattern slot, e.g. G1-P2A (Group 1-4, Pattern 1-8, Side A/B). WILL BE OVERWRITTEN during operation.",
    },
    FieldMeta {
        key: "UI_AUTO_CONNECT_TO_MIDI",
        section_id: "web_server",
        kind: FieldKind::Bool,
        description: "If 1, auto-connect to the TD-3 on page load.",
    },
    FieldMeta {
        key: "UI_AUTO_SET_LIVE_UPDATE",
        section_id: "web_server",
        kind: FieldKind::Bool,
        description: "If 1, every sequencer edit is sent immediately to the TD-3 scratch pattern.",
    },

    // --- SEQUENCER DEFAULTS ---
    FieldMeta {
        key: "UI_DEFAULT_BPM",
        section_id: "sequencer",
        kind: FieldKind::Integer { min: 20, max: 300 },
        description: "Initial BPM for the web UI transport and preview features.",
    },
    FieldMeta {
        key: "UI_DEFAULT_TRIPLET",
        section_id: "sequencer",
        kind: FieldKind::Bool,
        description: "Default state of the Triplet timing mode.",
    },
    FieldMeta {
        key: "UI_MAX_BANK_HISTORY_SIZE",
        section_id: "sequencer",
        kind: FieldKind::Integer { min: 1, max: 100_000 },
        description: "Maximum number of patterns the Bank History sidebar holds before discarding the oldest.",
    },

    // --- RANDOMIZER ---
    FieldMeta {
        key: "UI_RAND_DEFAULT_ROOT",
        section_id: "randomizer",
        kind: FieldKind::Integer { min: 0, max: 11 },
        description: "Default root note index (0=C, 1=C#, ..., 11=B).",
    },
    FieldMeta {
        key: "UI_RAND_DEFAULT_SCALE",
        section_id: "randomizer",
        kind: FieldKind::ScaleId,
        description: "Default scale name. Must match an id in config/scales-config.json (or scales-defaults.json) after normalization.",
    },
    FieldMeta {
        key: "UI_RAND_NOTE_PERCENT",
        section_id: "randomizer",
        kind: FieldKind::Integer { min: 0, max: 100 },
        description: "Default percentage of steps that will be active (note density).",
    },
    FieldMeta {
        key: "UI_RAND_SLIDE_PERCENT",
        section_id: "randomizer",
        kind: FieldKind::Integer { min: 0, max: 100 },
        description: "Default percentage of active steps that get a slide flag.",
    },
    FieldMeta {
        key: "UI_RAND_ACC_PERCENT",
        section_id: "randomizer",
        kind: FieldKind::Integer { min: 0, max: 100 },
        description: "Default percentage of active steps that get an accent flag.",
    },
    FieldMeta {
        key: "UI_RAND_UD_PERCENT",
        section_id: "randomizer",
        kind: FieldKind::Integer { min: 0, max: 100 },
        description: "Default percentage of steps that carry an UP/DOWN transpose flag. UP and DOWN are mutually exclusive; the randomizer picks one 50/50 per chosen step.",
    },

    // --- PROGRESSION ---
    FieldMeta {
        key: "PROGRESSION_NEXT_PATTERN_SAVE_STEP",
        section_id: "progression",
        kind: FieldKind::Integer { min: 0, max: 16 },
        description: "Step in the progression logic that triggers a pattern save to the local library.",
    },

    // --- BANK & LIBRARY ---
    FieldMeta {
        key: "LIBRARY_DATABASE_PATH",
        section_id: "library",
        kind: FieldKind::String,
        description: "Path to the SQLite file where the pattern library is stored.",
    },
    FieldMeta {
        key: "BACKUP_DIR_PATH",
        section_id: "library",
        kind: FieldKind::String,
        description: "Directory for temporary device backups created before UI sessions or bulk bank imports.",
    },
    FieldMeta {
        key: "PATTERN_SIDECAR_DIR",
        section_id: "library",
        kind: FieldKind::String,
        description: "Directory where per-pattern .syx sidecars are stored for duplicate detection.",
    },

    // --- MIDI EXPORT ---
    FieldMeta {
        key: "MIDI_EXPORT_CHANNEL",
        section_id: "midi_export",
        kind: FieldKind::Integer { min: 1, max: 16 },
        description: "Default MIDI channel (1-16) for exported .mid files.",
    },
    FieldMeta {
        key: "MIDI_EXPORT_PPQN",
        section_id: "midi_export",
        kind: FieldKind::Integer { min: 24, max: 3840 },
        description: "Ticks Per Quarter Note resolution for exported MIDI files.",
    },
    FieldMeta {
        key: "MIDI_EXPORT_OCTAVE_OFFSET",
        section_id: "midi_export",
        kind: FieldKind::Integer { min: -60, max: 60 },
        description: "Semitone offset applied to notes during MIDI export.",
    },
    FieldMeta {
        key: "MIDI_EXPORT_NORMAL_VELOCITY",
        section_id: "midi_export",
        kind: FieldKind::Integer { min: 0, max: 127 },
        description: "MIDI velocity used for standard (non-accented) notes.",
    },
    FieldMeta {
        key: "MIDI_EXPORT_ACCENT_VELOCITY",
        section_id: "midi_export",
        kind: FieldKind::Integer { min: 0, max: 127 },
        description: "MIDI velocity used for accented notes.",
    },
    FieldMeta {
        key: "MIDI_EXPORT_SLIDE_MODE",
        section_id: "midi_export",
        kind: FieldKind::Enum { options: &["td3", "generic", "none"] },
        description: "How slides are rendered in MIDI: td3 (overlapping notes), generic, or none.",
    },
    FieldMeta {
        key: "MIDI_EXPORT_LOOP_COUNT",
        section_id: "midi_export",
        kind: FieldKind::Integer { min: 1, max: 256 },
        description: "Default number of times the pattern loops in a single exported MIDI file.",
    },
];

/// Look up the metadata for a known key. Returns `None` for unknown keys.
pub fn find(key: &str) -> Option<&'static FieldMeta> {
    FIELDS.iter().find(|f| f.key == key)
}

/// Validate a raw string value against its key's metadata.
///
/// Unknown keys are rejected - the write endpoint must never persist a key
/// that the Settings UI did not declare as editable.
pub fn validate_value(key: &str, raw: &str) -> Result<(), Td3Error> {
    let meta = find(key).ok_or_else(|| cli_err(format!("unknown config key '{}'", key)))?;
    match &meta.kind {
        FieldKind::String => validate_non_empty(key, raw)?,
        FieldKind::Integer { min, max } => validate_integer(key, raw, *min, *max)?,
        FieldKind::Bool => validate_bool(key, raw)?,
        FieldKind::Enum { options } => validate_enum(key, raw, options)?,
        FieldKind::ScaleId => validate_non_empty(key, raw)?,
    }
    // Per-key custom validators on top of the base type check.
    if key == "UI_SCRATCH_PATTERN" {
        crate::config::parse_pattern_address(raw)
            .map_err(|e| cli_err(format!("'{}': {}", key, e)))?;
    }
    Ok(())
}

fn validate_non_empty(key: &str, raw: &str) -> Result<(), Td3Error> {
    if raw.trim().is_empty() {
        return Err(cli_err(format!("'{}' cannot be empty", key)));
    }
    Ok(())
}

fn validate_integer(key: &str, raw: &str, min: i64, max: i64) -> Result<(), Td3Error> {
    let parsed: i64 = raw.trim().parse().map_err(|_| {
        cli_err(format!(
            "'{}' must be an integer in {}..={}, got '{}'",
            key, min, max, raw
        ))
    })?;
    if parsed < min || parsed > max {
        return Err(cli_err(format!(
            "'{}' = {} out of range (expected {}..={})",
            key, parsed, min, max
        )));
    }
    Ok(())
}

fn validate_bool(key: &str, raw: &str) -> Result<(), Td3Error> {
    match raw.trim() {
        "0" | "1" | "true" | "false" | "TRUE" | "FALSE" | "yes" | "no" | "YES" | "NO" => Ok(()),
        other => Err(cli_err(format!(
            "'{}' must be 0/1 or true/false, got '{}'",
            key, other
        ))),
    }
}

fn validate_enum(key: &str, raw: &str, options: &[&str]) -> Result<(), Td3Error> {
    let v = raw.trim();
    if options.iter().any(|o| o.eq_ignore_ascii_case(v)) {
        Ok(())
    } else {
        Err(cli_err(format!(
            "'{}' must be one of {:?}, got '{}'",
            key, options, raw
        )))
    }
}

fn cli_err(msg: String) -> Td3Error {
    Td3Error::CliError(msg)
}
