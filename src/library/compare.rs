//! Pure comparison helpers.
//!
//! Both functions are deterministic, side-effect-free, and allocation-lean so
//! the handlers can call them directly without taking any lock.
//!
//! Item-vs-item diff returns per-field
//! counts plus a user-readable summary. Snapshot-vs-snapshot returns the
//! 64-slot outcome grid.

use serde::{Deserialize, Serialize};

use crate::pattern::Pattern;
use crate::step;

use super::model::SnapshotSlot;

// ---------------------------------------------------------------------------
// Item-vs-item
// ---------------------------------------------------------------------------

/// Diff counts for two patterns, plus a pre-rendered summary string.
///
/// `differ_steps` lists the step indices (0-based) where at least one field
/// (note, accent, slide, transpose, time) changed - the UI uses this to paint
/// a compact per-step diff row.
///
/// `duplicate_score` and `relatedness_score` are normalised to `[0.0, 1.0]`
/// and let the UI classify a pair at a glance:
///   - `duplicate_score >= 0.9` → effectively identical / exact duplicate.
///   - `duplicate_score >= 0.7 && rhythm_same` → near-duplicate.
///   - `relatedness_score >= 0.5` → worth surfacing as a relation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemCompareReport {
    pub note_diff: u32,
    pub accent_diff: u32,
    pub slide_diff: u32,
    pub transpose_diff: u32,
    pub time_diff: u32,
    pub active_steps_diff: bool,
    pub triplet_diff: bool,
    pub summary: String,
    pub identical: bool,
    /// Step indices (0-based) where at least one field differs.
    #[serde(default)]
    pub differ_steps: Vec<u32>,
    /// `true` when both patterns' rhythm fingerprints match.
    #[serde(default)]
    pub same_rhythm: bool,
    /// Normalised duplicate likelihood (1.0 = identical).
    #[serde(default)]
    pub duplicate_score: f32,
    /// Normalised relatedness score (1.0 = identical, lower = more edits).
    #[serde(default)]
    pub relatedness_score: f32,
}

/// Compare two patterns step-by-step. Both patterns are assumed already
/// validated (16 steps each).
pub fn compare_items(a: &Pattern, b: &Pattern) -> ItemCompareReport {
    let mut note_diff = 0u32;
    let mut accent_diff = 0u32;
    let mut slide_diff = 0u32;
    let mut transpose_diff = 0u32;
    let mut time_diff = 0u32;
    let mut differ_steps: Vec<u32> = Vec::new();

    for i in 0..16 {
        let sa = step_at(a, i);
        let sb = step_at(b, i);
        let mut any = false;
        if sa.note != sb.note {
            note_diff += 1;
            any = true;
        }
        if sa.accent != sb.accent {
            accent_diff += 1;
            any = true;
        }
        if sa.slide != sb.slide {
            slide_diff += 1;
            any = true;
        }
        if sa.transpose != sb.transpose {
            transpose_diff += 1;
            any = true;
        }
        if sa.time != sb.time {
            time_diff += 1;
            any = true;
        }
        if any {
            differ_steps.push(i as u32);
        }
    }

    let active_steps_diff = pattern_active_steps(a) != pattern_active_steps(b);
    let triplet_diff = pattern_triplet(a) != pattern_triplet(b);

    let total = note_diff + accent_diff + slide_diff + transpose_diff + time_diff;
    let identical = total == 0 && !active_steps_diff && !triplet_diff;

    // Rhythm-fingerprint parity - inlined here to avoid a cycle (duplicates.rs
    // already depends on step::Step but not on compare.rs).
    let same_rhythm =
        super::duplicates::rhythm_fingerprint(a) == super::duplicates::rhythm_fingerprint(b);

    // Score model:
    //   - per-step max 5 field diffs across 16 steps = 80 potential edits;
    //   - plus 2 structural flags (active-steps, triplet).
    // `relatedness_score` is 1.0 for identical and degrades linearly; patterns
    // with very different rhythms still score above 0 as long as some fields
    // match.
    let structural_diffs = (active_steps_diff as u32) + (triplet_diff as u32);
    let total_weighted = total + (structural_diffs * 8); // weight structural bits
    let max_weighted = 80 + 16; // 5*16 + 8*2
    let relatedness_score = if max_weighted == 0 {
        1.0
    } else {
        let ratio = (total_weighted as f32) / (max_weighted as f32);
        (1.0 - ratio).clamp(0.0, 1.0)
    };
    // `duplicate_score` is stricter: same rhythm + ≤3 note edits rides high.
    let duplicate_score = if identical {
        1.0
    } else if same_rhythm
        && note_diff <= 3
        && transpose_diff == 0
        && !active_steps_diff
        && !triplet_diff
    {
        // 3 edits max → score ∈ [0.7, 0.95]
        let edit_ratio = (note_diff as f32) / 3.0;
        (0.95 - 0.25 * edit_ratio).clamp(0.7, 0.95)
    } else {
        // Far from a duplicate: reuse relatedness but cap well below 0.7.
        (relatedness_score * 0.6).clamp(0.0, 0.69)
    };

    let summary = if identical {
        "Patterns are identical".to_string()
    } else {
        let mut parts: Vec<String> = Vec::new();
        if note_diff > 0 {
            parts.push(format!("{} note change(s)", note_diff));
        }
        if accent_diff > 0 {
            parts.push(format!("{} accent change(s)", accent_diff));
        }
        if slide_diff > 0 {
            parts.push(format!("{} slide change(s)", slide_diff));
        }
        if transpose_diff > 0 {
            parts.push(format!("{} transpose change(s)", transpose_diff));
        }
        if time_diff > 0 {
            parts.push(format!("{} time-gate change(s)", time_diff));
        }
        if active_steps_diff {
            parts.push("active-step count differs".into());
        }
        if triplet_diff {
            parts.push("triplet flag differs".into());
        }
        if same_rhythm {
            parts.push("same rhythm".into());
        }
        parts.join(", ")
    };

    ItemCompareReport {
        note_diff,
        accent_diff,
        slide_diff,
        transpose_diff,
        time_diff,
        active_steps_diff,
        triplet_diff,
        summary,
        identical,
        differ_steps,
        same_rhythm,
        duplicate_score,
        relatedness_score,
    }
}

// ---------------------------------------------------------------------------
// Snapshot-vs-snapshot
// ---------------------------------------------------------------------------

/// Outcome of comparing a single slot between two snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotCompareState {
    /// Both sides have the same pattern content.
    Identical,
    /// Both sides have a pattern, but contents differ.
    Different,
    /// Only source side has a pattern.
    SourceOnly,
    /// Only target side has a pattern.
    TargetOnly,
    /// Both sides are empty.
    EmptyBoth,
}

/// Per-slot diff row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotCompareOutcome {
    pub slot_key: String,
    pub state: SlotCompareState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src_item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dst_item_id: Option<String>,
}

/// Full snapshot diff: one row per slot_key across the 64-slot grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotCompareReport {
    pub slots: Vec<SlotCompareOutcome>,
    pub identical_count: u32,
    pub different_count: u32,
    pub source_only_count: u32,
    pub target_only_count: u32,
    pub empty_both_count: u32,
}

/// Compare two slot grids. `resolve` is called to fetch a pattern for an
/// item id (so the caller decides how patterns are loaded - from the library,
/// from SysEx dumps, etc.). If `resolve` returns `None` for a non-empty slot,
/// the slot is treated as if the item had no resolvable pattern (counts as
/// `Different` when both sides are present).
pub fn compare_snapshots<F>(
    src: &[SnapshotSlot],
    dst: &[SnapshotSlot],
    resolve: F,
) -> SnapshotCompareReport
where
    F: Fn(&str) -> Option<Pattern>,
{
    let mut slots: Vec<SlotCompareOutcome> = Vec::with_capacity(64);
    let mut identical_count = 0u32;
    let mut different_count = 0u32;
    let mut source_only_count = 0u32;
    let mut target_only_count = 0u32;
    let mut empty_both_count = 0u32;

    for g in 1..=4u8 {
        for p in 1..=8u8 {
            for side in ['A', 'B'] {
                let slot_key = format!("G{}-P{}{}", g, p, side);
                let s = src.iter().find(|s| s.slot_key == slot_key);
                let d = dst.iter().find(|s| s.slot_key == slot_key);

                let src_empty = s.map(|x| x.empty || x.item_id.is_none()).unwrap_or(true);
                let dst_empty = d.map(|x| x.empty || x.item_id.is_none()).unwrap_or(true);

                let state = match (src_empty, dst_empty) {
                    (true, true) => {
                        empty_both_count += 1;
                        SlotCompareState::EmptyBoth
                    }
                    (false, true) => {
                        source_only_count += 1;
                        SlotCompareState::SourceOnly
                    }
                    (true, false) => {
                        target_only_count += 1;
                        SlotCompareState::TargetOnly
                    }
                    (false, false) => {
                        let src_pat = s.and_then(|x| x.item_id.as_deref()).and_then(&resolve);
                        let dst_pat = d.and_then(|x| x.item_id.as_deref()).and_then(&resolve);
                        match (src_pat, dst_pat) {
                            (Some(a), Some(b)) if compare_items(&a, &b).identical => {
                                identical_count += 1;
                                SlotCompareState::Identical
                            }
                            (Some(_), Some(_)) => {
                                different_count += 1;
                                SlotCompareState::Different
                            }
                            _ => {
                                different_count += 1;
                                SlotCompareState::Different
                            }
                        }
                    }
                };

                slots.push(SlotCompareOutcome {
                    slot_key,
                    state,
                    src_item_id: s.and_then(|x| x.item_id.clone()),
                    dst_item_id: d.and_then(|x| x.item_id.clone()),
                });
            }
        }
    }

    SnapshotCompareReport {
        slots,
        identical_count,
        different_count,
        source_only_count,
        target_only_count,
        empty_both_count,
    }
}

// ---------------------------------------------------------------------------
// Internal step accessors (Pattern fields are crate-visible)
// ---------------------------------------------------------------------------

fn step_at(p: &Pattern, idx: usize) -> step::Step {
    // Pattern::step is `pub(crate)`; we access via the crate-visible path.
    p.step[idx]
}

fn pattern_active_steps(p: &Pattern) -> u8 {
    p.active_steps
}

fn pattern_triplet(p: &Pattern) -> bool {
    p.triplet
}
