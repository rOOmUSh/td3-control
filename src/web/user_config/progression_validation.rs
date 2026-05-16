use super::validation::validate_identifier;
use super::*;

pub(super) fn validate_progression_config(
    config: &ProgressionConfig,
) -> Result<(), ConfigValidationError> {
    validate_anchor_steps(&config.anchor_steps)?;
    validate_profile_priority(&config.profile_priority)?;
    validate_scale_profiles(&config.scale_profiles)?;
    validate_presets(&config.presets)?;
    validate_mutation(&config.mutation)?;
    validate_default_timeline(&config.default_timeline)?;
    Ok(())
}

fn validate_anchor_steps(values: &[u8]) -> Result<(), ConfigValidationError> {
    if values.len() != 4 {
        return Err(ConfigValidationError::new(format!(
            "anchor_steps must contain 4 values, got {}",
            values.len()
        )));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        if *value > 15 {
            return Err(ConfigValidationError::new(format!(
                "anchor step must be 0-15, got {}",
                value
            )));
        }
        if !seen.insert(*value) {
            return Err(ConfigValidationError::new(format!(
                "duplicate anchor step {}",
                value
            )));
        }
    }
    Ok(())
}

fn validate_profile_priority(values: &[ProgressionProfile]) -> Result<(), ConfigValidationError> {
    if values.len() != PROGRESSION_PROFILES.len() {
        return Err(ConfigValidationError::new(format!(
            "profile_priority must contain {} profiles",
            PROGRESSION_PROFILES.len()
        )));
    }

    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(*value) {
            return Err(ConfigValidationError::new(format!(
                "duplicate profile_priority '{}'",
                value.as_str()
            )));
        }
    }
    for profile in PROGRESSION_PROFILES {
        if !seen.contains(&profile) {
            return Err(ConfigValidationError::new(format!(
                "profile_priority missing '{}'",
                profile.as_str()
            )));
        }
    }
    Ok(())
}

fn validate_scale_profiles(
    values: &BTreeMap<String, ProgressionProfile>,
) -> Result<(), ConfigValidationError> {
    if values.is_empty() {
        return Err(ConfigValidationError::new(
            "scale_profiles must not be empty",
        ));
    }
    for scale_id in values.keys() {
        validate_identifier("scale profile id", scale_id)?;
    }
    Ok(())
}

fn validate_presets(
    values: &BTreeMap<ProgressionProfile, Vec<Vec<u8>>>,
) -> Result<(), ConfigValidationError> {
    for profile in PROGRESSION_PROFILES {
        let Some(profile_presets) = values.get(&profile) else {
            return Err(ConfigValidationError::new(format!(
                "presets missing '{}'",
                profile.as_str()
            )));
        };
        if profile_presets.is_empty() {
            return Err(ConfigValidationError::new(format!(
                "presets '{}' must not be empty",
                profile.as_str()
            )));
        }

        let mut seen = BTreeSet::new();
        for degrees in profile_presets {
            if degrees.len() != 4 {
                return Err(ConfigValidationError::new(format!(
                    "preset '{}' degree list must contain 4 values",
                    profile.as_str()
                )));
            }
            for degree in degrees {
                if *degree == 0 || *degree > 12 {
                    return Err(ConfigValidationError::new(format!(
                        "preset '{}' degree must be 1-12, got {}",
                        profile.as_str(),
                        degree
                    )));
                }
            }
            if !seen.insert(degrees.clone()) {
                return Err(ConfigValidationError::new(format!(
                    "duplicate preset for '{}'",
                    profile.as_str()
                )));
            }
        }
    }

    for profile in values.keys() {
        if !PROGRESSION_PROFILES.contains(profile) {
            return Err(ConfigValidationError::new(format!(
                "unknown preset profile '{}'",
                profile.as_str()
            )));
        }
    }

    Ok(())
}

fn validate_mutation(value: &ProgressionMutation) -> Result<(), ConfigValidationError> {
    if value.max_changes > 16 {
        return Err(ConfigValidationError::new(format!(
            "mutation max_changes must be 0-16, got {}",
            value.max_changes
        )));
    }
    if value.min_changes > value.target_changes || value.target_changes > value.max_changes {
        return Err(ConfigValidationError::new(
            "mutation changes must satisfy min_changes <= target_changes <= max_changes",
        ));
    }
    validate_ratio("mutation rhythm_preserve", value.rhythm_preserve)?;
    validate_ratio("mutation contour_preserve", value.contour_preserve)?;
    Ok(())
}

fn validate_ratio(label: &str, value: f64) -> Result<(), ConfigValidationError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(ConfigValidationError::new(format!(
            "{} must be 0.0-1.0, got {}",
            label, value
        )));
    }
    Ok(())
}

fn validate_default_timeline(values: &[u8]) -> Result<(), ConfigValidationError> {
    if values.len() != 16 {
        return Err(ConfigValidationError::new(format!(
            "default_timeline must contain 16 values, got {}",
            values.len()
        )));
    }
    for value in values {
        if !(1..=4).contains(value) {
            return Err(ConfigValidationError::new(format!(
                "default_timeline values must be 1-4, got {}",
                value
            )));
        }
    }
    Ok(())
}

const PROGRESSION_PROFILES: [ProgressionProfile; 4] = [
    ProgressionProfile::Safe,
    ProgressionProfile::Dark,
    ProgressionProfile::Tension,
    ProgressionProfile::Jazz,
];
