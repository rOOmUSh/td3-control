//! Launcher start and cancel outcome types.

use std::sync::{Arc, Mutex};

use super::child_args::LauncherMidiChoice;

/// Result returned to the caller once the launcher window closes.
#[derive(Debug, Clone)]
pub struct LauncherChoice {
    pub scratch: String,
    pub persist: bool,
    pub midi: LauncherMidiChoice,
    pub web_port: u16,
}

#[derive(Debug, Clone, Default)]
pub struct LauncherOutcome(pub Option<LauncherChoice>);

pub(crate) fn store_outcome(
    outcome: &Arc<Mutex<LauncherOutcome>>,
    choice: Option<LauncherChoice>,
) -> bool {
    if let Ok(mut guard) = outcome.lock() {
        *guard = LauncherOutcome(choice);
        true
    } else {
        false
    }
}
