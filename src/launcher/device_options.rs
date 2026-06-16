//! Pure MIDI port selection helpers for the launcher.

use std::collections::BTreeSet;

use super::midi_probe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigPortSelection {
    None,
    Single(String),
    Ambiguous(Vec<String>),
}

impl ConfigPortSelection {
    pub fn selected_name(&self) -> Option<&str> {
        match self {
            Self::Single(name) => Some(name.as_str()),
            Self::None | Self::Ambiguous(_) => None,
        }
    }

    pub fn message(&self, label: &str) -> Option<String> {
        match self {
            Self::None => None,
            Self::Single(name) => Some(format!("Configured {} port: {}", label, name)),
            Self::Ambiguous(names) => Some(format!(
                "Configured {} port is ambiguous: {}",
                label,
                names.join(", ")
            )),
        }
    }
}

pub fn select_configured_port(names: &[String], query: &str, strict: bool) -> ConfigPortSelection {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return ConfigPortSelection::None;
    }

    let matches = names
        .iter()
        .filter(|name| midi_probe::matches(name, trimmed, strict))
        .cloned()
        .collect::<Vec<String>>();

    match matches.len() {
        0 => ConfigPortSelection::None,
        1 => ConfigPortSelection::Single(matches[0].clone()),
        _ => {
            let unique = matches.into_iter().collect::<BTreeSet<String>>();
            ConfigPortSelection::Ambiguous(unique.into_iter().collect())
        }
    }
}

pub fn exact_name_present(names: &[String], selected: &str) -> bool {
    names.iter().any(|name| name == selected)
}
