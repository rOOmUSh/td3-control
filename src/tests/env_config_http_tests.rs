//! HTTP integration tests for the Settings → CONFIG endpoints:
//!
//! - `GET  /api/config/env/full`
//! - `POST /api/config/env`
//! - `POST /api/config/env/reset-section`
//!
//! Every test points `env_file_path` at a unique temp file so real
//! `TD3_CONFIG.env` is never touched. The bundled default template is
//! copied in for full-state/reset tests so template-backed defaults
//! match production behavior.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::app_env::DEFAULT_TEMPLATE;
use crate::web::handlers;
use crate::web::state::{AppState, ScratchSlot, UiConfigSnapshot};

fn unique_env_path(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("td3-env-config-{}-{}-{}.env", tag, pid, n))
}

fn make_library() -> std::sync::Arc<crate::library::LibraryStore> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("td3-env-config-lib-{}-{}.json", pid, n));
    let _ = std::fs::remove_file(&path);
    std::sync::Arc::new(crate::library::LibraryStore::load_or_create(path).expect("test library"))
}

/// Build a test router wired to a temp env file.
fn build_router(env_path: PathBuf) -> Router {
    let state = AppState::for_tests(
        ScratchSlot {
            patgroup: 0,
            slot: 0,
            side: 0,
        },
        make_library(),
        String::new(),
        UiConfigSnapshot::for_tests(),
        env_path,
    );
    Router::new()
        .route("/api/config/env/full", get(handlers::get_env_config_full))
        .route("/api/config/env", post(handlers::save_env_config))
        .route(
            "/api/config/env/reset-section",
            post(handlers::reset_env_config_section),
        )
        .with_state(state)
}

fn write_template_to(path: &PathBuf) {
    std::fs::write(path, DEFAULT_TEMPLATE).expect("seed env file");
}

async fn get_full(app: Router) -> serde_json::Value {
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/config/env/full")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn post_json(
    app: Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

// ── GET /api/config/env/full ─────────────────────────────────────────

#[tokio::test]
async fn full_returns_sections_fields_and_values() {
    let env_path = unique_env_path("full-basic");
    write_template_to(&env_path);
    let app = build_router(env_path.clone());

    let json = get_full(app).await;
    let sections = json["sections"].as_array().expect("sections array");
    let fields = json["fields"].as_array().expect("fields array");
    let values = json["values"].as_object().expect("values object");

    // All 7 sections declared in env_metadata.
    assert_eq!(sections.len(), 7, "sections: {:#?}", sections);
    // 29 editable keys = all template keys
    assert_eq!(fields.len(), 29, "fields: {}", fields.len());
    // Each field has a value populated from the template.
    for f in fields {
        let key = f["key"].as_str().unwrap();
        assert!(values.contains_key(key), "missing value for {}", key);
    }
    // env_file_path echoed back for UI display.
    assert_eq!(json["env_file_path"], env_path.display().to_string());
}

#[tokio::test]
async fn full_values_reflect_user_overrides_and_template_fallback() {
    let env_path = unique_env_path("full-override");
    // Only override WEB_PORT - every other key must fall back to the template.
    std::fs::write(&env_path, "WEB_PORT=4040\n").unwrap();
    let app = build_router(env_path);

    let json = get_full(app).await;
    let values = &json["values"];
    assert_eq!(values["WEB_PORT"], "4040");
    // UI_DEFAULT_BPM not in the user file → comes from the bundled template.
    assert_eq!(values["UI_DEFAULT_BPM"], "120");
}

#[tokio::test]
async fn full_handles_missing_env_file_via_template() {
    // A brand-new install before first run: file doesn't exist yet. The
    // endpoint must still return a populated form, not 500.
    let env_path = unique_env_path("full-missing");
    assert!(!env_path.exists());
    let app = build_router(env_path);

    let json = get_full(app).await;
    let values = json["values"].as_object().unwrap();
    assert!(
        !values.is_empty(),
        "values must fall back to the bundled template"
    );
    assert_eq!(values["WEB_PORT"], "3030");
}

#[tokio::test]
async fn full_field_kinds_carry_ranges_and_options() {
    let env_path = unique_env_path("full-kinds");
    write_template_to(&env_path);
    let app = build_router(env_path);

    let json = get_full(app).await;
    let fields = json["fields"].as_array().unwrap();

    // WEB_PORT is Integer { 1..=65535 }.
    let web_port = fields
        .iter()
        .find(|f| f["key"] == "WEB_PORT")
        .expect("WEB_PORT field");
    assert_eq!(web_port["kind"], "integer");
    assert_eq!(web_port["min"], 1);
    assert_eq!(web_port["max"], 65535);

    // MIDI_EXPORT_SLIDE_MODE is Enum with three options.
    let slide = fields
        .iter()
        .find(|f| f["key"] == "MIDI_EXPORT_SLIDE_MODE")
        .expect("slide mode field");
    assert_eq!(slide["kind"], "enum");
    let opts = slide["options"].as_array().unwrap();
    assert_eq!(opts.len(), 3);
}

// ── POST /api/config/env ─────────────────────────────────────────────

#[tokio::test]
async fn post_save_persists_changes_to_file() {
    let env_path = unique_env_path("post-save");
    write_template_to(&env_path);
    let app = build_router(env_path.clone());

    let (status, body) = post_json(
        app,
        "/api/config/env",
        serde_json::json!({ "updates": { "WEB_PORT": "4040" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {}", body);
    assert_eq!(body["ok"], true);

    let on_disk = std::fs::read_to_string(&env_path).unwrap();
    assert!(on_disk.contains("WEB_PORT=4040"), "got: {}", on_disk);
    // Template comments preserved.
    assert!(
        on_disk.contains("# "),
        "comments were stripped: {}",
        on_disk
    );
}

#[tokio::test]
async fn post_save_rejects_unknown_key() {
    let env_path = unique_env_path("post-unknown");
    write_template_to(&env_path);
    let original = std::fs::read_to_string(&env_path).unwrap();
    let app = build_router(env_path.clone());

    let (status, body) = post_json(
        app,
        "/api/config/env",
        serde_json::json!({ "updates": { "NOT_IN_TABLE": "x" } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("NOT_IN_TABLE"),
        "error body: {}",
        body
    );

    // File untouched - rejection must happen before apply_updates.
    let after = std::fs::read_to_string(&env_path).unwrap();
    assert_eq!(after, original);
}

#[tokio::test]
async fn post_save_rejects_out_of_range_integer() {
    let env_path = unique_env_path("post-oor");
    write_template_to(&env_path);
    let app = build_router(env_path.clone());

    let (status, body) = post_json(
        app,
        "/api/config/env",
        serde_json::json!({ "updates": { "WEB_PORT": "999999" } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {}", body);
    let on_disk = std::fs::read_to_string(&env_path).unwrap();
    assert!(on_disk.contains("WEB_PORT=3030"));
}

#[tokio::test]
async fn post_save_rejects_invalid_scratch_pattern() {
    let env_path = unique_env_path("post-scratch");
    write_template_to(&env_path);
    let app = build_router(env_path);

    let (status, _body) = post_json(
        app,
        "/api/config/env",
        serde_json::json!({ "updates": { "UI_SCRATCH_PATTERN": "G9-P9Z" } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_save_atomic_all_or_nothing() {
    // One invalid key in a batch - no keys must be written.
    let env_path = unique_env_path("post-atomic");
    write_template_to(&env_path);
    let app = build_router(env_path.clone());

    let (status, _body) = post_json(
        app,
        "/api/config/env",
        serde_json::json!({
            "updates": {
                "WEB_PORT": "4040",
                "MIDI_TIMEOUT_MS": "50"  // below the 100 minimum
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let on_disk = std::fs::read_to_string(&env_path).unwrap();
    // Neither key changed.
    assert!(on_disk.contains("WEB_PORT=3030"), "got: {}", on_disk);
    assert!(!on_disk.contains("WEB_PORT=4040"));
}

#[tokio::test]
async fn post_save_empty_updates_is_ok_noop() {
    let env_path = unique_env_path("post-empty");
    write_template_to(&env_path);
    let original = std::fs::read_to_string(&env_path).unwrap();
    let app = build_router(env_path.clone());

    let (status, body) =
        post_json(app, "/api/config/env", serde_json::json!({ "updates": {} })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);

    // File untouched - no .bak either since apply_updates short-circuits.
    let after = std::fs::read_to_string(&env_path).unwrap();
    assert_eq!(after, original);
}

// ── POST /api/config/env/reset-section ───────────────────────────────

#[tokio::test]
async fn reset_section_restores_template_defaults() {
    let env_path = unique_env_path("reset");
    write_template_to(&env_path);
    // First override WEB_PORT via the normal save path.
    let app = build_router(env_path.clone());
    let (status, _) = post_json(
        app,
        "/api/config/env",
        serde_json::json!({ "updates": { "WEB_PORT": "9999" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(std::fs::read_to_string(&env_path)
        .unwrap()
        .contains("WEB_PORT=9999"));

    // Then reset the whole web_server section.
    let app = build_router(env_path.clone());
    let (status, body) = post_json(
        app,
        "/api/config/env/reset-section",
        serde_json::json!({ "section_id": "web_server" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {}", body);
    assert_eq!(body["ok"], true);

    let after = std::fs::read_to_string(&env_path).unwrap();
    assert!(after.contains("WEB_PORT=3030"), "got: {}", after);
    assert!(!after.contains("WEB_PORT=9999"));
}

#[tokio::test]
async fn reset_section_rejects_unknown_section() {
    let env_path = unique_env_path("reset-unknown");
    write_template_to(&env_path);
    let app = build_router(env_path);

    let (status, body) = post_json(
        app,
        "/api/config/env/reset-section",
        serde_json::json!({ "section_id": "does_not_exist" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap_or("")
        .contains("does_not_exist"));
}

#[tokio::test]
async fn reset_section_only_touches_its_own_keys() {
    let env_path = unique_env_path("reset-scoped");
    write_template_to(&env_path);

    // Override one key in each of two different sections.
    let app = build_router(env_path.clone());
    let (status, _) = post_json(
        app,
        "/api/config/env",
        serde_json::json!({
            "updates": {
                "WEB_PORT": "9999",              // web_server
                "UI_DEFAULT_BPM": "222"          // sequencer
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Reset only the web_server section - UI_DEFAULT_BPM must survive.
    let app = build_router(env_path.clone());
    let (status, _) = post_json(
        app,
        "/api/config/env/reset-section",
        serde_json::json!({ "section_id": "web_server" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let after = std::fs::read_to_string(&env_path).unwrap();
    assert!(after.contains("WEB_PORT=3030"));
    assert!(
        after.contains("UI_DEFAULT_BPM=222"),
        "cross-section bleed: {}",
        after
    );
}
