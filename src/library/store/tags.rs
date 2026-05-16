use super::*;

impl LibraryStore {
    // ------------------------------------------------------------------
    // Tags
    // ------------------------------------------------------------------

    pub fn list_tags(&self) -> Result<Vec<Tag>, Td3Error> {
        persistence::list_tags(&self.path)
    }

    /// Insert-or-update a tag by `tag_id`. If `tag_id` is empty, a new one
    /// is generated.
    pub fn upsert_tag(&self, mut tag: Tag) -> Result<Tag, Td3Error> {
        if tag.tag_id.is_empty() {
            tag.tag_id = ids::new_id("tag");
        }
        persistence::upsert_tag(&self.path, &tag)?;
        {
            let mut data = self.write_data()?;
            match data.tags.iter_mut().find(|t| t.tag_id == tag.tag_id) {
                Some(slot) => *slot = tag.clone(),
                None => data.tags.push(tag.clone()),
            }
        }
        Ok(tag)
    }

    /// Ensure a tag with label `label` exists; return it. Creates a User-kind
    /// tag if not found.
    pub fn ensure_tag_by_label(&self, label: &str) -> Result<Tag, Td3Error> {
        self.ensure_tag_with_kind(label, TagKind::User)
    }

    /// Same as `ensure_tag_by_label` but lets the caller request a specific
    /// `TagKind` when the tag is newly created. Existing tags are returned
    /// unchanged - this method never rewrites an existing tag's kind, which
    /// means users can't accidentally demote a System tag by re-applying it.
    pub fn ensure_tag_with_kind(&self, label: &str, kind: TagKind) -> Result<Tag, Td3Error> {
        if let Some(existing) = persistence::get_tag_by_label(&self.path, label)? {
            let mut data = self.write_data()?;
            if data.tags.iter().all(|tag| tag.tag_id != existing.tag_id) {
                data.tags.push(existing.clone());
            }
            return Ok(existing);
        }

        let tag = Tag {
            tag_id: ids::new_id("tag"),
            label: label.to_string(),
            kind,
            color: None,
        };
        persistence::upsert_tag(&self.path, &tag)?;
        {
            let mut data = self.write_data()?;
            if let Some(existing) = data.tags.iter().find(|t| t.label == label) {
                return Ok(existing.clone());
            }
            data.tags.push(tag.clone());
        }
        Ok(tag)
    }

    pub fn add_tag_to_item(&self, item_id: &str, label: &str) -> Result<(), Td3Error> {
        let tag = self.ensure_tag_by_label(label)?;
        let mut item = match persistence::get_item(&self.path, item_id)? {
            Some(it) => it,
            None => return Ok(()),
        };
        if !item.tags.contains(&tag.label) {
            item.tags.push(tag.label.clone());
        }
        persistence::add_tag_to_item(&self.path, &item, &tag.tag_id)?;
        {
            let mut data = self.write_data()?;
            if !data
                .item_tags
                .iter()
                .any(|(i, t)| i == item_id && t == &tag.tag_id)
            {
                data.item_tags
                    .push((item_id.to_string(), tag.tag_id.clone()));
            }
            if let Some(slot) = data.items.iter_mut().find(|i| i.item_id == item_id) {
                if !slot.tags.contains(&tag.label) {
                    slot.tags.push(tag.label.clone());
                }
            }
        }
        Ok(())
    }

    pub fn remove_tag_from_item(&self, item_id: &str, label: &str) -> Result<(), Td3Error> {
        let tag_id_for_delete = persistence::get_tag_by_label(&self.path, label)?.map(|t| t.tag_id);
        let updated_item = match persistence::get_item(&self.path, item_id)? {
            Some(mut it) => {
                it.tags.retain(|t| t != label);
                Some(it)
            }
            None => None,
        };
        if let Some(item) = &updated_item {
            persistence::remove_tag_from_item(&self.path, item, tag_id_for_delete.as_deref())?;
        } else {
            persistence::delete_item_tag_edge(&self.path, item_id, tag_id_for_delete.as_deref())?;
        }
        {
            let mut data = self.write_data()?;
            if let Some(tid) = &tag_id_for_delete {
                data.item_tags.retain(|(i, t)| !(i == item_id && t == tid));
            }
            if let Some(slot) = data.items.iter_mut().find(|i| i.item_id == item_id) {
                slot.tags.retain(|t| t != label);
            }
        }
        Ok(())
    }

    /// Bulk-apply tag edits across multiple items. `add` and `remove` are tag
    /// labels. Invalid item IDs are silently skipped; callers can list first
    /// to detect those.
    pub fn bulk_tag(
        &self,
        item_ids: &[String],
        add: &[String],
        remove: &[String],
    ) -> Result<(), Td3Error> {
        if item_ids.is_empty() || (add.is_empty() && remove.is_empty()) {
            return Ok(());
        }

        let mut tags_to_upsert: Vec<Tag> = Vec::new();
        let mut items_to_upsert: Vec<LibraryItem> = Vec::new();
        let mut item_tags_to_add: BTreeSet<(String, String)> = BTreeSet::new();
        let mut item_tags_to_remove: BTreeSet<(String, String)> = BTreeSet::new();

        let existing_tags = persistence::list_tags(&self.path)?;
        let mut label_to_tag_id: std::collections::HashMap<String, String> = existing_tags
            .iter()
            .map(|t| (t.label.clone(), t.tag_id.clone()))
            .collect();

        for label in add {
            if !label_to_tag_id.contains_key(label) {
                let tag = Tag {
                    tag_id: ids::new_id("tag"),
                    label: label.clone(),
                    kind: TagKind::User,
                    color: None,
                };
                label_to_tag_id.insert(label.clone(), tag.tag_id.clone());
                tags_to_upsert.push(tag);
            }
        }

        for id in item_ids {
            let mut item = match persistence::get_item(&self.path, id)? {
                Some(it) => it,
                None => continue,
            };
            let mut item_changed = false;

            for label in add {
                let Some(tag_id) = label_to_tag_id.get(label) else {
                    continue;
                };
                if !item.tags.contains(label) {
                    item.tags.push(label.clone());
                    item_tags_to_add.insert((id.clone(), tag_id.clone()));
                    item_changed = true;
                }
            }

            for label in remove {
                let Some(tag_id) = label_to_tag_id.get(label) else {
                    continue;
                };
                let before_tags = item.tags.len();
                item.tags.retain(|existing| existing != label);
                if item.tags.len() != before_tags {
                    item_tags_to_remove.insert((id.clone(), tag_id.clone()));
                    item_changed = true;
                }
            }

            if item_changed {
                items_to_upsert.push(item);
            }
        }

        persistence::apply_bulk_tag(
            &self.path,
            &tags_to_upsert,
            &items_to_upsert,
            &item_tags_to_add.iter().cloned().collect::<Vec<_>>(),
            &item_tags_to_remove.iter().cloned().collect::<Vec<_>>(),
        )?;

        {
            let mut data = self.write_data()?;
            for tag in &tags_to_upsert {
                if data.tags.iter().all(|t| t.tag_id != tag.tag_id) {
                    data.tags.push(tag.clone());
                }
            }
            for item in &items_to_upsert {
                match data.items.iter_mut().find(|i| i.item_id == item.item_id) {
                    Some(slot) => *slot = item.clone(),
                    None => data.items.push(item.clone()),
                }
            }
            for (id, tag_id) in &item_tags_to_add {
                if !data.item_tags.iter().any(|(i, t)| i == id && t == tag_id) {
                    data.item_tags.push((id.clone(), tag_id.clone()));
                }
            }
            for (id, tag_id) in &item_tags_to_remove {
                data.item_tags.retain(|(i, t)| !(i == id && t == tag_id));
            }
        }

        Ok(())
    }
}
