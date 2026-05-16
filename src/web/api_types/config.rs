use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct SaveConfigResponse {
    pub ok: bool,
}

// --- Settings → CONFIG page (TD3_CONFIG.env editor) -----------------

/// One editable section of the env file - rendered as a sidebar nav item.
#[derive(Serialize, Deserialize)]
pub struct EnvSectionInfo {
    pub id: String,
    pub title: String,
}

/// One editable field - rendered as a form row inside its section.
/// `kind` is a tag plus optional constraints, matching
/// `env_metadata::FieldKind`:
///
/// - `"string"`
/// - `"integer"` with `min`, `max`
/// - `"bool"`
/// - `"enum"`    with `options`
/// - `"scaleId"`
#[derive(Serialize, Deserialize)]
pub struct EnvFieldInfo {
    pub key: String,
    pub section_id: String,
    pub description: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
pub struct EnvFullResponse {
    /// Sections in the order they should appear in the sidebar.
    pub sections: Vec<EnvSectionInfo>,
    /// All editable fields in the order declared in `env_metadata::FIELDS`.
    pub fields: Vec<EnvFieldInfo>,
    /// Current raw string value per key (from the live env file, falling
    /// back to the bundled default template for keys the user hasn't
    /// overridden yet). Scale ids are returned as written.
    pub values: std::collections::HashMap<String, String>,
    /// Full path to the file that will be written on save. The UI shows
    /// this under the save button so users know exactly what gets
    /// modified.
    pub env_file_path: String,
}

#[derive(Deserialize)]
pub struct EnvUpdateRequest {
    /// Sparse `{ KEY: raw_string_value }` map - only the fields the user
    /// actually edited. Keys unknown to `env_metadata::FIELDS` are
    /// rejected before any file write occurs.
    pub updates: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct EnvResetSectionRequest {
    pub section_id: String,
}

// ---------------------------------------------------------------------------
// Scratch pattern
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct ScratchPatternResponse {
    pub patgroup: u8,
    pub pattern: u8,
    pub side: String,
    pub label: String,
}

// ---------------------------------------------------------------------------
