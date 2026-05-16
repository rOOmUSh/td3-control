use std::path::PathBuf;

use super::*;
use crate::web::folder_picker::{pick_folder, FolderPicker, FolderPickerResult};

pub(super) async fn browse_folder(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<BrowseFolderResponse>, AppError> {
    browse_folder_with_picker(pick_folder).await
}

pub(crate) async fn browse_folder_with_picker<P>(
    picker: P,
) -> Result<Json<BrowseFolderResponse>, AppError>
where
    P: FolderPicker,
{
    let picked = tokio::task::spawn_blocking(move || picker.pick_folder())
        .await
        .map_err(|err| AppError::BadRequest(format!("browse-folder task failed: {}", err)))?;
    folder_picker_response(picked).map(Json)
}

fn folder_picker_response(picked: FolderPickerResult) -> Result<BrowseFolderResponse, AppError> {
    match picked {
        Ok(path) => Ok(BrowseFolderResponse {
            path: path.map(path_to_string),
        }),
        Err(err) => Err(AppError::BadRequest(err.to_string())),
    }
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}
