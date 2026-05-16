use serde::{Deserialize, Serialize};

use crate::library::model::LibraryItem;

use super::super::WebPattern;

#[derive(Serialize, Deserialize)]
pub struct BankItemsResponse {
    pub items: Vec<LibraryItem>,
    pub total: u32,
}

#[derive(Serialize, Deserialize)]
pub struct BankItemResponse {
    pub item: LibraryItem,
}

#[derive(Serialize, Deserialize)]
pub struct ItemPatternResponse {
    pub item_id: String,
    pub pattern: WebPattern,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteBankItemResponse {
    pub item_id: String,
    pub deleted: bool,
}

#[derive(Deserialize)]
pub struct FavoriteRequest {
    pub favorite: bool,
}

#[derive(Deserialize)]
pub struct ArchiveRequest {
    pub archived: bool,
}

#[derive(Serialize, Deserialize)]
pub struct BankItemFlagResponse {
    pub item_id: String,
    pub favorite: Option<bool>,
    pub archived: Option<bool>,
}
