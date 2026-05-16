use std::path::PathBuf;

use axum::Json;

use crate::web::bank_handlers::browse_folder_with_picker;
use crate::web::folder_picker::{FolderPickerError, FolderPickerResult};
use crate::web::handlers::AppError;

#[tokio::test]
async fn folder_picker_cancel_returns_none_not_error() {
    let Json(resp) = browse_folder_with_picker(|| -> FolderPickerResult { Ok(None) })
        .await
        .expect("cancel should be a successful empty response");
    assert_eq!(resp.path, None);
}

#[tokio::test]
async fn folder_picker_selected_path_returns_path() {
    let selected = PathBuf::from("selected-folder");
    let expected = selected.to_string_lossy().into_owned();
    let Json(resp) =
        browse_folder_with_picker(move || -> FolderPickerResult { Ok(Some(selected)) })
            .await
            .expect("selected folder should be returned");
    assert_eq!(resp.path.as_deref(), Some(expected.as_str()));
}

#[tokio::test]
async fn folder_picker_headless_mode_returns_clean_error() {
    let result = browse_folder_with_picker(|| -> FolderPickerResult {
        Err(FolderPickerError::Unsupported {
            reason: "no desktop session was detected".to_string(),
        })
    })
    .await;

    assert!(matches!(
        result,
        Err(AppError::BadRequest(ref msg))
            if msg.contains("native folder picker is not available")
                && msg.contains("no desktop session was detected")
    ));
}

#[tokio::test]
async fn folder_picker_dialog_failure_returns_bad_request() {
    let result = browse_folder_with_picker(|| -> FolderPickerResult {
        Err(FolderPickerError::Dialog {
            message: "dialog backend failed".to_string(),
        })
    })
    .await;

    assert!(matches!(
        result,
        Err(AppError::BadRequest(ref msg))
            if msg.contains("native folder picker failed")
                && msg.contains("dialog backend failed")
    ));
}
