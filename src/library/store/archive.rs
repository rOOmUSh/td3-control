use super::*;

impl LibraryStore {
    pub fn set_archived(&self, id: &str, archived: bool) -> Result<Option<bool>, Td3Error> {
        let mut existing = match persistence::get_item(&self.path, id)? {
            Some(it) => it,
            None => return Ok(None),
        };
        existing.archived = archived;
        persistence::upsert_item(&self.path, &existing)?;
        {
            let mut data = self.write_data()?;
            if let Some(slot) = data.items.iter_mut().find(|i| i.item_id == id) {
                slot.archived = archived;
            }
        }
        Ok(Some(archived))
    }
}
