use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::web::api_types::SaveConfigResponse;
use crate::web::state::{ConfigState, UiConfigSnapshot};

fn temp_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "td3-config-http-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = temp_path(tag);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn build_config_router(config_dir: PathBuf) -> axum::Router {
    use crate::web::handlers;
    use axum::routing::{get, post};

    let state = ConfigState {
        ui_config: UiConfigSnapshot::for_tests(),
        env_file_path: std::path::PathBuf::from("TD3_CONFIG.env"),
        user_config_dir: config_dir,
    };

    axum::Router::new()
        .route("/api/config/keyboard", get(handlers::get_keyboard_config))
        .route("/api/config/keyboard", post(handlers::save_keyboard_config))
        .route("/api/config/scales", get(handlers::get_scales_config))
        .route("/api/config/scales", post(handlers::save_scales_config))
        .route(
            "/api/config/progression",
            get(handlers::get_progression_config),
        )
        .route(
            "/api/config/progression",
            post(handlers::save_progression_config),
        )
        .with_state(state)
}

async fn post_json(app: axum::Router, uri: &str, body: String) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

fn keyboard_defaults_body() -> String {
    crate::web::embedded_ui::read_text("config/keyboard-defaults.json").unwrap()
}

#[tokio::test]
async fn config_routes_do_not_require_app_state() {
    let app = build_config_router(temp_dir("focused-state"));

    let (status, body) = get_json(app, "/api/config/keyboard").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_object());
}

#[tokio::test]
async fn keyboard_config_malformed_shape_returns_400() {
    let app = build_config_router(temp_dir("keyboard-shape"));

    let (status, body) = post_json(
        app,
        "/api/config/keyboard",
        r#"{"notes":[],"actions":{}}"#.to_string(),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("invalid keyboard config JSON"));
}

#[tokio::test]
async fn scales_config_invalid_semantic_value_returns_400() {
    let app = build_config_router(temp_dir("scales-semantic"));
    let body = json!({
        "tag_groups": [{ "label": "Safe", "tag": "safe" }],
        "scales": [{
            "id": "major",
            "name": "Major",
            "intervals": [0, 0, 4],
            "tags": ["safe"]
        }]
    });

    let (status, body) = post_json(app, "/api/config/scales", body.to_string()).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("intervals contain duplicates"));
}

#[tokio::test]
async fn valid_scales_config_persists_normalized_json() {
    let dir = temp_dir("scales-normalized");
    let app = build_config_router(dir.clone());
    let body = json!({
        "tag_groups": [
            { "label": "Safe", "tag": "safe" },
            { "label": "Dark", "tag": "dark" }
        ],
        "scales": [{
            "id": "custom_scale",
            "name": "Custom Scale",
            "intervals": [7, 0, 3],
            "tags": ["dark", "safe"]
        }]
    });

    let (status, body) = post_json(app, "/api/config/scales", body.to_string()).await;

    assert_eq!(status, StatusCode::OK);
    let save: SaveConfigResponse = serde_json::from_value(body).unwrap();
    assert!(save.ok);

    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("scales-config.json")).unwrap())
            .unwrap();
    assert_eq!(saved["scales"][0]["intervals"], json!([0, 3, 7]));
    assert_eq!(saved["scales"][0]["tags"], json!(["safe", "dark"]));

    let app = build_config_router(dir);
    let (status, reloaded) = get_json(app, "/api/config/scales").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reloaded, saved);
}

#[tokio::test]
async fn keyboard_config_write_failure_returns_500() {
    let blocker = temp_path("config-dir-file");
    let _ = std::fs::remove_dir_all(&blocker);
    std::fs::write(&blocker, "not a directory").unwrap();
    let app = build_config_router(blocker.clone());

    let (status, body) = post_json(app, "/api/config/keyboard", keyboard_defaults_body()).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body["error"].as_str().unwrap().contains("failed to create"));
    assert_eq!(
        std::fs::read_to_string(&blocker).unwrap(),
        "not a directory"
    );
}

#[tokio::test]
async fn progression_config_defaults_load_through_api() {
    let app = build_config_router(temp_dir("progression-defaults"));

    let (status, body) = get_json(app, "/api/config/progression").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["anchor_steps"], json!([0, 4, 8, 12]));
    assert_eq!(body["default_timeline"].as_array().unwrap().len(), 16);
}
