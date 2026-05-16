use super::*;

impl LibraryStore {
    /// Toggle favorite. Returns the new value, or `None` if the item is gone.
    pub fn set_favorite(&self, id: &str, favorite: bool) -> Result<Option<bool>, Td3Error> {
        let mut existing = match persistence::get_item(&self.path, id)? {
            Some(it) => it,
            None => return Ok(None),
        };
        existing.favorite = favorite;
        persistence::upsert_item(&self.path, &existing)?;
        {
            let mut data = self.write_data()?;
            if let Some(slot) = data.items.iter_mut().find(|i| i.item_id == id) {
                slot.favorite = favorite;
            }
        }
        Ok(Some(favorite))
    }
}
