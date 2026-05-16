use super::*;

impl LibraryStore {
    // ------------------------------------------------------------------
    // File index + import batches
    // ------------------------------------------------------------------

    pub fn list_file_index(&self) -> Result<Vec<FileIndexEntry>, Td3Error> {
        persistence::list_file_index(&self.path)
    }

    pub fn append_file_index_entry(&self, entry: FileIndexEntry) -> Result<(), Td3Error> {
        persistence::append_file_index_entry(&self.path, &entry)
    }

    /// Return every `FileIndexEntry` that belongs to `batch_id`. Used by
    /// `GET /api/bank/import-batches/:id` to render the per-batch drill-down.
    pub fn list_batch_entries(&self, batch_id: &str) -> Result<Vec<FileIndexEntry>, Td3Error> {
        persistence::list_batch_entries(&self.path, batch_id)
    }

    /// Return every `FileIndexEntry` with status `Failed` across all batches.
    pub fn list_failed_entries(&self) -> Result<Vec<FileIndexEntry>, Td3Error> {
        persistence::list_failed_entries(&self.path)
    }

    /// Replace (by path + batch_id) an existing `FileIndexEntry` with a new
    /// copy. If no matching row is found, appends `entry` instead.
    pub fn replace_file_index_entry(&self, entry: FileIndexEntry) -> Result<(), Td3Error> {
        persistence::replace_file_index_entry(&self.path, &entry)
    }

    pub fn create_import_batch(&self, scan_root: Option<String>) -> Result<ImportBatch, Td3Error> {
        let batch = ImportBatch {
            batch_id: ids::new_id("batch"),
            started_at: now_iso(),
            finished_at: None,
            scan_root,
            files_found: 0,
            files_imported: 0,
            duplicates_skipped: 0,
            unsupported: 0,
            failed: 0,
        };
        persistence::upsert_import_batch(&self.path, &batch)?;
        Ok(batch)
    }

    pub fn finish_import_batch(
        &self,
        batch_id: &str,
        files_found: u32,
        files_imported: u32,
        duplicates_skipped: u32,
        unsupported: u32,
        failed: u32,
    ) -> Result<Option<ImportBatch>, Td3Error> {
        let Some(mut batch) = persistence::get_import_batch(&self.path, batch_id)? else {
            return Ok(None);
        };
        batch.finished_at = Some(now_iso());
        batch.files_found = files_found;
        batch.files_imported = files_imported;
        batch.duplicates_skipped = duplicates_skipped;
        batch.unsupported = unsupported;
        batch.failed = failed;
        persistence::upsert_import_batch(&self.path, &batch)?;
        Ok(Some(batch))
    }

    pub fn list_import_batches(&self) -> Result<Vec<ImportBatch>, Td3Error> {
        persistence::list_import_batches(&self.path)
    }

    pub fn get_import_batch(&self, id: &str) -> Result<Option<ImportBatch>, Td3Error> {
        persistence::get_import_batch(&self.path, id)
    }

    /// Delete an import batch and every catalog row it exclusively owns.
    ///
    /// Semantics:
    /// - All `FileIndexEntry` rows with `batch_id == batch_id` are removed.
    /// - The `ImportBatch` record is removed.
    /// - A `LibraryItem` is removed iff it was created by this batch AND no
    ///   other batch's entry still references it (as `item_id` or
    ///   `duplicate_of`). A candidate item is either (a) the `item_id` field
    ///   of one of this batch's entries (single-pattern imports) or
    ///   (b) an item whose `source_path` matches one of this batch's entry
    ///   paths (catches `.sqs` slot items whose entry carries no `item_id`).
    ///   Any `duplicate_of` pointer in a surviving entry that targeted a
    ///   deleted item is cleared so the catalog stays consistent.
    /// - A `Snapshot` (origin `Imported`) is removed iff (a) its `name`
    ///   matches the file_stem of a path in this batch (covers all-silent
    ///   and dedup-linked banks ingested by this scan) OR (b) every one of
    ///   its non-empty slots references an item whose `source_path` lies in
    ///   this batch (covers renamed snapshots whose items still tie back).
    ///   Its `SnapshotSlot` rows go with it. Non-`Imported` snapshots
    ///   (Manual, Backup) are never touched by a batch delete.
    /// - Pattern sidecars for deleted items are best-effort unlinked on disk.
    /// - The originating source files on disk are **never** touched.
    pub fn delete_import_batch(&self, batch_id: &str) -> Result<DeleteImportBatchReport, Td3Error> {
        // --- Build the delete plan from SQLite ---------------------------
        let plan = persistence::plan_delete_import_batch(&self.path, batch_id)?;
        if !plan.batch_existed && plan.batch_paths.is_empty() {
            return Ok(DeleteImportBatchReport {
                batch_id: batch_id.to_string(),
                removed_entries: 0,
                removed_items: 0,
                removed_snapshots: 0,
            });
        }
        let items_to_delete = plan.items_to_delete;
        let snapshot_ids_to_delete: Vec<String> = plan
            .snapshots_to_delete
            .into_iter()
            .chain(plan.orphan_snapshot_ids)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        // --- Apply the delete in SQLite, then refresh the mirror ---------
        let removed_entries = persistence::apply_delete_import_batch(
            &self.path,
            batch_id,
            &items_to_delete,
            &snapshot_ids_to_delete,
        )?
        .removed_entries;
        let refreshed = persistence::load(&self.path)?;
        {
            let mut data = self.write_data()?;
            *data = refreshed.into();
        }

        // Best-effort sidecar cleanup. A missing sidecar is fine.
        for id in &items_to_delete {
            self.remove_pattern_sidecar(id);
        }

        Ok(DeleteImportBatchReport {
            batch_id: batch_id.to_string(),
            removed_entries,
            removed_items: items_to_delete.len() as u32,
            removed_snapshots: snapshot_ids_to_delete.len() as u32,
        })
    }

    // ------------------------------------------------------------------
    // Related groups
    // ------------------------------------------------------------------

    /// Legacy compatibility surface. Related-group computation now lives in
    /// `library::related`, so this store hook currently returns an empty set.
    pub fn list_related_groups(&self) -> Result<Vec<Vec<String>>, Td3Error> {
        Ok(Vec::new())
    }

    /// Bulk-update the `duplicate_status` field for a list of items. Used by
    /// the duplicates handler to write back cluster assignments after a
    /// detection pass. Silently ignores unknown item IDs.
    pub fn set_duplicate_statuses(
        &self,
        updates: &[(String, DuplicateStatus)],
    ) -> Result<(), Td3Error> {
        if updates.is_empty() {
            return Ok(());
        }
        let mut changed_items: Vec<LibraryItem> = Vec::new();
        for (id, status) in updates {
            if let Some(mut item) = persistence::get_item(&self.path, id)? {
                item.duplicate_status = *status;
                changed_items.push(item);
            }
        }
        persistence::upsert_items(&self.path, &changed_items)?;
        {
            let mut data = self.write_data()?;
            for changed in &changed_items {
                if let Some(slot) = data.items.iter_mut().find(|i| i.item_id == changed.item_id) {
                    slot.duplicate_status = changed.duplicate_status;
                }
            }
        }
        Ok(())
    }

    pub fn list_pattern_relations(&self) -> Result<Vec<PatternRelation>, Td3Error> {
        persistence::list_pattern_relations(&self.path)
    }
}
