use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod keyboard;
mod progression;
mod progression_validation;
mod scales;
mod validation;

pub(crate) use keyboard::KeyboardConfig;
#[allow(unused_imports)]
pub(crate) use progression::{
    AnchorPriority, BodyPriority, ProgressionConfig, ProgressionMutation, ProgressionProfile,
    ProgressionRangePolicy, RangeMode,
};
#[allow(unused_imports)]
pub(crate) use scales::{ScaleDefinition, ScaleTag, ScaleTagGroup, ScalesConfig};

#[derive(Debug, Error)]
#[error("{0}")]
pub(crate) struct ConfigValidationError(String);

impl ConfigValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

pub(crate) trait UserConfigFile: Serialize {
    const NAME: &'static str;

    fn validate_and_normalize(&mut self) -> Result<(), ConfigValidationError>;
}
