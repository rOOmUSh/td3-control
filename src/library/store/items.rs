use super::*;

impl LibraryStore {
    // ------------------------------------------------------------------
    // Items
    // ------------------------------------------------------------------

    pub fn list_items(&self, filter: &ItemFilter) -> Result<Vec<LibraryItem>, Td3Error> {
        let mut base_filter = filter.clone();
        base_filter.related_only = false;
        base_filter.failed_imports_only = false;

        let mut items = persistence::list_items(&self.path, &base_filter)?;

        if filter.related_only {
            let related_ids: HashSet<String> =
                crate::library::related::compute_related_groups(self)?
                    .into_iter()
                    .flat_map(|group| group.item_ids)
                    .collect();
            items.retain(|item| related_ids.contains(&item.item_id));
        }

        if filter.failed_imports_only {
            let failed_entries = persistence::list_failed_entries(&self.path)?;
            let failed_item_ids: HashSet<String> = failed_entries
                .iter()
                .filter_map(|entry| entry.item_id.clone())
                .collect();
            let failed_paths: HashSet<String> =
                failed_entries.into_iter().map(|entry| entry.path).collect();
            items.retain(|item| {
                failed_item_ids.contains(&item.item_id)
                    || item
                        .source_path
                        .as_deref()
                        .is_some_and(|path| failed_paths.contains(path))
            });
        }

        Ok(items)
    }

    pub fn get_item(&self, id: &str) -> Result<Option<LibraryItem>, Td3Error> {
        persistence::get_item(&self.path, id)
    }

    /// Look up a `LibraryItem` by its canonical pattern content hash. Used by
    /// ingest to skip re-importing duplicates.
    pub fn find_item_by_content_hash(&self, hash: &str) -> Result<Option<LibraryItem>, Td3Error> {
        persistence::find_item_by_content_hash(&self.path, hash)
    }

    /// Insert-or-update an item by `item_id`. Returns the resulting row.
    pub fn upsert_item(&self, mut item: LibraryItem) -> Result<LibraryItem, Td3Error> {
        if item.item_id.is_empty() {
            item.item_id = ids::new_id("item");
        }
        persistence::upsert_item(&self.path, &item)?;
        {
            let mut data = self.write_data()?;
            match data.items.iter_mut().find(|i| i.item_id == item.item_id) {
                Some(slot) => *slot = item.clone(),
                None => data.items.push(item.clone()),
            }
        }
        Ok(item)
    }

    pub fn delete_item(&self, id: &str) -> Result<bool, Td3Error> {
        let existed = persistence::get_item(&self.path, id)?.is_some();
        if !existed {
            return Ok(false);
        }
        persistence::delete_item_and_item_tags(&self.path, id)?;
        {
            let mut data = self.write_data()?;
            data.items.retain(|i| i.item_id != id);
            data.item_tags.retain(|(iid, _)| iid != id);
        }
        // Best-effort sidecar cleanup. A missing file is fine.
        self.remove_pattern_sidecar(id);
        Ok(true)
    }
}
