use super::super::*;
use super::zip_helpers::{read_bank_sqs_payloads, read_slot_presence};

impl LibraryStore {
    /// Sync a slice of backup-zip inventory entries into the library as
    /// `SnapshotOrigin::Backup` snapshots. Idempotent: a snapshot whose
    /// `backup_path` already matches `entry.path` is left alone.
    ///
    /// For each newly inserted snapshot, create 64 `SnapshotSlot` rows. Each
    /// slot's `empty` flag reflects whether the zip contains a slot subfolder.
    /// When `bank.sqs` is readable, non-silent slots are linked to real
    /// library items.
    ///
    /// Returns the number of new snapshots added.
    pub fn sync_backup_inventory(
        &self,
        entries: &[BackupInventoryEntry],
    ) -> Result<usize, Td3Error> {
        let mut added = 0usize;
        for entry in entries {
            let path_str = entry.path.to_string_lossy().to_string();

            if persistence::snapshot_exists_with_backup_path(&self.path, &path_str)? {
                continue;
            }

            let zip_payloads = read_bank_sqs_payloads(&entry.path).unwrap_or_default();
            let present_slots = if zip_payloads.is_empty() {
                match read_slot_presence(&entry.path) {
                    Ok(s) => s,
                    Err(e) => {
                        return Err(Td3Error::Other(format!(
                            "sync_backup_inventory: {}: {}",
                            entry.path.display(),
                            e
                        )));
                    }
                }
            } else {
                zip_payloads
                    .iter()
                    .filter(|(_, payload)| !crate::formats::sqs::is_silent(payload))
                    .map(|(key, _)| key.replace('-', ""))
                    .collect()
            };

            let name = entry
                .filename
                .strip_suffix(".zip")
                .unwrap_or(&entry.filename)
                .to_string();

            let snap = Snapshot {
                snapshot_id: ids::new_id("snap"),
                name: name.clone(),
                created_at: entry.timestamp.clone(),
                origin: SnapshotOrigin::Backup,
                slot_count: present_slots.len() as u32,
                description: None,
                pinned: false,
                tags: Vec::new(),
                backup_path: Some(path_str.clone()),
            };
            let snapshot_for_db = snap.clone();

            let mut slots: Vec<SnapshotSlot> = Vec::with_capacity(64);
            let mut new_items: Vec<(LibraryItem, Vec<u8>)> = Vec::new();

            for g in 1..=4u8 {
                for p in 1..=8u8 {
                    for side in ['A', 'B'] {
                        let key = format!("G{}-P{}{}", g, p, side);
                        let zip_key = format!("G{}P{}{}", g, p, side);
                        let empty = !present_slots.contains(&zip_key);
                        let mut item_id: Option<String> = None;

                        if !empty {
                            item_id = self.sync_backup_slot_item(
                                &zip_payloads,
                                &key,
                                &name,
                                &path_str,
                                &snap,
                                &mut new_items,
                            )?;
                        }

                        slots.push(SnapshotSlot {
                            snapshot_id: snap.snapshot_id.clone(),
                            slot_key: key.clone(),
                            item_id,
                            empty,
                            display_name: Some(key),
                        });
                    }
                }
            }

            let slots_for_db = slots.clone();
            let items_for_db: Vec<LibraryItem> =
                new_items.iter().map(|(item, _)| item.clone()).collect();

            for (item, payload) in &new_items {
                self.write_pattern_bytes(&item.item_id, payload)?;
            }

            if let Err(e) = persistence::append_backup_snapshot_bundle(
                &self.path,
                &snapshot_for_db,
                &slots_for_db,
                &items_for_db,
            ) {
                for (item, _) in &new_items {
                    self.remove_pattern_sidecar(&item.item_id);
                }
                return Err(e);
            }
            {
                let mut data = self.write_data()?;
                data.snapshots.push(snap);
                data.snapshot_slots.extend(slots);
                for (item, _) in &new_items {
                    data.items.push(item.clone());
                }
            }
            added += 1;
        }
        Ok(added)
    }

    fn sync_backup_slot_item(
        &self,
        zip_payloads: &[(String, Vec<u8>)],
        key: &str,
        snapshot_name: &str,
        path_str: &str,
        snap: &Snapshot,
        new_items: &mut Vec<(LibraryItem, Vec<u8>)>,
    ) -> Result<Option<String>, Td3Error> {
        let Some((_, payload)) = zip_payloads
            .iter()
            .find(|(k, _)| *k == key || *k == key.replace('-', ""))
        else {
            return Ok(None);
        };
        let Some((content_hash, payload_vec)) = pattern_hash_from_payload(payload) else {
            return Ok(None);
        };
        let existing = match self.find_item_by_content_hash(&content_hash) {
            Ok(Some(item)) => Some(item.item_id),
            Ok(None) | Err(_) => None,
        };
        if existing.is_some() {
            return Ok(existing);
        }

        let now = now_iso();
        let id = ids::new_id("item");
        let item = LibraryItem {
            item_id: id.clone(),
            display_name: key.to_string(),
            source_kind: SourceKind::SnapshotSlot,
            source_label: format!("{} @ {}", snapshot_name, key),
            source_path: Some(path_str.to_string()),
            created_at: now.clone(),
            updated_at: now,
            tags: vec!["snapshot-origin".to_string()],
            favorite: false,
            archived: false,
            slot_key: Some(key.to_string()),
            snapshot_id: Some(snap.snapshot_id.clone()),
            snapshot_name: Some(snapshot_name.to_string()),
            format: Some("sqs".to_string()),
            scale_name: None,
            root_note: None,
            duplicate_status: DuplicateStatus::Unique,
            related_group_count: 0,
            analysis_status: AnalysisStatus::Unknown,
            notes: None,
            content_hash: Some(content_hash),
        };
        new_items.push((item, payload_vec));
        Ok(Some(id))
    }
}

fn pattern_hash_from_payload(payload: &[u8]) -> Option<(String, Vec<u8>)> {
    let mut sx = Vec::with_capacity(115);
    sx.push(0x78);
    sx.push(0x00);
    sx.push(0x00);
    sx.extend_from_slice(payload);
    let pat = crate::pattern::sysex_to_pattern(&sx).ok()?;
    let hash = crate::library::duplicates::pattern_hash(&pat);
    Some((hash, payload.to_vec()))
}
