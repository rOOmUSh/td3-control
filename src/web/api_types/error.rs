use serde::{Deserialize, Serialize};

// Generic error
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}
