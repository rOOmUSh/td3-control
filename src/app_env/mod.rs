//! Central runtime configuration loaded from `TD3_CONFIG.env`.
//!
//! Layering: CLI flag > `TD3_CONFIG.env` > bundled template (`config/default_env.template`).
//!
//! This module owns file loading and typed runtime configuration. Consumers read
//! fields from `AppEnv` to resolve their defaults.

mod keys;
mod model;
mod parse;
mod template;
mod values;

pub use model::AppEnv;
pub use template::{CONFIG_FILE_PATH, DEFAULT_TEMPLATE};

/// Scale-id normalizer: trim, lowercase, spaces to underscores.
pub fn normalize_scale_id(raw: &str) -> String {
    raw.trim().to_lowercase().replace(' ', "_")
}
