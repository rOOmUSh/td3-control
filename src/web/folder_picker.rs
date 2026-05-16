use std::path::PathBuf;

use thiserror::Error;

pub(crate) type FolderPickerResult = Result<Option<PathBuf>, FolderPickerError>;

pub(crate) trait FolderPicker: Send + 'static {
    fn pick_folder(self) -> FolderPickerResult;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum FolderPickerError {
    #[error("native folder picker is not available: {reason}")]
    Unsupported { reason: String },

    #[cfg(test)]
    #[error("native folder picker failed: {message}")]
    Dialog { message: String },
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NativeFolderPicker;

impl FolderPicker for NativeFolderPicker {
    fn pick_folder(self) -> FolderPickerResult {
        if let Some(reason) = unsupported_desktop_reason() {
            return Err(FolderPickerError::Unsupported { reason });
        }
        Ok(rfd::FileDialog::new()
            .set_title("Select folder")
            .pick_folder())
    }
}

pub(crate) fn pick_folder() -> FolderPickerResult {
    NativeFolderPicker.pick_folder()
}

impl<F> FolderPicker for F
where
    F: FnOnce() -> FolderPickerResult + Send + 'static,
{
    fn pick_folder(self) -> FolderPickerResult {
        self()
    }
}

#[cfg(target_os = "linux")]
fn unsupported_desktop_reason() -> Option<String> {
    if std::env::var_os("DISPLAY").is_some()
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var_os("XDG_CURRENT_DESKTOP").is_some()
        || std::env::var_os("DESKTOP_SESSION").is_some()
    {
        None
    } else {
        Some("no desktop session was detected".to_string())
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn unsupported_desktop_reason() -> Option<String> {
    None
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn unsupported_desktop_reason() -> Option<String> {
    Some("this platform is not supported".to_string())
}
