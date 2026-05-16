//! Tests for the embedded UI asset handler.
//!
//! These cover both the disk-backed debug build behavior and the
//! compile-time embedded release build behavior, which `rust-embed`
//! switches between transparently.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::web::embedded_ui;

fn build_app() -> Router {
    Router::new().fallback(embedded_ui::serve_asset)
}

#[tokio::test]
async fn root_path_resolves_to_index_html() {
    let app = build_app();
    let req = Request::builder().uri("/").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.starts_with("text/html"),
        "expected text/html, got {}",
        ct
    );
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert!(!body.is_empty(), "index.html should not be empty");
}

#[tokio::test]
async fn explicit_index_path_returns_html() {
    let app = build_app();
    let req = Request::builder()
        .uri("/index.html")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.starts_with("text/html"),
        "expected text/html, got {}",
        ct
    );
}

#[tokio::test]
async fn js_asset_returns_javascript_content_type() {
    let app = build_app();
    let req = Request::builder()
        .uri("/js/main.js")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("javascript") || ct.contains("ecmascript"),
        "expected javascript content-type, got {}",
        ct
    );
}

#[tokio::test]
async fn css_asset_returns_css_content_type() {
    let app = build_app();
    let req = Request::builder()
        .uri("/css/bank.css")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("css"), "expected css content-type, got {}", ct);
}

#[tokio::test]
async fn tailwind_asset_returns_css_content_type() {
    let app = build_app();
    let req = Request::builder()
        .uri("/css/tailwind.css")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("css"), "expected css content-type, got {}", ct);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert!(!body.is_empty(), "tailwind.css should not be empty");
    let css = std::str::from_utf8(&body).unwrap_or("");
    assert!(
        !css.contains("@tailwind"),
        "tailwind.css should contain generated CSS, not raw Tailwind directives"
    );
    assert!(
        css.contains("--tw-ring-offset-shadow"),
        "tailwind.css should include Tailwind generated output"
    );
}

#[test]
fn html_uses_local_tailwind_css() {
    for path in [
        "index.html",
        "progression.html",
        "bank.html",
        "settings.html",
    ] {
        let body = embedded_ui::read_text(path).unwrap_or_else(|| panic!("embed missing {}", path));
        assert!(
            body.contains("css/tailwind.css"),
            "{} should link local tailwind.css",
            path
        );
        assert!(
            !body.contains("cdn.tailwindcss.com"),
            "{} should not load the Tailwind CDN",
            path
        );
        assert!(
            !body.contains("tailwind.config"),
            "{} should not configure browser Tailwind",
            path
        );
    }
}

#[tokio::test]
async fn unknown_path_returns_404() {
    let app = build_app();
    let req = Request::builder()
        .uri("/this/path/does/not/exist.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn read_text_returns_some_for_existing_html() {
    let content = embedded_ui::read_text("index.html");
    assert!(
        content.is_some(),
        "index.html should be readable from embed"
    );
    let body = content.unwrap();
    assert!(!body.is_empty(), "index.html body should not be empty");
}

#[test]
fn read_text_returns_none_for_missing_path() {
    let content = embedded_ui::read_text("does/not/exist.html");
    assert!(content.is_none());
}

#[test]
fn embedded_defaults_for_settings_handlers_exist_and_parse() {
    for name in ["keyboard", "scales", "progression"] {
        let asset = format!("config/{}-defaults.json", name);
        let raw =
            embedded_ui::read_text(&asset).unwrap_or_else(|| panic!("embed missing {}", asset));
        let val: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{} not valid JSON: {}", asset, e));
        assert!(val.is_object(), "{} must be a JSON object", asset);
    }
}
