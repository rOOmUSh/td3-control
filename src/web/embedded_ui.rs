//! Static UI asset serving from a `rust-embed`-generated map.
//!
//! In debug builds the assets are read from the `ui/` folder on disk so
//! frontend edits reload without a rebuild. In release builds the bytes
//! are baked into the binary, allowing single-file distribution with no
//! `ui/` sibling required at runtime.

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "ui/"]
#[exclude = "config/bank-library.sqlite3*"]
#[exclude = "config/bank-library-patterns/*"]
struct UiAssets;

pub async fn serve_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match UiAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.into_owned()))
                .unwrap_or_else(|_| internal_error())
        }
        None => not_found(),
    }
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "asset not found").into_response()
}

fn internal_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "asset response error").into_response()
}

pub fn read_text(path: &str) -> Option<String> {
    let asset = UiAssets::get(path)?;
    String::from_utf8(asset.data.into_owned()).ok()
}
