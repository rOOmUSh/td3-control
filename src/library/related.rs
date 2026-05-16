//! Related-group computation for the Bank Management UI.
//!
//! Pure function `compute_related_groups(&LibraryStore)` scans the catalog
//! and emits `RelatedGroup` rows according to five grouping rules:
//!
//! - `SameScale`          - items that share a non-None `scale_name`.
//! - `SameRoot`           - items that share a non-None `root_note`.
//! - `SameRhythm`         - items whose cached pattern bytes decode to the
//!   same `rhythm_fingerprint`.
//! - `AnalyzerRelated`    - reserved bucket for analyzer-generated
//!   `PatternRelation` rows. This module does not
//!   synthesize those groups yet, so the bucket is
//!   currently empty.
//! - `ProgressionFamily`  - items sharing a user tag whose label starts with
//!   either `progression:` or `family:`.
//!
//! Only groups with two or more members are emitted. Representatives are the
//! first four `item_id`s sorted by `updated_at` descending, falling back to
//! `created_at` when `updated_at` ties.
//!
//! The function is intentionally side-effect-free: the store is queried via
//! its existing public accessors and the function returns a fresh `Vec`. This
//! means handlers can call it without taking any lock of their own.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Td3Error;
use crate::pattern::sysex_to_pattern;

use super::duplicates::rhythm_fingerprint;
use super::filter::ItemFilter;
use super::model::LibraryItem;
use super::store::LibraryStore;

/// Classification of a related group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupKind {
    SameScale,
    SameRoot,
    SameRhythm,
    AnalyzerRelated,
    ProgressionFamily,
}

impl GroupKind {
    /// Parse the `?kind=` query value used by `/api/bank/related`.
    pub fn parse(s: &str) -> Option<GroupKind> {
        match s.trim().to_ascii_lowercase().as_str() {
            "same-scale" | "samescale" => Some(GroupKind::SameScale),
            "same-root" | "sameroot" => Some(GroupKind::SameRoot),
            "same-rhythm" | "samerhythm" => Some(GroupKind::SameRhythm),
            "analyzer-related" | "analyzerrelated" => Some(GroupKind::AnalyzerRelated),
            "progression-family" | "progressionfamily" => Some(GroupKind::ProgressionFamily),
            _ => None,
        }
    }
}

/// A single related group row. Deterministic field order + a stable
/// `group_id` means the UI can keep selection state across refreshes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedGroup {
    pub group_id: String,
    pub kind: GroupKind,
    pub label: String,
    pub reason: String,
    pub item_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_root: Option<String>,
    pub representative_ids: Vec<String>,
    pub item_count: u32,
}

/// Walk the catalog and produce every related group of ≥ 2 members.
///
/// The result is stable across runs: groups are emitted in a deterministic
/// order and each group's `item_ids` are sorted by `updated_at` desc (then by
/// `created_at` desc, then by id) before being trimmed.
pub fn compute_related_groups(store: &LibraryStore) -> Result<Vec<RelatedGroup>, Td3Error> {
    // Default filter = list everything including archived, so related
    // detection mirrors the broader comparison surfaces in the UI.
    let items = store.list_items(&ItemFilter::default())?;

    let mut out: Vec<RelatedGroup> = Vec::new();

    out.extend(same_scale_groups(&items));
    out.extend(same_root_groups(&items));
    out.extend(same_rhythm_groups(store, &items)?);
    // AnalyzerRelated is a deliberate no-op here. See file header.
    out.extend(progression_family_groups(&items));

    Ok(out)
}

// ---------------------------------------------------------------------------
// Rule: SameScale
// ---------------------------------------------------------------------------

fn same_scale_groups(items: &[LibraryItem]) -> Vec<RelatedGroup> {
    let mut buckets: BTreeMap<String, Vec<&LibraryItem>> = BTreeMap::new();
    for it in items {
        if let Some(scale) = &it.scale_name {
            if !scale.is_empty() {
                buckets.entry(scale.clone()).or_default().push(it);
            }
        }
    }
    buckets
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(scale, members)| {
            let sorted = sort_by_recency(members);
            let ids: Vec<String> = sorted.iter().map(|i| i.item_id.clone()).collect();
            let reps: Vec<String> = ids.iter().take(4).cloned().collect();
            let count = ids.len() as u32;
            RelatedGroup {
                group_id: format!("same-scale:{}", scale),
                kind: GroupKind::SameScale,
                label: format!("Scale: {}", scale),
                reason: format!("Same scale: {}", scale),
                item_ids: ids,
                primary_scale: Some(scale),
                primary_root: None,
                representative_ids: reps,
                item_count: count,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Rule: SameRoot
// ---------------------------------------------------------------------------

fn same_root_groups(items: &[LibraryItem]) -> Vec<RelatedGroup> {
    let mut buckets: BTreeMap<String, Vec<&LibraryItem>> = BTreeMap::new();
    for it in items {
        if let Some(root) = &it.root_note {
            if !root.is_empty() {
                buckets.entry(root.clone()).or_default().push(it);
            }
        }
    }
    buckets
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(root, members)| {
            let sorted = sort_by_recency(members);
            let ids: Vec<String> = sorted.iter().map(|i| i.item_id.clone()).collect();
            let reps: Vec<String> = ids.iter().take(4).cloned().collect();
            let count = ids.len() as u32;
            RelatedGroup {
                group_id: format!("same-root:{}", root),
                kind: GroupKind::SameRoot,
                label: format!("Root: {}", root),
                reason: format!("Same root: {}", root),
                item_ids: ids,
                primary_scale: None,
                primary_root: Some(root),
                representative_ids: reps,
                item_count: count,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Rule: SameRhythm
// ---------------------------------------------------------------------------

fn same_rhythm_groups(
    store: &LibraryStore,
    items: &[LibraryItem],
) -> Result<Vec<RelatedGroup>, Td3Error> {
    // (rhythm_hex, Vec<&item>) - we group by the 16-char fingerprint and skip
    // items whose sidecar is missing or unparseable.
    let mut buckets: BTreeMap<String, Vec<&LibraryItem>> = BTreeMap::new();
    for it in items {
        let Some(payload) = store.pattern_bytes_for(&it.item_id) else {
            continue;
        };
        if payload.len() != 112 {
            continue;
        }
        let mut sysex = Vec::with_capacity(115);
        sysex.push(0x78);
        sysex.push(0x00);
        sysex.push(0x00);
        sysex.extend_from_slice(&payload);
        let Ok(pat) = sysex_to_pattern(&sysex) else {
            continue;
        };
        let fp = rhythm_fingerprint(&pat);
        buckets.entry(fp).or_default().push(it);
    }
    let mut out: Vec<RelatedGroup> = Vec::new();
    for (fp, members) in buckets {
        if members.len() < 2 {
            continue;
        }
        let sorted = sort_by_recency(members);
        let ids: Vec<String> = sorted.iter().map(|i| i.item_id.clone()).collect();
        let reps: Vec<String> = ids.iter().take(4).cloned().collect();
        let count = ids.len() as u32;
        out.push(RelatedGroup {
            group_id: format!("same-rhythm:{}", fp),
            kind: GroupKind::SameRhythm,
            // 8-char display is enough to distinguish fingerprints in the UI
            // while keeping labels compact. Consumers who need the full
            // fingerprint can read `group_id`.
            label: format!("Rhythm: {}", &fp[..fp.len().min(8)]),
            reason: "Same rhythm pattern".to_string(),
            item_ids: ids,
            primary_scale: None,
            primary_root: None,
            representative_ids: reps,
            item_count: count,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Rule: ProgressionFamily
// ---------------------------------------------------------------------------

fn progression_family_groups(items: &[LibraryItem]) -> Vec<RelatedGroup> {
    let mut buckets: BTreeMap<String, Vec<&LibraryItem>> = BTreeMap::new();
    for it in items {
        for tag in &it.tags {
            if tag.starts_with("progression:") || tag.starts_with("family:") {
                buckets.entry(tag.clone()).or_default().push(it);
                // An item could legitimately carry several family/progression
                // tags. We include it in every matching bucket so each
                // progression group is surfaced.
            }
        }
    }
    buckets
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(tag, members)| {
            let sorted = sort_by_recency(members);
            let ids: Vec<String> = sorted.iter().map(|i| i.item_id.clone()).collect();
            let reps: Vec<String> = ids.iter().take(4).cloned().collect();
            let count = ids.len() as u32;
            let label = if let Some(rest) = tag.strip_prefix("progression:") {
                format!("Progression: {}", rest)
            } else if let Some(rest) = tag.strip_prefix("family:") {
                format!("Family: {}", rest)
            } else {
                tag.clone()
            };
            RelatedGroup {
                group_id: format!("progression:{}", tag),
                kind: GroupKind::ProgressionFamily,
                label,
                reason: format!("Shared tag: {}", tag),
                item_ids: ids,
                primary_scale: None,
                primary_root: None,
                representative_ids: reps,
                item_count: count,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sort members by `updated_at desc`, falling back to `created_at desc`, then
/// by `item_id` for a stable tie-break. Returns owned order.
fn sort_by_recency(mut members: Vec<&LibraryItem>) -> Vec<&LibraryItem> {
    members.sort_by(|a, b| {
        let ua = a.updated_at.as_str();
        let ub = b.updated_at.as_str();
        match ub.cmp(ua) {
            std::cmp::Ordering::Equal => {
                let ca = a.created_at.as_str();
                let cb = b.created_at.as_str();
                match cb.cmp(ca) {
                    std::cmp::Ordering::Equal => a.item_id.cmp(&b.item_id),
                    other => other,
                }
            }
            other => other,
        }
    });
    members
}
