use super::*;

// ---------------------------------------------------------------------------
// User-mutable settings storage (keyboard / scales / progression)
// ---------------------------------------------------------------------------

fn config_error(err: config_storage::UserConfigStorageError) -> AppError {
    if err.is_client_error() {
        AppError::BadRequest(err.to_string())
    } else {
        AppError::Internal(err.to_string())
    }
}

fn typed_config_payload<T>(
    payload: Result<Json<T>, JsonRejection>,
    name: &'static str,
) -> Result<T, AppError> {
    payload.map(|Json(config)| config).map_err(|err| {
        let mut message = String::from("invalid ");
        message.push_str(name);
        message.push_str(" config JSON: ");
        message.push_str(&err.to_string());
        AppError::BadRequest(message)
    })
}

pub async fn get_keyboard_config(
    State(state): State<ConfigState>,
) -> Result<Json<KeyboardConfig>, AppError> {
    let config = config_storage::read_user_config::<KeyboardConfig>(&state.user_config_dir)
        .map_err(config_error)?;
    Ok(Json(config))
}

pub async fn save_keyboard_config(
    State(state): State<ConfigState>,
    payload: Result<Json<KeyboardConfig>, JsonRejection>,
) -> Result<Json<SaveConfigResponse>, AppError> {
    let config = typed_config_payload(payload, KeyboardConfig::NAME)?;
    config_storage::write_user_config(&state.user_config_dir, config).map_err(config_error)?;
    Ok(Json(SaveConfigResponse { ok: true }))
}

pub async fn get_scales_config(
    State(state): State<ConfigState>,
) -> Result<Json<ScalesConfig>, AppError> {
    let config = config_storage::read_user_config::<ScalesConfig>(&state.user_config_dir)
        .map_err(config_error)?;
    Ok(Json(config))
}

pub async fn save_scales_config(
    State(state): State<ConfigState>,
    payload: Result<Json<ScalesConfig>, JsonRejection>,
) -> Result<Json<SaveConfigResponse>, AppError> {
    let config = typed_config_payload(payload, ScalesConfig::NAME)?;
    config_storage::write_user_config(&state.user_config_dir, config).map_err(config_error)?;
    Ok(Json(SaveConfigResponse { ok: true }))
}

pub async fn get_progression_config(
    State(state): State<ConfigState>,
) -> Result<Json<ProgressionConfig>, AppError> {
    let config = config_storage::read_user_config::<ProgressionConfig>(&state.user_config_dir)
        .map_err(config_error)?;
    Ok(Json(config))
}

pub async fn save_progression_config(
    State(state): State<ConfigState>,
    payload: Result<Json<ProgressionConfig>, JsonRejection>,
) -> Result<Json<SaveConfigResponse>, AppError> {
    let config = typed_config_payload(payload, ProgressionConfig::NAME)?;
    config_storage::write_user_config(&state.user_config_dir, config).map_err(config_error)?;
    Ok(Json(SaveConfigResponse { ok: true }))
}

// ---------------------------------------------------------------------------
// GET /api/config/env
// ---------------------------------------------------------------------------
//
// Returns the UI-relevant subset of `TD3_CONFIG.env` so the browser can stamp
// boot-time defaults into sliders, selects, and toggles instead of hard-coding
// them in HTML attributes or JS `|| fallback` expressions. Populated once at
// server startup from `AppEnv` - the response is a faithful snapshot of the
// env values, not a live read of the file, so edits made while the server is
// running require a restart to take effect.

pub async fn get_env_config(
    State(state): State<ConfigState>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(crate::web::static_html::build_payload(
        &state.ui_config,
    )))
}

// ---------------------------------------------------------------------------
// GET /api/config/env/full
// ---------------------------------------------------------------------------
//
// Settings → CONFIG page payload. Every call re-reads `TD3_CONFIG.env` so
// that a value rolled back externally or via the "Reset section" button
// shows up immediately. Keys the file doesn't mention fall back to the
// bundled default template - this matches the loader's layering rule so
// the form is never left with blank inputs.

pub async fn get_env_config_full(
    State(state): State<ConfigState>,
) -> Result<Json<EnvFullResponse>, AppError> {
    let env_path = crate::path_safety::require_safe_user_path(state.env_file_path.clone())
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let file_content = match std::fs::read_to_string(&env_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(AppError::BadRequest(format!(
                "failed to read {}: {}",
                env_path.display(),
                e
            )))
        }
    };

    let user_pairs = crate::env_writer::read_raw_pairs(&file_content);
    let template_pairs = crate::env_writer::read_raw_pairs(crate::app_env::DEFAULT_TEMPLATE);

    let sections = crate::env_metadata::SECTIONS
        .iter()
        .map(|s| EnvSectionInfo {
            id: s.id.to_owned(),
            title: s.title.to_owned(),
        })
        .collect();

    let fields = crate::env_metadata::FIELDS
        .iter()
        .map(|f| {
            let (kind, min, max, options) = match &f.kind {
                crate::env_metadata::FieldKind::String => ("string", None, None, None),
                crate::env_metadata::FieldKind::Integer { min, max } => {
                    ("integer", Some(*min), Some(*max), None)
                }
                crate::env_metadata::FieldKind::Bool => ("bool", None, None, None),
                crate::env_metadata::FieldKind::Enum { options } => (
                    "enum",
                    None,
                    None,
                    Some(options.iter().map(|o| (*o).to_owned()).collect()),
                ),
                crate::env_metadata::FieldKind::ScaleId => ("scaleId", None, None, None),
            };
            EnvFieldInfo {
                key: f.key.to_owned(),
                section_id: f.section_id.to_owned(),
                description: f.description.to_owned(),
                kind: kind.to_owned(),
                min,
                max,
                options,
            }
        })
        .collect();

    let mut values = std::collections::HashMap::new();
    for f in crate::env_metadata::FIELDS {
        let v = user_pairs
            .get(f.key)
            .or_else(|| template_pairs.get(f.key))
            .cloned()
            .unwrap_or_default();
        values.insert(f.key.to_owned(), v);
    }

    Ok(Json(EnvFullResponse {
        sections,
        fields,
        values,
        env_file_path: env_path.display().to_string(),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/config/env
// ---------------------------------------------------------------------------
//
// Persist a sparse `{ KEY: raw_value }` patch to `TD3_CONFIG.env`. The
// pipeline is:
//
//   1. Reject any key that isn't declared in `env_metadata::FIELDS`.
//      Unknown keys never touch disk - this is the gate that prevents
//      the UI from accidentally (or maliciously) introducing scaffold
//      keys the loader would then ignore.
//   2. Run the per-field validator for every accepted key; any failure
//      aborts the whole batch so the file can't end up half-new.
//   3. Hand the validated batch to `env_writer::apply_updates`, which
//      preserves comments, keeps the single `.bak` generation, and does
//      the tmp→rename dance.
//
// Success only means the disk file was rewritten. Changes take effect
// on the next restart; the UI surfaces that to the user.

pub async fn save_env_config(
    State(state): State<ConfigState>,
    Json(req): Json<EnvUpdateRequest>,
) -> Result<Json<SaveConfigResponse>, AppError> {
    if req.updates.is_empty() {
        return Ok(Json(SaveConfigResponse { ok: true }));
    }

    for (key, raw) in &req.updates {
        crate::env_metadata::validate_value(key, raw)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    }

    let env_path = state.env_file_path.clone();
    crate::env_writer::apply_updates(&env_path, &req.updates)
        .map_err(|e| AppError::BadRequest(format!("write failed: {}", e)))?;

    Ok(Json(SaveConfigResponse { ok: true }))
}

// ---------------------------------------------------------------------------
// POST /api/config/env/reset-section
// ---------------------------------------------------------------------------
//
// Reset every editable key in a section back to the bundled template
// default. Goes through the same validate → apply_updates pipeline as
// a normal save, so the single `.bak` generation and comment
// preservation apply here too. Unknown section ids are rejected.

pub async fn reset_env_config_section(
    State(state): State<ConfigState>,
    Json(req): Json<EnvResetSectionRequest>,
) -> Result<Json<SaveConfigResponse>, AppError> {
    let section_known = crate::env_metadata::SECTIONS
        .iter()
        .any(|s| s.id == req.section_id);
    if !section_known {
        return Err(AppError::BadRequest(format!(
            "unknown section '{}'",
            req.section_id
        )));
    }

    let template_pairs = crate::env_writer::read_raw_pairs(crate::app_env::DEFAULT_TEMPLATE);

    let mut updates: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for f in crate::env_metadata::FIELDS {
        if f.section_id != req.section_id {
            continue;
        }
        let Some(v) = template_pairs.get(f.key) else {
            // The template is the single source of truth; a missing key
            // here means the table and the template drifted apart -
            // surface it clearly rather than silently skipping.
            return Err(AppError::BadRequest(format!(
                "internal: default template is missing '{}' (section '{}')",
                f.key, f.section_id
            )));
        };
        crate::env_metadata::validate_value(f.key, v)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        updates.insert(f.key.to_owned(), v.clone());
    }

    if updates.is_empty() {
        return Ok(Json(SaveConfigResponse { ok: true }));
    }

    let env_path = state.env_file_path.clone();
    crate::env_writer::apply_updates(&env_path, &updates)
        .map_err(|e| AppError::BadRequest(format!("write failed: {}", e)))?;

    Ok(Json(SaveConfigResponse { ok: true }))
}
