use super::validation::{validate_identifier, validate_non_empty};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScaleTag {
    Safe,
    Dark,
    Tension,
    Jazz,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScaleTagGroup {
    pub label: String,
    pub tag: ScaleTag,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScaleDefinition {
    pub id: String,
    pub name: String,
    pub intervals: Vec<u8>,
    pub tags: Vec<ScaleTag>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScalesConfig {
    pub tag_groups: Vec<ScaleTagGroup>,
    pub scales: Vec<ScaleDefinition>,
}

impl UserConfigFile for ScalesConfig {
    const NAME: &'static str = "scales";

    fn validate_and_normalize(&mut self) -> Result<(), ConfigValidationError> {
        if self.tag_groups.is_empty() {
            return Err(ConfigValidationError::new(
                "scales tag_groups must not be empty",
            ));
        }
        if self.scales.is_empty() {
            return Err(ConfigValidationError::new("scales must not be empty"));
        }

        let mut declared_tags = BTreeSet::new();
        for group in &self.tag_groups {
            validate_non_empty("tag group label", &group.label)?;
            if !declared_tags.insert(group.tag.clone()) {
                return Err(ConfigValidationError::new(format!(
                    "duplicate tag group '{}'",
                    group.tag.as_str()
                )));
            }
        }

        let mut ids = BTreeSet::new();
        for scale in &mut self.scales {
            validate_identifier("scale id", &scale.id)?;
            validate_non_empty("scale name", &scale.name)?;
            if !ids.insert(scale.id.clone()) {
                return Err(ConfigValidationError::new(format!(
                    "duplicate scale id '{}'",
                    scale.id
                )));
            }

            normalize_intervals(&scale.id, &mut scale.intervals)?;
            normalize_scale_tags(&scale.id, &declared_tags, &mut scale.tags)?;
        }

        Ok(())
    }
}

impl ScaleTag {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Dark => "dark",
            Self::Tension => "tension",
            Self::Jazz => "jazz",
            Self::Custom => "custom",
        }
    }
}

fn normalize_intervals(
    scale_id: &str,
    intervals: &mut Vec<u8>,
) -> Result<(), ConfigValidationError> {
    if intervals.is_empty() {
        return Err(ConfigValidationError::new(format!(
            "scale '{}' intervals must not be empty",
            scale_id
        )));
    }
    if intervals.iter().any(|v| *v > 11) {
        return Err(ConfigValidationError::new(format!(
            "scale '{}' intervals must be 0-11",
            scale_id
        )));
    }

    intervals.sort_unstable();
    let original_len = intervals.len();
    intervals.dedup();
    if intervals.len() != original_len {
        return Err(ConfigValidationError::new(format!(
            "scale '{}' intervals contain duplicates",
            scale_id
        )));
    }
    if !intervals.contains(&0) {
        return Err(ConfigValidationError::new(format!(
            "scale '{}' intervals must include 0",
            scale_id
        )));
    }

    Ok(())
}

fn normalize_scale_tags(
    scale_id: &str,
    declared_tags: &BTreeSet<ScaleTag>,
    tags: &mut Vec<ScaleTag>,
) -> Result<(), ConfigValidationError> {
    if tags.is_empty() {
        return Err(ConfigValidationError::new(format!(
            "scale '{}' tags must not be empty",
            scale_id
        )));
    }

    tags.sort();
    let original_len = tags.len();
    tags.dedup();
    if tags.len() != original_len {
        return Err(ConfigValidationError::new(format!(
            "scale '{}' tags contain duplicates",
            scale_id
        )));
    }

    for tag in tags.iter() {
        if !declared_tags.contains(tag) {
            return Err(ConfigValidationError::new(format!(
                "scale '{}' references undeclared tag '{}'",
                scale_id,
                tag.as_str()
            )));
        }
    }

    Ok(())
}
