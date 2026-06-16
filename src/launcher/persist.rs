//! Persistence helpers for launcher-controlled startup settings.

use std::collections::HashMap;
use std::path::Path;

use crate::env_metadata;
use crate::env_writer;
use crate::error::Td3Error;

use super::child_args::LauncherMidiChoice;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherPersistChoice {
    pub scratch: String,
    pub web_port: u16,
    pub midi: LauncherMidiChoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherPersistReport {
    pub midi_persisted: bool,
}

pub fn build_updates(choice: &LauncherPersistChoice) -> Result<HashMap<String, String>, Td3Error> {
    let mut updates = HashMap::new();
    updates.insert("UI_SCRATCH_PATTERN".to_string(), choice.scratch.clone());
    updates.insert("WEB_PORT".to_string(), choice.web_port.to_string());

    if let LauncherMidiChoice::ExactPair { input, output } = &choice.midi {
        if input == output {
            updates.insert("MIDI_PORT_SUBSTRING".to_string(), input.clone());
            updates.insert("MIDI_STRICT_NAME_MATCH".to_string(), "1".to_string());
        }
    }

    for (key, value) in &updates {
        env_metadata::validate_value(key, value)?;
    }

    Ok(updates)
}

pub fn persist_launcher_choice(
    env_path: &Path,
    choice: &LauncherPersistChoice,
) -> Result<LauncherPersistReport, Td3Error> {
    let updates = build_updates(choice)?;
    env_writer::apply_updates(env_path, &updates)?;
    Ok(LauncherPersistReport {
        midi_persisted: updates.contains_key("MIDI_PORT_SUBSTRING"),
    })
}
