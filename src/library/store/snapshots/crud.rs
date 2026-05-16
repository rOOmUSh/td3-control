use super::super::*;

impl LibraryStore {
    pub fn list_snapshots(&self) -> Result<Vec<Snapshot>, Td3Error> {
        persistence::list_snapshots(&self.path)
    }

    pub fn get_snapshot(&self, id: &str) -> Result<Option<Snapshot>, Td3Error> {
        persistence::get_snapshot(&self.path, id)
    }

    /// Delete a snapshot from the Bank catalog.
    ///
    /// This is database-only cleanup: source files and real devices are not
    /// touched. Snapshot slot rows are removed with the snapshot. Snapshot-
    /// origin items are removed only when no remaining snapshot slot points at
    /// them; file/curated/generated items survive because the snapshot did not
    /// own their source data.
    pub fn delete_snapshot(&self, id: &str) -> Result<Option<DeleteSnapshotReport>, Td3Error> {
        let mut data = persistence::load(&self.path)?;
        if !data.snapshots.iter().any(|s| s.snapshot_id == id) {
            return Ok(None);
        }

        let snapshot_slot_item_ids: BTreeSet<String> = data
            .snapshot_slots
            .iter()
            .filter(|slot| slot.snapshot_id == id)
            .filter_map(|slot| slot.item_id.clone())
            .collect();
        let item_ids_referenced_elsewhere: HashSet<String> = data
            .snapshot_slots
            .iter()
            .filter(|slot| slot.snapshot_id != id)
            .filter_map(|slot| slot.item_id.clone())
            .collect();
        let item_ids_to_delete: BTreeSet<String> = data
            .items
            .iter()
            .filter(|item| item.snapshot_id.as_deref() == Some(id))
            .filter(|item| matches!(item.source_kind, SourceKind::SnapshotSlot))
            .filter(|item| snapshot_slot_item_ids.contains(&item.item_id))
            .filter(|item| !item_ids_referenced_elsewhere.contains(&item.item_id))
            .map(|item| item.item_id.clone())
            .collect();

        let removed_slots = data
            .snapshot_slots
            .iter()
            .filter(|slot| slot.snapshot_id == id)
            .count() as u32;
        data.snapshots.retain(|snapshot| snapshot.snapshot_id != id);
        data.snapshot_slots.retain(|slot| slot.snapshot_id != id);

        if !item_ids_to_delete.is_empty() {
            data.items
                .retain(|item| !item_ids_to_delete.contains(&item.item_id));
            data.item_tags
                .retain(|(item_id, _)| !item_ids_to_delete.contains(item_id));
            data.pattern_analysis
                .retain(|analysis| !item_ids_to_delete.contains(&analysis.item_id));
            data.pattern_relations.retain(|relation| {
                !item_ids_to_delete.contains(&relation.from_item_id)
                    && !item_ids_to_delete.contains(&relation.to_item_id)
            });
            for entry in &mut data.file_index {
                if entry
                    .item_id
                    .as_ref()
                    .is_some_and(|item_id| item_ids_to_delete.contains(item_id))
                {
                    entry.item_id = None;
                }
                if entry
                    .duplicate_of
                    .as_ref()
                    .is_some_and(|item_id| item_ids_to_delete.contains(item_id))
                {
                    entry.duplicate_of = None;
                }
            }
        }

        persistence::save(&self.path, &data)?;
        {
            let mut mirror = self.write_data()?;
            *mirror = data.into();
        }
        for item_id in &item_ids_to_delete {
            self.remove_pattern_sidecar(item_id);
        }

        Ok(Some(DeleteSnapshotReport {
            snapshot_id: id.to_string(),
            removed_slots,
            removed_items: item_ids_to_delete.len() as u32,
        }))
    }

    pub fn create_snapshot(
        &self,
        name: String,
        description: Option<String>,
        origin: SnapshotOrigin,
    ) -> Result<Snapshot, Td3Error> {
        let snap = Snapshot {
            snapshot_id: ids::new_id("snap"),
            name,
            created_at: now_iso(),
            origin,
            slot_count: 0,
            description,
            pinned: false,
            tags: Vec::new(),
            backup_path: None,
        };
        persistence::upsert_snapshot(&self.path, &snap)?;
        {
            let mut data = self.write_data()?;
            data.snapshots.push(snap.clone());
        }
        Ok(snap)
    }

    pub fn rename_snapshot(
        &self,
        id: &str,
        new_name: String,
    ) -> Result<Option<Snapshot>, Td3Error> {
        let mut snapshot = match persistence::get_snapshot(&self.path, id)? {
            Some(s) => s,
            None => return Ok(None),
        };
        snapshot.name = new_name.clone();
        persistence::upsert_snapshot(&self.path, &snapshot)?;
        {
            let mut data = self.write_data()?;
            if let Some(slot) = data.snapshots.iter_mut().find(|s| s.snapshot_id == id) {
                slot.name = new_name;
            }
        }
        Ok(Some(snapshot))
    }

    pub fn pin_snapshot(&self, id: &str, pinned: bool) -> Result<Option<Snapshot>, Td3Error> {
        let mut snapshot = match persistence::get_snapshot(&self.path, id)? {
            Some(s) => s,
            None => return Ok(None),
        };
        snapshot.pinned = pinned;
        persistence::upsert_snapshot(&self.path, &snapshot)?;
        {
            let mut data = self.write_data()?;
            if let Some(slot) = data.snapshots.iter_mut().find(|s| s.snapshot_id == id) {
                slot.pinned = pinned;
            }
        }
        Ok(Some(snapshot))
    }

    pub fn update_snapshot_description(
        &self,
        id: &str,
        description: Option<String>,
    ) -> Result<Option<Snapshot>, Td3Error> {
        let mut snapshot = match persistence::get_snapshot(&self.path, id)? {
            Some(s) => s,
            None => return Ok(None),
        };
        snapshot.description = description.clone();
        persistence::upsert_snapshot(&self.path, &snapshot)?;
        {
            let mut data = self.write_data()?;
            if let Some(slot) = data.snapshots.iter_mut().find(|s| s.snapshot_id == id) {
                slot.description = description;
            }
        }
        Ok(Some(snapshot))
    }
}
