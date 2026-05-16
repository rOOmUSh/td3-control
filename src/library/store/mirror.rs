use super::*;

impl LibraryStore {
    // Internal lock helpers
    // ------------------------------------------------------------------

    pub(super) fn write_data(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, LibraryMirrorData>, Td3Error> {
        self.data
            .write()
            .map_err(|_| Td3Error::Other("library: write lock poisoned".into()))
    }

    /// Test-only mirror introspection. Returns the shapes of every collection
    /// that the in-memory mirror tracks so failure-path tests can assert the
    /// mirror was not mutated when a persistence transaction failed.
    #[cfg(test)]
    pub fn mirror_snapshot_for_tests(&self) -> MirrorSnapshot {
        let g = match self.data.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        MirrorSnapshot {
            item_ids: g.items.iter().map(|i| i.item_id.clone()).collect(),
            item_favorites: g
                .items
                .iter()
                .map(|i| (i.item_id.clone(), i.favorite))
                .collect(),
            item_archived: g
                .items
                .iter()
                .map(|i| (i.item_id.clone(), i.archived))
                .collect(),
            item_tags_per_item: g
                .items
                .iter()
                .map(|i| (i.item_id.clone(), i.tags.clone()))
                .collect(),
            snapshot_ids: g.snapshots.iter().map(|s| s.snapshot_id.clone()).collect(),
            snapshot_names: g
                .snapshots
                .iter()
                .map(|s| (s.snapshot_id.clone(), s.name.clone()))
                .collect(),
            snapshot_pinned: g
                .snapshots
                .iter()
                .map(|s| (s.snapshot_id.clone(), s.pinned))
                .collect(),
            tag_labels: g.tags.iter().map(|t| t.label.clone()).collect(),
            item_tag_edges: g.item_tags.clone(),
        }
    }
}
