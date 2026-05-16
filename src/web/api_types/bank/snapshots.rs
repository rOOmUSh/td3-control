use serde::{Deserialize, Serialize};

use crate::library::model::{LibraryItem, Snapshot, SnapshotOrigin, SnapshotSlot};

use super::super::WebPattern;

#[derive(Serialize, Deserialize)]
pub struct SnapshotsResponse {
    pub snapshots: Vec<Snapshot>,
}

#[derive(Serialize, Deserialize)]
pub struct SnapshotDetailResponse {
    pub snapshot: Snapshot,
    pub slots: Vec<SnapshotSlotView>,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteSnapshotResponse {
    pub snapshot_id: String,
    pub removed_slots: u32,
    pub removed_items: u32,
}

/// UI-facing view of a single snapshot slot. Extends the stored
/// `SnapshotSlot` with optional compare markers (`changed`, `duplicate`) so
/// the grid in the Snapshots view can render compare context without
/// changing the persisted slot schema.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnapshotSlotView {
    pub slot_key: String,
    pub empty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Set to `Some(true)` when this slot's payload differs from the
    /// current compare context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    /// Set to `Some(true)` when this slot's payload is a duplicate of
    /// another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate: Option<bool>,
}

impl SnapshotSlotView {
    /// Convenience constructor that lifts a persisted `SnapshotSlot` into
    /// the UI-facing view with no compare markers set.
    pub fn from_slot(slot: SnapshotSlot) -> Self {
        Self {
            slot_key: slot.slot_key,
            empty: slot.empty,
            item_id: slot.item_id,
            display_name: slot.display_name,
            changed: None,
            duplicate: None,
        }
    }
}

/// Body for `POST /api/bank/snapshots/sync-backups`. `backup_dir` is
/// optional; when omitted the server uses its default backup directory.
#[derive(Deserialize, Default)]
pub struct SyncBackupsRequest {
    #[serde(default)]
    pub backup_dir: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SyncBackupsResponse {
    pub added: u32,
    pub total: u32,
}

#[derive(Serialize, Deserialize)]
pub struct CreateSnapshotRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub origin: SnapshotOrigin,
}

/// One entry in a `CreateSnapshotFromPatternsRequest`. `slot_key` is the
/// dashed form "G{g}-P{p}{A|B}" to match the store's canonical shape.
///
/// `display_name`, when present, becomes the snapshot slot's visible label
/// (and the underlying `LibraryItem.display_name` for newly-created items -
/// reused dedup'd items keep their existing name). When absent the snapshot
/// slot falls back to `slot_key` for display, preserving the legacy main-page
/// overflow behavior.
#[derive(Serialize, Deserialize, Clone)]
pub struct SnapshotFromPatternSlot {
    pub slot_key: String,
    pub pattern: WebPattern,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Atomic "create snapshot + fill N slots" endpoint, used by the main-page
/// PUSH TO TD-3 overflow flow. The backend materialises the snapshot,
/// creates (or reuses, via content-hash dedupe) a `LibraryItem` per supplied
/// pattern, writes the 112-byte sidecar, and upserts a `SnapshotSlot` pointing
/// at the item. Name collisions are resolved by appending " (2)", " (3)", ...
/// until a free slot is found - the effective name is returned in the
/// resulting `Snapshot`.
#[derive(Serialize, Deserialize)]
pub struct CreateSnapshotFromPatternsRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub slots: Vec<SnapshotFromPatternSlot>,
}

/// One UI pattern being saved from the multipattern canvas into the Bank.
///
/// `slot_key` is optional. When present and the target snapshot slot is empty,
/// the backend uses it; otherwise it falls back to the first empty slot in the
/// snapshot. Standalone item saves ignore it.
#[derive(Serialize, Deserialize, Clone)]
pub struct BankPatternSaveEntry {
    pub pattern: WebPattern,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub slot_key: Option<String>,
}

/// Save one or more UI patterns into the Bank catalog.
///
/// `destination` is one of:
/// - `"new_snapshot"`: create `snapshot_name` or an `SN_*` timestamp snapshot.
/// - `"snapshot"`: insert into the existing `snapshot_id`.
/// - `"single_item"`: create/reuse standalone generated LibraryItem rows.
#[derive(Serialize, Deserialize)]
pub struct SavePatternsToBankRequest {
    pub destination: String,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub snapshot_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub root_note: Option<String>,
    #[serde(default)]
    pub scale_name: Option<String>,
    pub entries: Vec<BankPatternSaveEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct SavePatternsToBankResponse {
    pub items: Vec<LibraryItem>,
    #[serde(default)]
    pub snapshot: Option<Snapshot>,
    #[serde(default)]
    pub slots: Vec<SnapshotSlotView>,
    pub created_snapshot: bool,
}

#[derive(Deserialize, Default)]
pub struct AddItemToSnapshotRequest {
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub slot_key: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct AddItemToSnapshotResponse {
    pub item_id: String,
    pub snapshot: Snapshot,
    pub slot: SnapshotSlotView,
    pub slots: Vec<SnapshotSlotView>,
    pub created_snapshot: bool,
}

#[derive(Deserialize, Default)]
pub struct PatchSnapshotRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

/// Body for `DELETE /api/bank/snapshots/:id/slots`. Removes the listed slot
/// rows from the snapshot - the padded grid will fill the holes back in with
/// `empty = true` placeholders, so a delete is non-destructive to the 64-cell
/// layout. `LibraryItem`s referenced by the deleted slots are left alone (the
/// catalog still owns them, and they may still appear in other snapshots).
#[derive(Serialize, Deserialize)]
pub struct DeleteSnapshotSlotsRequest {
    pub slot_keys: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct DeleteSnapshotSlotsResponse {
    pub deleted: u32,
}

/// Body for `POST /api/bank/snapshots/:id/move-slot`. Re-positions the slot
/// stored at `from_key` to `to_key`. When `to_key` is empty the slot is moved
/// in place; when `to_key` is occupied the two slots are swapped - the
/// underlying `LibraryItem`s are not touched in either case (the catalog
/// entries' origin metadata stays accurate).
#[derive(Serialize, Deserialize)]
pub struct MoveSnapshotSlotRequest {
    pub from_key: String,
    pub to_key: String,
}

/// Result of a move/swap. `swapped` is `true` iff the destination was already
/// occupied and the two rows traded places. Returned in addition to a fresh
/// `SnapshotDetailResponse` so callers can re-render in one round trip.
#[derive(Serialize, Deserialize)]
pub struct MoveSnapshotSlotResponse {
    pub swapped: bool,
    pub snapshot: Snapshot,
    pub slots: Vec<SnapshotSlotView>,
}
