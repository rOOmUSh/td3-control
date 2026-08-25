//! Launcher window icon.
//!
//! `build.rs` decodes `ui/img/td3-control.png` into raw RGBA and emits
//! the `WIDTH`, `HEIGHT` and `RGBA` constants included below, so the
//! window icon, the browser tab icon and the executable resource icon
//! are all the same artwork with no runtime PNG decoding.

include!(concat!(env!("OUT_DIR"), "/launcher_icon.rs"));

/// The window icon for the launcher viewport.
pub fn icon_data() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: RGBA.to_vec(),
        width: WIDTH,
        height: HEIGHT,
    }
}
