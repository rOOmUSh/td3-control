use serde::{Deserialize, Serialize};

use crate::library::model::Tag;

#[derive(Serialize, Deserialize)]
pub struct TagsResponse {
    pub tags: Vec<Tag>,
}

#[derive(Deserialize)]
pub struct AddTagRequest {
    pub label: String,
}

#[derive(Serialize, Deserialize)]
pub struct TagOpResponse {
    pub ok: bool,
}

#[derive(Deserialize)]
pub struct BulkTagRequest {
    pub item_ids: Vec<String>,
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}
