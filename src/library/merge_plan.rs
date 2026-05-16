//! Pure merge-plan builder.
//!
//! Given a compare report between two snapshots and a user's selection of
//! slot keys to merge from source into target, emit a deterministic plan
//! describing the resulting action per slot.
//!
//! The plan exposes two views of the same decision table:
//!
//! 1. A rich internal `steps: Vec<MergePlanStep>` with Copy / Overwrite /
//!    Clear / Keep / Skip enum variants - retained for backward-compat with
//!    existing callers and tests.
//! 2. Vec<MergeOperation>` reduces every
//!    row to one of four canonical actions (`copy_source_to_target`,
//!    `keep_target`, `skip_empty_source`, `clear_target`). This is the
//!    JSON view the UI downloads.

use serde::{Deserialize, Serialize};

use super::compare::{SlotCompareState, SnapshotCompareReport};

/// A single action in the merge plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlanStep {
    pub slot_key: String,
    pub action: MergeAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src_item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dst_item_id: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeAction {
    /// Keep the target slot as-is (nothing to change).
    Keep,
    /// Copy source pattern into this slot, creating a new entry.
    Copy,
    /// Overwrite the existing target pattern with the source pattern.
    Overwrite,
    /// Clear the slot (source is empty + selected).
    Clear,
    /// Not selected for merging - left unchanged.
    Skip,
}

/// The complete merge plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlan {
    pub source_snapshot_id: String,
    pub target_snapshot_id: String,
    pub steps: Vec<MergePlanStep>,
    pub copy_count: u32,
    pub overwrite_count: u32,
    pub clear_count: u32,
    pub keep_count: u32,
    pub skip_count: u32,
    /// View: each slot mapped to one of the four canonical
    /// actions. Always 64 rows in canonical slot order.
    #[serde(default)]
    pub operations: Vec<MergeOperation>,
}

/// Canonical wire-shape action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeOperationAction {
    CopySourceToTarget,
    KeepTarget,
    SkipEmptySource,
    ClearTarget,
}

/// One row of the operations array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeOperation {
    pub slot_key: String,
    pub action: MergeOperationAction,
    pub reason: String,
}

pub fn build_merge_plan(
    source_snapshot_id: &str,
    target_snapshot_id: &str,
    compare: &SnapshotCompareReport,
    selection: &[String],
) -> MergePlan {
    let selected: std::collections::HashSet<&str> = selection.iter().map(|s| s.as_str()).collect();

    let mut steps: Vec<MergePlanStep> = Vec::with_capacity(compare.slots.len());
    let mut copy_count = 0u32;
    let mut overwrite_count = 0u32;
    let mut clear_count = 0u32;
    let mut keep_count = 0u32;
    let mut skip_count = 0u32;

    for row in &compare.slots {
        let is_selected = selected.contains(row.slot_key.as_str());
        let (action, note) = if !is_selected {
            match row.state {
                SlotCompareState::Identical => {
                    keep_count += 1;
                    (MergeAction::Keep, "identical - no action".into())
                }
                _ => {
                    skip_count += 1;
                    (MergeAction::Skip, "not selected".into())
                }
            }
        } else {
            match row.state {
                SlotCompareState::Identical => {
                    keep_count += 1;
                    (MergeAction::Keep, "already identical".into())
                }
                SlotCompareState::SourceOnly => {
                    copy_count += 1;
                    (
                        MergeAction::Copy,
                        "copy source pattern into empty slot".into(),
                    )
                }
                SlotCompareState::TargetOnly => {
                    clear_count += 1;
                    (
                        MergeAction::Clear,
                        "source is empty - clear target slot".into(),
                    )
                }
                SlotCompareState::Different => {
                    overwrite_count += 1;
                    (
                        MergeAction::Overwrite,
                        "overwrite target pattern with source pattern".into(),
                    )
                }
                SlotCompareState::EmptyBoth => {
                    keep_count += 1;
                    (MergeAction::Keep, "both empty - no action".into())
                }
            }
        };

        steps.push(MergePlanStep {
            slot_key: row.slot_key.clone(),
            action,
            src_item_id: row.src_item_id.clone(),
            dst_item_id: row.dst_item_id.clone(),
            note,
        });
    }

    let operations = build_operations(&steps);

    MergePlan {
        source_snapshot_id: source_snapshot_id.to_string(),
        target_snapshot_id: target_snapshot_id.to_string(),
        steps,
        copy_count,
        overwrite_count,
        clear_count,
        keep_count,
        skip_count,
        operations,
    }
}

/// Reduce the rich `MergePlanStep` rows to the four actions
/// with the documented reason strings. Unselected `Identical` rows fold into
/// `keep_target/identical`; unselected non-empty rows fold into
/// `keep_target/different`. Empty source slots are always reported as
/// `skip_empty_source/source_empty_skipped` so the UI can warn about them.
fn build_operations(steps: &[MergePlanStep]) -> Vec<MergeOperation> {
    steps
        .iter()
        .map(|s| {
            let (action, reason) = match s.action {
                MergeAction::Copy => (
                    MergeOperationAction::CopySourceToTarget,
                    "different".to_string(),
                ),
                MergeAction::Overwrite => (
                    MergeOperationAction::CopySourceToTarget,
                    "different".to_string(),
                ),
                MergeAction::Clear => (
                    MergeOperationAction::ClearTarget,
                    "target_empty".to_string(),
                ),
                MergeAction::Keep => (MergeOperationAction::KeepTarget, "identical".to_string()),
                MergeAction::Skip => {
                    // Source-only without selection = empty source contribution
                    // gets skipped; otherwise we keep the existing target row.
                    if s.src_item_id.is_none() && s.dst_item_id.is_some() {
                        (MergeOperationAction::KeepTarget, "different".to_string())
                    } else if s.src_item_id.is_none() && s.dst_item_id.is_none() {
                        (
                            MergeOperationAction::SkipEmptySource,
                            "source_empty_skipped".to_string(),
                        )
                    } else {
                        (MergeOperationAction::KeepTarget, "different".to_string())
                    }
                }
            };
            MergeOperation {
                slot_key: s.slot_key.clone(),
                action,
                reason,
            }
        })
        .collect()
}
