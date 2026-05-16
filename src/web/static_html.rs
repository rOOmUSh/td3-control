//! Inline-config-injecting HTML handler.
//!
//! The four UI pages (`index.html`, `progression.html`, `bank.html`,
//! `settings.html`) carry an `<!-- TD3_CONFIG_INJECT -->` placeholder in
//! their `<head>`. At request time we read the file, replace the token
//! with a `<script>window.TD3_CONFIG_ENV = {...};</script>` block, and
//! serve the result. This way the browser has the resolved env at first
//! paint - no `/api/config/env` round-trip on boot, no async race, no
//! hardcoded fallback literals required on the client.
//!
//! If the placeholder is missing from a file we fall back to inserting
//! before `</head>`. If neither marker is present we prepend the script
//! tag - the page still works, but operators should add the token so
//! the script lives inside `<head>` like every other module.
//!
//! Static assets (CSS, JS, JSON) keep going through the embedded asset
//! handler. Only HTML files get the injection.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;

use crate::web::embedded_ui;
use crate::web::state::{ConfigState, UiConfigSnapshot};

const TOKEN: &str = "<!-- TD3_CONFIG_INJECT -->";

pub async fn serve_index(state: State<ConfigState>) -> Result<Html<String>, (StatusCode, String)> {
    serve_html(state, "index.html").await
}

pub async fn serve_progression(
    state: State<ConfigState>,
) -> Result<Html<String>, (StatusCode, String)> {
    serve_html(state, "progression.html").await
}

pub async fn serve_bank(state: State<ConfigState>) -> Result<Html<String>, (StatusCode, String)> {
    serve_html(state, "bank.html").await
}

pub async fn serve_settings(
    state: State<ConfigState>,
) -> Result<Html<String>, (StatusCode, String)> {
    serve_html(state, "settings.html").await
}

async fn serve_html(
    State(state): State<ConfigState>,
    relative_path: &str,
) -> Result<Html<String>, (StatusCode, String)> {
    let content = match embedded_ui::read_text(relative_path) {
        Some(c) => c,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("{}: not found in embedded assets", relative_path),
            ))
        }
    };

    let inject = build_inline_script(&state.ui_config);
    let injected = if let Some(pos) = content.find(TOKEN) {
        let mut s = String::with_capacity(content.len() + inject.len());
        s.push_str(&content[..pos]);
        s.push_str(&inject);
        s.push_str(&content[pos + TOKEN.len()..]);
        s
    } else if let Some(pos) = content.find("</head>") {
        let mut s = String::with_capacity(content.len() + inject.len());
        s.push_str(&content[..pos]);
        s.push_str(&inject);
        s.push_str(&content[pos..]);
        s
    } else {
        format!("{}{}", inject, content)
    };

    Ok(Html(injected))
}

/// Build the `<script>window.TD3_CONFIG_ENV = {...};</script>` block.
fn build_inline_script(c: &UiConfigSnapshot) -> String {
    let payload = build_payload(c);
    let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
    format!("<script>window.TD3_CONFIG_ENV={};</script>", json)
}

/// JSON-serializable payload - the same shape `GET /api/config/env`
/// returns. Pulled into its own helper so the inline-inject path and the
/// fetch endpoint cannot drift.
pub fn build_payload(c: &UiConfigSnapshot) -> serde_json::Value {
    serde_json::json!({
        "uiAutoConnectToMidi": c.ui_auto_connect_to_midi,
        "uiAutoSetLiveUpdate": c.ui_auto_set_live_update,
        "uiDefaultBpm": c.ui_default_bpm,
        "uiDefaultTriplet": c.ui_default_triplet,
        "uiMaxBankHistorySize": c.ui_max_bank_history_size,
        "uiRandDefaultRoot": c.ui_rand_default_root,
        "uiRandDefaultScale": c.ui_rand_default_scale,
        "uiRandNotePercent": c.ui_rand_note_percent,
        "uiRandSlidePercent": c.ui_rand_slide_percent,
        "uiRandAccPercent": c.ui_rand_acc_percent,
        "uiRandUdPercent": c.ui_rand_ud_percent,
        "progressionNextPatternSaveStep": c.progression_next_pattern_save_step,
    })
}
