use super::*;

pub(super) fn validate_non_empty(label: &str, value: &str) -> Result<(), ConfigValidationError> {
    if value.trim().is_empty() {
        return Err(ConfigValidationError::new(format!(
            "{} must not be empty",
            label
        )));
    }
    Ok(())
}

pub(super) fn validate_identifier(label: &str, value: &str) -> Result<(), ConfigValidationError> {
    validate_non_empty(label, value)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(ConfigValidationError::new(format!(
            "{} must not be empty",
            label
        )));
    };
    if !first.is_ascii_lowercase() {
        return Err(ConfigValidationError::new(format!(
            "{} '{}' must start with a lowercase ASCII letter",
            label, value
        )));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(ConfigValidationError::new(format!(
            "{} '{}' must contain only lowercase ASCII letters, digits, and underscores",
            label, value
        )));
    }
    Ok(())
}
