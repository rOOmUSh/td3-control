use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyboardConfig {
    pub notes: BTreeMap<String, String>,
    pub actions: BTreeMap<String, String>,
}

impl UserConfigFile for KeyboardConfig {
    const NAME: &'static str = "keyboard";

    fn validate_and_normalize(&mut self) -> Result<(), ConfigValidationError> {
        validate_required_key_set("keyboard notes", &self.notes, &KEYBOARD_NOTE_KEYS)?;
        validate_required_key_set("keyboard actions", &self.actions, &KEYBOARD_ACTION_KEYS)?;

        let mut used = BTreeMap::new();
        for (section, entries) in [("notes", &self.notes), ("actions", &self.actions)] {
            for (name, binding) in entries {
                validate_key_binding(section, name, binding)?;
                let identity = key_binding_identity(binding);
                let current = format!("{}.{}", section, name);
                if let Some(previous) = used.insert(identity, current.clone()) {
                    return Err(ConfigValidationError::new(format!(
                        "keyboard binding '{}' is used by {} and {}",
                        binding, previous, current
                    )));
                }
            }
        }

        Ok(())
    }
}

const KEYBOARD_NOTE_KEYS: [&str; 13] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C^",
];

const KEYBOARD_ACTION_KEYS: [&str; 11] = [
    "accent",
    "slide",
    "transpose_up",
    "transpose_down",
    "prev_step",
    "next_step",
    "rest",
    "rest_alt",
    "randomize",
    "play",
    "live_toggle",
];

fn validate_required_key_set(
    label: &str,
    map: &BTreeMap<String, String>,
    required: &[&str],
) -> Result<(), ConfigValidationError> {
    let allowed: BTreeSet<&str> = required.iter().copied().collect();

    for key in required {
        if !map.contains_key(*key) {
            return Err(ConfigValidationError::new(format!(
                "{} missing '{}'",
                label, key
            )));
        }
    }

    for key in map.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(ConfigValidationError::new(format!(
                "{} contains unknown key '{}'",
                label, key
            )));
        }
    }

    Ok(())
}

fn validate_key_binding(
    section: &str,
    name: &str,
    binding: &str,
) -> Result<(), ConfigValidationError> {
    if binding.is_empty() {
        return Err(ConfigValidationError::new(format!(
            "keyboard {}.{} binding must not be empty",
            section, name
        )));
    }
    if binding != " " && binding.trim().is_empty() {
        return Err(ConfigValidationError::new(format!(
            "keyboard {}.{} binding must not be only whitespace",
            section, name
        )));
    }
    Ok(())
}

fn key_binding_identity(binding: &str) -> String {
    binding.to_ascii_lowercase()
}
