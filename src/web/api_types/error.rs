use serde::{Deserialize, Serialize};

// Generic error
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}
