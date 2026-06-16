//! GUI launcher used when the binary is invoked with no subcommand.
//!
//! Pops a small egui window letting the user pick a scratch pattern slot
//! (G/P/A-B), see live MIDI status, optionally persist the selection to
//! `TD3_CONFIG.env`, and read the CLI help inline.

pub mod app;
pub(crate) mod child_args;
pub mod choice;
pub(crate) mod device_options;
pub mod help_text;
pub mod midi_probe;
pub(crate) mod persist;
pub(crate) mod process;
pub mod selection;
pub(crate) mod startup_state;
pub(crate) mod view;
pub(crate) mod web_port;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::app_env::AppEnv;
use crate::error::Td3Error;

pub use choice::LauncherChoice;

/// Open the launcher window. Blocks until the user clicks Start (which
/// spawns the control-mode child process and exits this process via
/// `std::process::exit(0)`), Cancel (also exits), or closes the window
/// via the X button (returns `Ok(None)` so main can exit cleanly).
///
/// The Start path uses `process::exit` rather than returning normally
/// because eframe 0.29 + glow + winit 0.30's `run_app_on_demand` does
/// not reliably tear down on Windows once a close has been requested
/// from inside `App::update`. Spawning a fresh child sidesteps the
/// teardown entirely - the OS kills the launcher process and destroys
/// the GL context, then the child runs the CLI/web server cleanly.
pub fn run(env: &AppEnv, env_path: PathBuf) -> Result<Option<LauncherChoice>, Td3Error> {
    let initial =
        selection::SelectionState::from_label(&env.ui_scratch_pattern).unwrap_or_default();
    let help_text = help_text::full_help();
    let outcome = Arc::new(Mutex::new(choice::LauncherOutcome::default()));
    let outcome_for_app = outcome.clone();

    let midi_substring = env.midi_port_substring.clone();
    let midi_strict = env.midi_strict_name_match;
    let web_port = env.web_port;
    let web_bind = env.web_bind.clone();

    let viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([720.0, 640.0])
        .with_min_inner_size([560.0, 540.0])
        .with_title("TD-3 Control Launcher");
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "TD-3 Control Launcher",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(app::LauncherApp::new(app::LauncherAppConfig {
                initial,
                midi_substring,
                midi_strict,
                web_port,
                web_bind,
                help_text,
                outcome: outcome_for_app,
                env_path,
            })))
        }),
    )
    .map_err(|e| Td3Error::Other(format!("launcher GUI failed: {}", e)))?;

    let final_outcome = match outcome.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => {
            log::warn!("launcher outcome mutex poisoned, recovering stored value");
            poisoned.into_inner().clone()
        }
    };
    Ok(final_outcome.0)
}
