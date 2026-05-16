use super::super::*;

impl LibraryStore {
    /// Return a padded 64-entry slot grid for `snapshot_id`. Missing slots are
    /// synthesized with `empty = true`.
    pub fn list_snapshot_slots(&self, snapshot_id: &str) -> Result<Vec<SnapshotSlot>, Td3Error> {
        persistence::list_snapshot_slots(&self.path, snapshot_id)
    }

    pub fn upsert_snapshot_slot(&self, slot: SnapshotSlot) -> Result<SnapshotSlot, Td3Error> {
        persistence::upsert_snapshot_slot(&self.path, &slot)?;
        {
            let mut data = self.write_data()?;
            match data
                .snapshot_slots
                .iter_mut()
                .find(|s| s.snapshot_id == slot.snapshot_id && s.slot_key == slot.slot_key)
            {
                Some(existing) => *existing = slot.clone(),
                None => data.snapshot_slots.push(slot.clone()),
            }
        }
        Ok(slot)
    }

    pub fn refresh_snapshot_slot_count(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Snapshot>, Td3Error> {
        let all_slots = persistence::list_snapshot_slots(&self.path, snapshot_id)?;
        let slot_count = all_slots.iter().filter(|s| !s.empty).count() as u32;
        let mut snapshot = match persistence::get_snapshot(&self.path, snapshot_id)? {
            Some(s) => s,
            None => return Ok(None),
        };
        snapshot.slot_count = slot_count;
        persistence::upsert_snapshot(&self.path, &snapshot)?;
        {
            let mut data = self.write_data()?;
            if let Some(slot) = data
                .snapshots
                .iter_mut()
                .find(|s| s.snapshot_id == snapshot_id)
            {
                slot.slot_count = slot_count;
            }
        }
        Ok(Some(snapshot))
    }

    /// Move or swap a snapshot slot row from `from_key` to `to_key`.
    pub fn move_snapshot_slot(
        &self,
        snapshot_id: &str,
        from_key: &str,
        to_key: &str,
    ) -> Result<bool, Td3Error> {
        if from_key == to_key {
            return Ok(false);
        }
        let swapped = persistence::move_snapshot_slot(&self.path, snapshot_id, from_key, to_key)?;
        let mut data = self.write_data()?;
        let from_idx = data
            .snapshot_slots
            .iter()
            .position(|s| s.snapshot_id == snapshot_id && s.slot_key == from_key);
        let to_idx = data
            .snapshot_slots
            .iter()
            .position(|s| s.snapshot_id == snapshot_id && s.slot_key == to_key);
        match (from_idx, to_idx) {
            (Some(fi), Some(ti)) => {
                data.snapshot_slots[fi].slot_key = to_key.to_string();
                data.snapshot_slots[ti].slot_key = from_key.to_string();
            }
            (Some(fi), None) => {
                data.snapshot_slots[fi].slot_key = to_key.to_string();
            }
            (None, _) => {}
        }
        Ok(swapped)
    }

    /// Remove snapshot slot rows matching `(snapshot_id, slot_key)`.
    pub fn delete_snapshot_slots(
        &self,
        snapshot_id: &str,
        slot_keys: &[String],
    ) -> Result<usize, Td3Error> {
        let removed = persistence::delete_snapshot_slots(&self.path, snapshot_id, slot_keys)?;
        let updated_snapshot = {
            let mut data = self.write_data()?;
            data.snapshot_slots.retain(|s| {
                !(s.snapshot_id == snapshot_id && slot_keys.iter().any(|k| k == &s.slot_key))
            });
            let remaining = data
                .snapshot_slots
                .iter()
                .filter(|s| s.snapshot_id == snapshot_id && !s.empty)
                .count() as u32;
            match data
                .snapshots
                .iter_mut()
                .find(|s| s.snapshot_id == snapshot_id)
            {
                Some(snap) => {
                    snap.slot_count = remaining;
                    Some(snap.clone())
                }
                None => None,
            }
        };
        if let Some(snap) = updated_snapshot {
            persistence::upsert_snapshot(&self.path, &snap)?;
        }
        Ok(removed)
    }
}
