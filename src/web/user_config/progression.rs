use super::progression_validation::validate_progression_config;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProgressionProfile {
    Safe,
    Dark,
    Tension,
    Jazz,
}

impl ProgressionProfile {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Dark => "dark",
            Self::Tension => "tension",
            Self::Jazz => "jazz",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RangeMode {
    FoldThenClamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnchorPriority {
    TargetPitchClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BodyPriority {
    ContourThenTarget,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgressionMutation {
    pub target_changes: u8,
    pub min_changes: u8,
    pub max_changes: u8,
    pub rhythm_preserve: f64,
    pub contour_preserve: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgressionRangePolicy {
    pub mode: RangeMode,
    pub anchor_priority: AnchorPriority,
    pub body_priority: BodyPriority,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgressionConfig {
    pub anchor_steps: Vec<u8>,
    pub profile_priority: Vec<ProgressionProfile>,
    pub scale_profiles: BTreeMap<String, ProgressionProfile>,
    pub presets: BTreeMap<ProgressionProfile, Vec<Vec<u8>>>,
    pub mutation: ProgressionMutation,
    pub range_policy: ProgressionRangePolicy,
    pub default_timeline: Vec<u8>,
}

impl UserConfigFile for ProgressionConfig {
    const NAME: &'static str = "progression";

    fn validate_and_normalize(&mut self) -> Result<(), ConfigValidationError> {
        validate_progression_config(self)
    }
}
